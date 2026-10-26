//! Playlists: smart (an ordering minus tagged episodes) or manual.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use cabinet_core::playlists::{self, Playlist, PlaylistError, PlaylistInput};
use serde::Deserialize;
use utoipa::IntoParams;

use super::ApiError;
use super::auth::{Actor, Viewer};
use super::error::ErrorBody;
use crate::AppState;

impl From<PlaylistError> for ApiError {
    fn from(err: PlaylistError) -> Self {
        match err {
            PlaylistError::NotFound => ApiError::NotFound,
            PlaylistError::Forbidden => ApiError::Forbidden,
            PlaylistError::Invalid(m) => ApiError::BadRequest(m),
            PlaylistError::Db(e) => ApiError::Internal(e.to_string()),
        }
    }
}

#[derive(Deserialize, IntoParams)]
pub struct PlaylistQuery {
    /// Only this series' playlists.
    series_id: Option<i64>,
}

/// Shared playlists and your own.
#[utoipa::path(
    get,
    path = "/playlists",
    tag = "playlists",
    params(PlaylistQuery),
    responses((status = 200, body = Vec<Playlist>))
)]
pub async fn list(
    State(state): State<AppState>,
    Viewer(user): Viewer,
    Query(query): Query<PlaylistQuery>,
) -> Result<Json<Vec<Playlist>>, ApiError> {
    Ok(Json(
        playlists::list(&state.db, user, query.series_id).await?,
    ))
}

/// Creates a playlist: yours, or shared (admins only).
#[utoipa::path(
    post,
    path = "/playlists",
    tag = "playlists",
    request_body = PlaylistInput,
    responses((status = 201, body = Playlist), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody))
)]
pub async fn create(
    State(state): State<AppState>,
    Actor(actor): Actor,
    Json(body): Json<PlaylistInput>,
) -> Result<(StatusCode, Json<Playlist>), ApiError> {
    Ok((
        StatusCode::CREATED,
        Json(playlists::create(&state.db, actor, body).await?),
    ))
}

/// A playlist you can see.
#[utoipa::path(
    get,
    path = "/playlists/{id}",
    tag = "playlists",
    params(("id" = i64, Path)),
    responses((status = 200, body = Playlist), (status = 404, body = ErrorBody))
)]
pub async fn get(
    State(state): State<AppState>,
    Viewer(user): Viewer,
    Path(id): Path<i64>,
) -> Result<Json<Playlist>, ApiError> {
    Ok(Json(playlists::get(&state.db, user, id).await?))
}

/// Renames a playlist or changes its rules or episodes.
#[utoipa::path(
    patch,
    path = "/playlists/{id}",
    tag = "playlists",
    params(("id" = i64, Path)),
    request_body = PlaylistInput,
    responses(
        (status = 200, body = Playlist),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody)
    )
)]
pub async fn update(
    State(state): State<AppState>,
    Actor(actor): Actor,
    Path(id): Path<i64>,
    Json(body): Json<PlaylistInput>,
) -> Result<Json<Playlist>, ApiError> {
    Ok(Json(playlists::update(&state.db, actor, id, body).await?))
}

/// Deletes a playlist. Anyone following it goes back to the default ordering.
#[utoipa::path(
    delete,
    path = "/playlists/{id}",
    tag = "playlists",
    params(("id" = i64, Path)),
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody))
)]
pub async fn delete(
    State(state): State<AppState>,
    Actor(actor): Actor,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    playlists::delete(&state.db, actor, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
