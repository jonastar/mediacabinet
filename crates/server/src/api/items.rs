//! Movies, search, artwork, and changes to items: edits, locks, tags, refresh
//! and re-matching.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::Response;
use cabinet_core::catalog::movies::{self, MovieDetail, MovieSummary, SearchResults};
use cabinet_core::catalog::{self, ViewRef};
use cabinet_core::items::{self, ItemEdit, ItemError};
use cabinet_core::metadata::import::ImportReport;
use cabinet_core::metadata::refresh::{self, RefreshError};
use cabinet_core::scan::Trigger;
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};

use super::ApiError;
use super::auth::Viewer;
use super::catalog::LibraryQuery;
use super::error::ErrorBody;
use crate::AppState;

impl From<ItemError> for ApiError {
    fn from(err: ItemError) -> Self {
        match err {
            ItemError::NotFound => ApiError::NotFound,
            ItemError::Invalid(m) => ApiError::BadRequest(m),
            ItemError::Provider(m) => ApiError::Unavailable(m),
            ItemError::Db(e) => ApiError::Internal(e.to_string()),
        }
    }
}

impl From<RefreshError> for ApiError {
    fn from(err: RefreshError) -> Self {
        match err {
            RefreshError::NotFound => ApiError::NotFound,
            RefreshError::WrongProvider(m) => ApiError::BadRequest(m),
            RefreshError::Provider(e) => ApiError::Unavailable(e.to_string()),
            RefreshError::Import(m) => ApiError::Conflict(m),
            RefreshError::Db(e) => ApiError::Internal(e.to_string()),
        }
    }
}

/// All movies, by title.
#[utoipa::path(
    get,
    path = "/movies",
    tag = "catalog",
    params(LibraryQuery),
    responses((status = 200, body = Vec<MovieSummary>))
)]
pub async fn list_movies(
    State(state): State<AppState>,
    viewer: Viewer,
    Query(query): Query<LibraryQuery>,
) -> Result<Json<Vec<MovieSummary>>, ApiError> {
    let user = Some(viewer.0);
    Ok(Json(
        movies::list_movies(&state.db, user, query.library).await?,
    ))
}

/// A movie with its files (versions, editions and parts).
#[utoipa::path(
    get,
    path = "/movies/{id}",
    tag = "catalog",
    params(("id" = i64, Path)),
    responses((status = 200, body = MovieDetail), (status = 404, body = ErrorBody))
)]
pub async fn movie_detail(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i64>,
) -> Result<Json<MovieDetail>, ApiError> {
    let user = Some(viewer.0);
    Ok(Json(movies::movie_detail(&state.db, id, user).await?))
}

#[derive(Deserialize, IntoParams)]
pub struct SearchQuery {
    q: String,
}

/// Searches the library by title: series, movies and episodes.
#[utoipa::path(get, path = "/search", tag = "catalog", params(SearchQuery), responses((status = 200, body = SearchResults)))]
pub async fn search(
    State(state): State<AppState>,
    viewer: Viewer,
    Query(query): Query<SearchQuery>,
) -> Result<Json<SearchResults>, ApiError> {
    let user = Some(viewer.0);
    Ok(Json(
        movies::search(&state.db, &query.q, user, catalog::now()).await?,
    ))
}

/// An item's artwork (poster, backdrop or still), cached locally after first use.
#[utoipa::path(
    get,
    path = "/artwork/{id}/{kind}",
    tag = "catalog",
    params(("id" = i64, Path), ("kind" = String, Path)),
    responses((status = 200, content_type = "image/*"), (status = 404, body = ErrorBody))
)]
pub async fn artwork(
    State(state): State<AppState>,
    Path((id, kind)): Path<(i64, String)>,
) -> Result<Response, ApiError> {
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT provider, path FROM item_artwork WHERE item_id = ? AND kind = ?")
            .bind(id)
            .bind(&kind)
            .fetch_optional(&state.db)
            .await?;
    let (provider, path) = row.ok_or(ApiError::NotFound)?;
    state.artwork.get(&provider, &kind, &path).await
}

/// Edits a series', episode's or movie's details, optionally locking them against refresh.
#[utoipa::path(
    patch,
    path = "/items/{id}",
    tag = "items",
    params(("id" = i64, Path)),
    request_body = ItemEdit,
    responses((status = 204), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody))
)]
pub async fn edit_item(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<ItemEdit>,
) -> Result<StatusCode, ApiError> {
    items::edit(&state.db, id, body).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, ToSchema)]
pub struct Locks {
    /// Fields refreshes must not overwrite; empty unlocks all.
    fields: Vec<String>,
}

#[utoipa::path(
    put,
    path = "/items/{id}/locks",
    tag = "items",
    params(("id" = i64, Path)),
    request_body = Locks,
    responses((status = 204), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody))
)]
pub async fn set_locks(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<Locks>,
) -> Result<StatusCode, ApiError> {
    items::set_locks(&state.db, id, &body.fields).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, ToSchema)]
pub struct Tags {
    /// Who sets them, e.g. `animefillerlist`. Replaces that source's tags only.
    source: String,
    tags: Vec<String>,
}

/// Sets an item's tags from one source; external tools use this for filler lists.
#[utoipa::path(
    put,
    path = "/items/{id}/tags",
    tag = "items",
    params(("id" = i64, Path)),
    request_body = Tags,
    responses((status = 204), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody))
)]
pub async fn set_tags(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<Tags>,
) -> Result<StatusCode, ApiError> {
    items::set_tags(&state.db, id, &body.source, &body.tags).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Fetches a series' or movie's metadata from its provider now.
#[utoipa::path(
    post,
    path = "/items/{id}/refresh",
    tag = "items",
    params(("id" = i64, Path)),
    responses(
        (status = 200, body = ImportReport),
        (status = 400, body = ErrorBody),
        (status = 404, body = ErrorBody),
        (status = 503, body = ErrorBody)
    )
)]
pub async fn refresh_item(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<ImportReport>, ApiError> {
    Ok(Json(
        refresh::refresh_item(&state.db, &state.provider, id).await?,
    ))
}

#[derive(Deserialize, ToSchema)]
pub struct MatchOrdering {
    /// `ordering:<id>` of this series, or null to try the default first.
    view: Option<String>,
}

/// Sets which ordering filenames of this series are numbered in.
#[utoipa::path(
    put,
    path = "/series/{id}/match-ordering",
    tag = "items",
    params(("id" = i64, Path)),
    request_body = MatchOrdering,
    responses((status = 204), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody))
)]
pub async fn set_match_ordering(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<MatchOrdering>,
) -> Result<StatusCode, ApiError> {
    let ordering = match body.view.as_deref().map(ViewRef::parse) {
        None => None,
        Some(Some(ViewRef::Ordering(o))) => Some(o),
        Some(_) => {
            return Err(ApiError::BadRequest(
                "Use ordering:<id>; playlists can't number files.".into(),
            ));
        }
    };
    items::set_match_ordering(&state.db, id, ordering).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, ToSchema)]
pub struct Rematch {
    /// The provider's ID for the right series.
    provider_id: String,
}

/// Points a series at a different provider entry and refreshes it from there.
#[utoipa::path(
    post,
    path = "/series/{id}/rematch",
    tag = "items",
    params(("id" = i64, Path)),
    request_body = Rematch,
    responses((status = 200, body = ImportReport), (status = 400, body = ErrorBody), (status = 503, body = ErrorBody))
)]
pub async fn rematch_series(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<Rematch>,
) -> Result<Json<ImportReport>, ApiError> {
    Ok(Json(
        items::rematch_series(&state.db, &state.provider, id, &body.provider_id).await?,
    ))
}

/// Rescans every root holding one of the series' folders.
#[utoipa::path(
    post,
    path = "/series/{id}/rescan",
    tag = "items",
    params(("id" = i64, Path)),
    responses((status = 202), (status = 404, body = ErrorBody))
)]
pub async fn rescan_series(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    let roots: Vec<i64> =
        sqlx::query_scalar("SELECT DISTINCT root_id FROM series_location WHERE series_item_id = ?")
            .bind(id)
            .fetch_all(&state.db)
            .await?;
    if roots.is_empty() {
        return Err(ApiError::NotFound);
    }
    for root in roots {
        state.scans.request(root, Trigger::Manual);
    }
    Ok(StatusCode::ACCEPTED)
}
