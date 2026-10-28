//! The integration API for external tools: item lookup by reference, gaps,
//! adding series and movies, API keys, and *arr compatibility.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use cabinet_core::apikeys::{self, ApiKey, NewApiKey};
use cabinet_core::arr::{self, RadarrListItem, SonarrListItem};
use cabinet_core::catalog::{self, Episode, ViewRef};
use cabinet_core::items;
use cabinet_core::metadata::import::ImportReport;
use cabinet_core::refs::{self, ItemRef, ItemView, Position, RefError};
use cabinet_core::tasks::Trigger;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use super::ApiError;
use super::auth::ApiKeyAuth;
use super::error::ErrorBody;
use crate::AppState;

impl From<RefError> for ApiError {
    fn from(err: RefError) -> Self {
        match err {
            RefError::NotFound => ApiError::NotFound,
            RefError::Invalid(m) => ApiError::BadRequest(m),
            RefError::Db(e) => ApiError::Internal(e.to_string()),
        }
    }
}

#[derive(Deserialize, IntoParams)]
pub struct PositionQuery {
    /// Ordering kind or name for `episode`: aired, dvd, absolute… or default.
    ordering: Option<String>,
    /// `S02E05`, `2x05` or an absolute number, within the series in `ref`.
    episode: Option<String>,
}

pub(crate) async fn resolve(state: &AppState, r: &str, q: &PositionQuery) -> Result<i64, ApiError> {
    let item = ItemRef::parse(r)?;
    let position = match &q.episode {
        Some(e) => Some(Position::parse(
            q.ordering.as_deref().unwrap_or("default"),
            e,
        )?),
        None => None,
    };
    Ok(refs::resolve(&state.db, &item, position.as_ref()).await?)
}

/// An item by reference: `item:<id>`, or `<provider>:<id>` with an optional
/// position (`?ordering=aired&episode=S02E05`). Availability, wanted state, its
/// number in every ordering, files and statuses.
#[utoipa::path(
    get,
    path = "/items/{id}",
    tag = "integration",
    params(("id" = String, Path, description = "A reference: an item ID, item:<id>, or <provider>:<id> such as tmdb:57243"), PositionQuery),
    responses((status = 200, body = ItemView), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody))
)]
pub async fn item(
    State(state): State<AppState>,
    Path(r): Path<String>,
    Query(q): Query<PositionQuery>,
) -> Result<Json<ItemView>, ApiError> {
    let id = resolve(&state, &r, &q).await?;
    Ok(Json(refs::item_view(&state.db, id, catalog::now()).await?))
}

#[derive(Deserialize, IntoParams)]
pub struct GapsQuery {
    /// `ordering:<id>` or `playlist:<id>`; default ordering if absent.
    view: Option<String>,
}

/// Episodes missing before an available one, in an ordering or playlist.
#[utoipa::path(
    get,
    path = "/series/{id}/gaps",
    tag = "integration",
    params(("id" = String, Path, description = "A reference: an item ID or <provider>:<id>"), GapsQuery),
    responses((status = 200, body = Vec<Episode>), (status = 404, body = ErrorBody))
)]
pub async fn gaps(
    State(state): State<AppState>,
    Path(r): Path<String>,
    Query(q): Query<GapsQuery>,
) -> Result<Json<Vec<Episode>>, ApiError> {
    let id = refs::resolve(&state.db, &ItemRef::parse(&r)?, None).await?;
    let view = match q.view.as_deref() {
        Some(v) => Some(ViewRef::parse(v).ok_or_else(|| {
            ApiError::BadRequest("view is ordering:<id> or playlist:<id>".into())
        })?),
        None => None,
    };
    let default_view = match view {
        Some(v) => Some(v),
        None => catalog::series_detail(&state.db, id, None, catalog::now())
            .await?
            .views
            .iter()
            .find(|v| v.is_default)
            .and_then(|v| ViewRef::parse(&v.id)),
    };
    let list = catalog::episodes(&state.db, id, default_view, None, catalog::now()).await?;
    Ok(Json(
        list.entries.into_iter().filter(|e| e.is_gap).collect(),
    ))
}

#[derive(Deserialize, ToSchema)]
pub struct AddItem {
    /// The metadata provider's ID (TMDB).
    provider_id: String,
    /// The library to add it to. Defaults to the library of that kind with an
    /// import target, else the first one.
    library_id: Option<i64>,
    #[serde(default = "yes")]
    monitored: bool,
    quality_profile_id: Option<i64>,
}

fn yes() -> bool {
    true
}

/// Adds a series by provider ID: imports every episode and its orderings, and
/// monitors it. Files arrive later, by scan or import.
#[utoipa::path(
    post,
    path = "/series",
    tag = "integration",
    request_body = AddItem,
    responses((status = 201, body = ImportReport), (status = 400, body = ErrorBody), (status = 503, body = ErrorBody))
)]
pub async fn add_series(
    State(state): State<AppState>,
    Json(body): Json<AddItem>,
) -> Result<(StatusCode, Json<ImportReport>), ApiError> {
    let r = items::add(
        &state.db,
        &state.provider,
        "series",
        &body.provider_id,
        body.library_id,
        body.monitored,
        body.quality_profile_id,
    )
    .await?;
    Ok((
        if r.created {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        },
        Json(r),
    ))
}

/// Adds a movie by provider ID and monitors it.
#[utoipa::path(
    post,
    path = "/movies",
    tag = "integration",
    request_body = AddItem,
    responses((status = 201, body = ImportReport), (status = 400, body = ErrorBody), (status = 503, body = ErrorBody))
)]
pub async fn add_movie(
    State(state): State<AppState>,
    Json(body): Json<AddItem>,
) -> Result<(StatusCode, Json<ImportReport>), ApiError> {
    let r = items::add(
        &state.db,
        &state.provider,
        "movie",
        &body.provider_id,
        body.library_id,
        body.monitored,
        body.quality_profile_id,
    )
    .await?;
    Ok((
        if r.created {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        },
        Json(r),
    ))
}

// ── API keys ───────────────────────────────────────────────────────────────

#[utoipa::path(get, path = "/api-keys", tag = "integration", responses((status = 200, body = Vec<ApiKey>)))]
pub async fn list_keys(State(state): State<AppState>) -> Result<Json<Vec<ApiKey>>, ApiError> {
    Ok(Json(apikeys::list(&state.db).await?))
}

#[derive(Deserialize, ToSchema)]
pub struct CreateKey {
    /// What it's for, e.g. "Sonarr".
    name: String,
}

/// Creates an API key. The key is in the response only; keep it in the tool.
#[utoipa::path(post, path = "/api-keys", tag = "integration", request_body = CreateKey, responses((status = 201, body = NewApiKey)))]
pub async fn create_key(
    State(state): State<AppState>,
    Json(body): Json<CreateKey>,
) -> Result<(StatusCode, Json<NewApiKey>), ApiError> {
    Ok((
        StatusCode::CREATED,
        Json(apikeys::create(&state.db, &body.name).await?),
    ))
}

#[utoipa::path(
    delete,
    path = "/api-keys/{id}",
    tag = "integration",
    params(("id" = i64, Path)),
    responses((status = 204), (status = 404, body = ErrorBody))
)]
pub async fn revoke_key(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    if apikeys::revoke(&state.db, id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}

// ── *arr compatibility ─────────────────────────────────────────────────────

/// Monitored series that want episodes, as a Sonarr "Custom List". Requires an API key.
#[utoipa::path(
    get,
    path = "/arr/sonarr-list",
    tag = "arr",
    responses((status = 200, body = Vec<SonarrListItem>), (status = 401, body = ErrorBody))
)]
pub async fn sonarr_list(
    _: ApiKeyAuth,
    State(state): State<AppState>,
) -> Result<Json<Vec<SonarrListItem>>, ApiError> {
    Ok(Json(arr::sonarr_list(&state.db).await?))
}

/// Wanted movies, readable as a Radarr "Radarr List" or "StevenLu Custom" list. Requires an API key.
#[utoipa::path(
    get,
    path = "/arr/radarr-list",
    tag = "arr",
    responses((status = 200, body = Vec<RadarrListItem>), (status = 401, body = ErrorBody))
)]
pub async fn radarr_list(
    _: ApiKeyAuth,
    State(state): State<AppState>,
) -> Result<Json<Vec<RadarrListItem>>, ApiError> {
    Ok(Json(arr::radarr_list(&state.db).await?))
}

#[derive(Serialize, ToSchema)]
pub struct WebhookReceived {
    /// Roots rescanned because the payload named paths in them.
    rescanning: Vec<i64>,
}

/// Receives Sonarr and Radarr Connect webhooks (import, upgrade, rename, delete)
/// and rescans the roots holding the paths they mention. Requires an API key.
#[utoipa::path(
    post,
    path = "/arr/webhook",
    tag = "arr",
    request_body(content = Object, description = "Any *arr webhook payload"),
    responses((status = 202, body = WebhookReceived), (status = 401, body = ErrorBody))
)]
pub async fn arr_webhook(
    _: ApiKeyAuth,
    State(state): State<AppState>,
    Json(payload): Json<serde_json::Value>,
) -> Result<(StatusCode, Json<WebhookReceived>), ApiError> {
    let paths = arr::paths_in(&payload);
    let roots = arr::roots_for(&state.db, &paths).await?;
    for root in &roots {
        state.scans.request(*root, Trigger::Watch).await?;
    }
    tracing::info!(event = ?payload.get("eventType"), ?roots, "*arr webhook");
    Ok((
        StatusCode::ACCEPTED,
        Json(WebhookReceived { rescanning: roots }),
    ))
}
