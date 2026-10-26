//! Read API for the web UI, plus the two settings it changes in place.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use cabinet_core::catalog::{self, EpisodeList, Home, SeriesDetail, SeriesSummary, ViewRef};
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};

use super::ApiError;
use super::auth::Viewer;
use super::error::ErrorBody;
use crate::AppState;

fn parse_view(view: &str) -> Result<ViewRef, ApiError> {
    ViewRef::parse(view).ok_or_else(|| {
        ApiError::BadRequest(format!(
            "invalid view {view:?}; expected ordering:<id> or playlist:<id>"
        ))
    })
}

/// Everything the home screen shows: continue watching, coming up, shows, and
/// what needs attention.
#[utoipa::path(get, path = "/home", tag = "catalog", responses((status = 200, body = Home)))]
pub async fn home(State(state): State<AppState>, viewer: Viewer) -> Result<Json<Home>, ApiError> {
    let user = Some(viewer.0);
    Ok(Json(catalog::home(&state.db, user, catalog::now()).await?))
}

/// All series, by title.
#[utoipa::path(get, path = "/series", tag = "catalog", responses((status = 200, body = Vec<SeriesSummary>)))]
pub async fn list_series(
    State(state): State<AppState>,
    viewer: Viewer,
) -> Result<Json<Vec<SeriesSummary>>, ApiError> {
    let user = Some(viewer.0);
    Ok(Json(
        catalog::list_series(&state.db, user, catalog::now()).await?,
    ))
}

/// A series with its orderings, playlists, metadata and locations.
#[utoipa::path(
    get,
    path = "/series/{id}",
    tag = "catalog",
    params(("id" = i64, Path)),
    responses((status = 200, body = SeriesDetail), (status = 404, body = ErrorBody))
)]
pub async fn series_detail(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i64>,
) -> Result<Json<SeriesDetail>, ApiError> {
    let user = Some(viewer.0);
    Ok(Json(
        catalog::series_detail(&state.db, id, user, catalog::now()).await?,
    ))
}

#[derive(Deserialize, IntoParams)]
pub struct EpisodesQuery {
    /// `ordering:<id>` or `playlist:<id>`. Defaults to the user's view, then the default ordering.
    view: Option<String>,
}

/// A series' episodes in one ordering or playlist, with availability, files,
/// watch state, gaps and up next.
#[utoipa::path(
    get,
    path = "/series/{id}/episodes",
    tag = "catalog",
    params(("id" = i64, Path), EpisodesQuery),
    responses(
        (status = 200, body = EpisodeList),
        (status = 400, body = ErrorBody),
        (status = 404, body = ErrorBody)
    )
)]
pub async fn series_episodes(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i64>,
    Query(query): Query<EpisodesQuery>,
) -> Result<Json<EpisodeList>, ApiError> {
    let view = query.view.as_deref().map(parse_view).transpose()?;
    let user = Some(viewer.0);
    Ok(Json(
        catalog::episodes(&state.db, id, view, user, catalog::now()).await?,
    ))
}

#[derive(Deserialize, ToSchema)]
pub struct SetView {
    /// `ordering:<id>` or `playlist:<id>` of this series.
    view: String,
}

/// Sets the ordering or playlist that drives the user's "next up" for a series.
#[utoipa::path(
    put,
    path = "/series/{id}/view",
    tag = "catalog",
    params(("id" = i64, Path)),
    request_body = SetView,
    responses(
        (status = 204),
        (status = 400, body = ErrorBody),
        (status = 404, body = ErrorBody)
    )
)]
pub async fn set_series_view(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i64>,
    Json(body): Json<SetView>,
) -> Result<StatusCode, ApiError> {
    let view = parse_view(&body.view)?;
    let Viewer(user) = viewer;
    catalog::set_view(&state.db, id, user, view).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, ToSchema)]
pub struct SetMonitor {
    /// `null` inherits from the parent (an episode from its series).
    monitored: Option<bool>,
}

/// Sets whether a series or episode is monitored. Emits `item.wanted` and
/// `item.unwanted` for episodes whose wanted state changes.
#[utoipa::path(
    put,
    path = "/items/{id}/monitor",
    tag = "catalog",
    params(("id" = i64, Path)),
    request_body = SetMonitor,
    responses((status = 204), (status = 404, body = ErrorBody))
)]
pub async fn set_item_monitor(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<SetMonitor>,
) -> Result<StatusCode, ApiError> {
    catalog::set_monitored(&state.db, id, body.monitored).await?;
    Ok(StatusCode::NO_CONTENT)
}
