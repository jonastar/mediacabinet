//! The activity log, filtered for whoever is looking.

use axum::Json;
use axum::extract::{Query, State};
use cabinet_core::activity::{self, ActivityError, ActivityFilter, ActivityPage};
use serde::Serialize;
use utoipa::ToSchema;

use super::ApiError;
use super::auth::{Actor, Viewer};
use super::error::ErrorBody;
use crate::AppState;

impl From<ActivityError> for ApiError {
    fn from(err: ActivityError) -> Self {
        match err {
            ActivityError::Invalid(m) => ApiError::BadRequest(m),
            ActivityError::Db(e) => ApiError::Internal(e.to_string()),
        }
    }
}

/// Records a configuration change for admins: what changed and which fields,
/// never their values (keys and secrets stay out of the log).
pub async fn config_changed(
    state: &AppState,
    actor: Option<i64>,
    target: &str,
    action: &str,
    id: i64,
    name: Option<&str>,
    fields: &[&str],
) {
    let entry = activity::config_changed(
        cabinet_core::catalog::now(),
        actor,
        target,
        action,
        id,
        name,
        fields,
    );
    activity::record_or_warn(&state.db, entry).await;
}

/// What happened, newest first: only the entries the caller may see. Admins see
/// everything; others see what's for everyone and what's about themselves.
#[utoipa::path(
    get,
    path = "/activity",
    tag = "activity",
    params(ActivityFilter),
    responses((status = 200, body = ActivityPage), (status = 400, body = ErrorBody))
)]
pub async fn list(
    State(state): State<AppState>,
    Actor(actor): Actor,
    Query(filter): Query<ActivityFilter>,
) -> Result<Json<ActivityPage>, ApiError> {
    let viewer = activity::Viewer {
        user: actor.user,
        admin: actor.admin,
    };
    Ok(Json(activity::list(&state.db, viewer, &filter).await?))
}

#[derive(Serialize, ToSchema)]
pub struct Cleared {
    removed: u64,
}

/// Clears what you watched from the activity log. Resume points and played
/// marks stay.
#[utoipa::path(
    delete,
    path = "/activity/mine/watch",
    tag = "activity",
    responses((status = 200, body = Cleared))
)]
pub async fn clear_my_watch(
    State(state): State<AppState>,
    Viewer(user): Viewer,
) -> Result<Json<Cleared>, ApiError> {
    Ok(Json(Cleared {
        removed: activity::clear_watch(&state.db, user).await?,
    }))
}
