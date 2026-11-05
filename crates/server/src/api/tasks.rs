//! Background tasks, for the Tasks tab: what's running, what happened, and why.
//! Admins only. Requests live with what they act on, such as
//! `POST /libraries/{id}/thumbnails`.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use cabinet_core::tasks::Cancel;
use cabinet_core::tasks::history::{TaskDetail, TaskFilter, TaskPage};
use serde::Deserialize;
use utoipa::IntoParams;

use super::ApiError;
use super::error::ErrorBody;
use crate::AppState;

const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 1000;

#[derive(Deserialize, IntoParams)]
pub struct TasksQuery {
    /// `scan`, `metadata_refresh` or `thumbnails`.
    kind: Option<String>,
    /// For finished tasks: `ok`, `failed` or `cancelled`.
    state: Option<String>,
    /// Finished tasks to return: 50 unless asked, 1000 at most.
    limit: Option<i64>,
}

/// What's queued and running, and the latest finished tasks.
#[utoipa::path(get, path = "/tasks", tag = "tasks", params(TasksQuery), responses((status = 200, body = TaskPage)))]
pub async fn list(
    State(state): State<AppState>,
    Query(query): Query<TasksQuery>,
) -> Result<Json<TaskPage>, ApiError> {
    let filter = TaskFilter {
        kind: query.kind,
        state: query.state,
        limit: query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT),
    };
    Ok(Json(state.tasks.page(&filter).await?))
}

/// One task, with its outcome and log.
#[utoipa::path(
    get,
    path = "/tasks/{id}",
    tag = "tasks",
    params(("id" = i64, Path)),
    responses((status = 200, body = TaskDetail), (status = 404, body = ErrorBody))
)]
pub async fn detail(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<TaskDetail>, ApiError> {
    let task = state.tasks.detail(id).await?.ok_or(ApiError::NotFound)?;
    Ok(Json(task))
}

/// Stops a task: one that hasn't started won't, and one running stops at its next
/// item. A running scan can't be stopped.
#[utoipa::path(
    post,
    path = "/tasks/{id}/cancel",
    tag = "tasks",
    params(("id" = i64, Path)),
    responses(
        (status = 204),
        (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody)
    )
)]
pub async fn cancel(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    match state.tasks.cancel(id).await? {
        Cancel::Cancelled | Cancel::Stopping => Ok(StatusCode::NO_CONTENT),
        Cancel::CantStop => Err(ApiError::Conflict(
            "A running scan can't be stopped.".into(),
        )),
        Cancel::NotOpen => {
            state.tasks.detail(id).await?.ok_or(ApiError::NotFound)?;
            Err(ApiError::Conflict("This task has already finished.".into()))
        }
    }
}
