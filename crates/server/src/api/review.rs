//! The review queue and metadata search.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use cabinet_core::matcher::review::{self, ReviewError, ReviewQueue};
use cabinet_core::metadata::{MetadataError, SearchResult};
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};

use super::ApiError;
use super::error::ErrorBody;
use crate::AppState;

impl From<ReviewError> for ApiError {
    fn from(err: ReviewError) -> Self {
        match err {
            ReviewError::NotFound => ApiError::NotFound,
            ReviewError::Invalid(m) => ApiError::BadRequest(m),
            ReviewError::Provider(m) => ApiError::Unavailable(m),
            ReviewError::Db(e) => ApiError::Internal(e.to_string()),
        }
    }
}

impl From<MetadataError> for ApiError {
    fn from(err: MetadataError) -> Self {
        match err {
            MetadataError::NotFound => ApiError::NotFound,
            e => ApiError::Unavailable(e.to_string()),
        }
    }
}

/// Folders to identify, files to confirm, and files nothing matched.
#[utoipa::path(get, path = "/review", tag = "review", responses((status = 200, body = ReviewQueue)))]
pub async fn queue(State(state): State<AppState>) -> Result<Json<ReviewQueue>, ApiError> {
    Ok(Json(review::queue(&state.db).await?))
}

#[derive(Deserialize, ToSchema)]
pub struct Accept {
    /// Pin the resulting link so rescans never change it.
    #[serde(default)]
    pin: bool,
}

/// Accepts a candidate: links the file, or identifies the folder and matches its files.
#[utoipa::path(
    post,
    path = "/review/candidates/{id}/accept",
    tag = "review",
    params(("id" = i64, Path)),
    request_body = Accept,
    responses(
        (status = 204),
        (status = 400, body = ErrorBody),
        (status = 404, body = ErrorBody),
        (status = 503, body = ErrorBody)
    )
)]
pub async fn accept(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<Accept>,
) -> Result<StatusCode, ApiError> {
    review::accept(
        &state.db,
        id,
        body.pin,
        &state.provider,
        state.match_settings,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/review/candidates/{id}/reject",
    tag = "review",
    params(("id" = i64, Path)),
    responses((status = 204), (status = 404, body = ErrorBody))
)]
pub async fn reject(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    review::reject(&state.db, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, ToSchema)]
pub struct LinkFile {
    /// Episodes (several for a multi-episode file) or a movie.
    item_ids: Vec<i64>,
    #[serde(default)]
    pin: bool,
}

/// Links a file to items chosen by hand.
#[utoipa::path(
    post,
    path = "/files/{id}/link",
    tag = "review",
    params(("id" = i64, Path)),
    request_body = LinkFile,
    responses((status = 204), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody))
)]
pub async fn link_file(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<LinkFile>,
) -> Result<StatusCode, ApiError> {
    review::link_file(&state.db, id, &body.item_ids, body.pin).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Removes a link: the file isn't that item. Scans won't link them again, and
/// the file goes to review if nothing else links it.
#[utoipa::path(
    delete,
    path = "/files/{id}/links/{item}",
    tag = "review",
    params(("id" = i64, Path), ("item" = i64, Path)),
    responses((status = 204), (status = 404, body = ErrorBody))
)]
pub async fn unlink_file(
    State(state): State<AppState>,
    Path((id, item)): Path<(i64, i64)>,
) -> Result<StatusCode, ApiError> {
    review::unlink(&state.db, id, item).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, ToSchema)]
pub struct EditLink {
    /// Which edition of the item the file is, such as "Director's Cut"; null or
    /// empty for the only or usual one.
    edition: Option<String>,
}

/// Changes a link: which edition the file is.
#[utoipa::path(
    patch,
    path = "/files/{id}/links/{item}",
    tag = "review",
    params(("id" = i64, Path), ("item" = i64, Path)),
    request_body = EditLink,
    responses((status = 204), (status = 404, body = ErrorBody))
)]
pub async fn edit_link(
    State(state): State<AppState>,
    Path((id, item)): Path<(i64, i64)>,
    Json(body): Json<EditLink>,
) -> Result<StatusCode, ApiError> {
    review::set_edition(&state.db, id, item, body.edition.as_deref()).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Excludes a file from matching (samples, junk).
#[utoipa::path(
    post,
    path = "/files/{id}/ignore",
    tag = "review",
    params(("id" = i64, Path)),
    responses((status = 204), (status = 404, body = ErrorBody))
)]
pub async fn ignore_file(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    review::ignore_file(&state.db, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, ToSchema)]
pub struct IdentifyFolder {
    root_id: i64,
    folder_path: String,
    /// The provider's ID for the series or movie.
    provider_id: String,
}

/// Identifies a folder by hand: imports the chosen series or movie and matches its files.
#[utoipa::path(
    post,
    path = "/review/folders/identify",
    tag = "review",
    request_body = IdentifyFolder,
    responses((status = 204), (status = 404, body = ErrorBody), (status = 503, body = ErrorBody))
)]
pub async fn identify_folder(
    State(state): State<AppState>,
    Json(body): Json<IdentifyFolder>,
) -> Result<StatusCode, ApiError> {
    review::identify_as(
        &state.db,
        body.root_id,
        &body.folder_path,
        &body.provider_id,
        &state.provider,
        state.match_settings,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, IntoParams)]
pub struct SearchQuery {
    /// `tv` or `movies`.
    kind: String,
    query: String,
    year: Option<i64>,
}

/// Searches the metadata provider.
#[utoipa::path(
    get,
    path = "/metadata/search",
    tag = "review",
    params(SearchQuery),
    responses((status = 200, body = Vec<SearchResult>), (status = 503, body = ErrorBody))
)]
pub async fn search(
    State(state): State<AppState>,
    Query(q): Query<SearchQuery>,
) -> Result<Json<Vec<SearchResult>>, ApiError> {
    let results = match q.kind.as_str() {
        "movies" => state.provider.search_movies(&q.query, q.year).await?,
        _ => state.provider.search_series(&q.query, q.year).await?,
    };
    Ok(Json(results))
}
