//! Wanted items, the calendar, custom status and monitoring settings.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use cabinet_core::catalog::{self, QualityProfile, ViewRef};
use cabinet_core::wanted::{
    self, CalendarEntry, StatusUpdate, WantedError, WantedFilter, WantedPage,
};
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};

use super::ApiError;
use super::error::ErrorBody;
use crate::AppState;

impl From<WantedError> for ApiError {
    fn from(err: WantedError) -> Self {
        match err {
            WantedError::NotFound => ApiError::NotFound,
            WantedError::Invalid(m) => ApiError::BadRequest(m),
            WantedError::Db(e) => ApiError::Internal(e.to_string()),
        }
    }
}

/// Wanted episodes and movies with why they're wanted, their quality target and
/// any external status. Filterable by library, series, reason and kind.
#[utoipa::path(
    get,
    path = "/wanted",
    tag = "wanted",
    params(WantedFilter),
    responses((status = 200, body = WantedPage), (status = 400, body = ErrorBody))
)]
pub async fn list(
    State(state): State<AppState>,
    Query(filter): Query<WantedFilter>,
) -> Result<Json<WantedPage>, ApiError> {
    Ok(Json(
        wanted::list(&state.db, &filter, catalog::now()).await?,
    ))
}

#[derive(Deserialize, IntoParams)]
pub struct CalendarQuery {
    /// Unix seconds; defaults to 7 days ago.
    from: Option<i64>,
    /// Unix seconds; defaults to 28 days from now.
    to: Option<i64>,
}

/// Episodes and movie releases in a time range, by air time.
#[utoipa::path(
    get,
    path = "/calendar",
    tag = "wanted",
    params(CalendarQuery),
    responses((status = 200, body = Vec<CalendarEntry>), (status = 400, body = ErrorBody))
)]
pub async fn calendar(
    State(state): State<AppState>,
    Query(q): Query<CalendarQuery>,
) -> Result<Json<Vec<CalendarEntry>>, ApiError> {
    let now = catalog::now();
    let from = q.from.unwrap_or(now - 7 * 86400);
    let to = q.to.unwrap_or(now + 28 * 86400);
    Ok(Json(wanted::calendar(&state.db, from, to).await?))
}

/// Sets an external tool's status for an item, e.g. Sonarr reporting a download.
#[utoipa::path(
    put,
    path = "/items/{id}/status/{source}",
    tag = "wanted",
    params(("id" = String, Path, description = "A reference: an item ID, item:<id>, or <provider>:<id> such as tmdb:57243"), ("source" = String, Path), super::integration::PositionQuery),
    request_body = StatusUpdate,
    responses((status = 204), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody))
)]
pub async fn set_status(
    State(state): State<AppState>,
    Path((r, source)): Path<(String, String)>,
    Query(position): Query<super::integration::PositionQuery>,
    Json(body): Json<StatusUpdate>,
) -> Result<StatusCode, ApiError> {
    let id = super::integration::resolve(&state, &r, &position).await?;
    wanted::set_status(&state.db, id, &source, &body).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Clears an external tool's status for an item.
#[utoipa::path(
    delete,
    path = "/items/{id}/status/{source}",
    tag = "wanted",
    params(("id" = String, Path, description = "A reference: an item ID or <provider>:<id>"), ("source" = String, Path), super::integration::PositionQuery),
    responses((status = 204), (status = 404, body = ErrorBody))
)]
pub async fn clear_status(
    State(state): State<AppState>,
    Path((id, source)): Path<(i64, String)>,
) -> Result<StatusCode, ApiError> {
    wanted::clear_status(&state.db, id, &source).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Monitoring settings. Leave a field out to keep it; send null to clear it.
#[derive(Deserialize, ToSchema)]
pub struct Monitoring {
    /// A quality profile ID, or null for none.
    #[schema(value_type = Option<i64>)]
    #[serde(default, deserialize_with = "present")]
    quality_profile_id: Option<Option<i64>>,
    /// Series only: `playlist:<id>` that wanted follows, or null for all episodes.
    #[schema(value_type = Option<String>)]
    #[serde(default, deserialize_with = "present")]
    wanted_follows: Option<Option<String>>,
}

/// Tells a field that's present (even as null) from one that's absent.
fn present<'de, T, D>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

/// Sets a series' or movie's quality profile, and what a series' wanted follows.
#[utoipa::path(
    put,
    path = "/items/{id}/monitoring",
    tag = "wanted",
    params(("id" = i64, Path)),
    request_body = Monitoring,
    responses((status = 204), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody))
)]
pub async fn set_monitoring(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<Monitoring>,
) -> Result<StatusCode, ApiError> {
    let playlist = match body.wanted_follows {
        None => None,
        Some(None) => Some(None),
        Some(Some(view)) => match ViewRef::parse(&view) {
            Some(ViewRef::Playlist(p)) => Some(Some(p)),
            _ => {
                return Err(ApiError::BadRequest(
                    "wanted_follows is playlist:<id> or null".into(),
                ));
            }
        },
    };
    wanted::set_monitoring(&state.db, id, body.quality_profile_id, playlist).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Quality profiles: a resolution cutoff below which files are wanted as upgrades.
#[utoipa::path(get, path = "/quality-profiles", tag = "wanted", responses((status = 200, body = Vec<QualityProfile>)))]
pub async fn quality_profiles(
    State(state): State<AppState>,
) -> Result<Json<Vec<QualityProfile>>, ApiError> {
    let rows: Vec<(i64, String)> =
        sqlx::query_as("SELECT id, name FROM quality_profile ORDER BY id")
            .fetch_all(&state.db)
            .await?;
    Ok(Json(
        rows.into_iter()
            .map(|(id, name)| QualityProfile { id, name })
            .collect(),
    ))
}
