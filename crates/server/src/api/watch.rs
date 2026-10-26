//! Watch state for the signed-in user (or, for an API key, the first admin).

use axum::Json;
use axum::extract::{Path, State};
use cabinet_core::catalog;
use cabinet_core::watch::{self, ItemWatch, Scope, WatchError};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::ApiError;
use super::auth::Viewer;
use super::error::ErrorBody;
use crate::AppState;

impl From<WatchError> for ApiError {
    fn from(err: WatchError) -> Self {
        match err {
            WatchError::NotFound => ApiError::NotFound,
            WatchError::Invalid(m) => ApiError::BadRequest(m),
            WatchError::Db(e) => ApiError::Internal(e.to_string()),
        }
    }
}

#[derive(Deserialize, ToSchema)]
pub struct Progress {
    position_seconds: f64,
    /// What the player knows; otherwise the file's probed duration is used.
    duration_seconds: Option<f64>,
}

/// Reports playback progress for an episode or movie. Past 90% it counts as
/// played and the resume position clears.
#[utoipa::path(
    put,
    path = "/items/{id}/progress",
    tag = "watch",
    params(("id" = i64, Path)),
    request_body = Progress,
    responses((status = 200, body = ItemWatch), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody))
)]
pub async fn report_progress(
    State(state): State<AppState>,
    Viewer(user): Viewer,
    Path(id): Path<i64>,
    Json(body): Json<Progress>,
) -> Result<Json<ItemWatch>, ApiError> {
    Ok(Json(
        watch::report_progress(
            &state.db,
            user,
            id,
            body.position_seconds,
            body.duration_seconds,
            catalog::now(),
        )
        .await?,
    ))
}

#[derive(Deserialize, ToSchema)]
pub struct SetPlayed {
    played: bool,
    /// For a series: only this season (an ordering season's ID).
    season_id: Option<i64>,
    /// For a series: only these of its episodes.
    episodes: Option<Vec<i64>>,
}

#[derive(Serialize, ToSchema)]
pub struct PlayedChanged {
    changed: u64,
}

/// Marks an episode, movie, series or one season as played or unplayed.
#[utoipa::path(
    put,
    path = "/items/{id}/played",
    tag = "watch",
    params(("id" = i64, Path)),
    request_body = SetPlayed,
    responses((status = 200, body = PlayedChanged), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody))
)]
pub async fn set_played(
    State(state): State<AppState>,
    Viewer(user): Viewer,
    Path(id): Path<i64>,
    Json(body): Json<SetPlayed>,
) -> Result<Json<PlayedChanged>, ApiError> {
    let scope = match (body.season_id, body.episodes) {
        (Some(season), None) => Scope::Season(season),
        (None, Some(episodes)) => Scope::Episodes(episodes),
        (None, None) => Scope::All,
        (Some(_), Some(_)) => {
            return Err(ApiError::BadRequest(
                "Give a season or episodes, not both.".into(),
            ));
        }
    };
    let changed =
        watch::set_played(&state.db, user, id, scope, body.played, catalog::now()).await?;
    Ok(Json(PlayedChanged { changed }))
}
