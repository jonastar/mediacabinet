//! Writing to the library: imports, organising by template, moving series.

use std::path::PathBuf;

use axum::Json;
use axum::extract::{Path, State};
use cabinet_core::matcher::{episodes, load_index};
use cabinet_core::organize::{
    self, FolderRename, ImportResult, MovePlan, OrganizeError, TransferMode,
};
use cabinet_core::parse::parse;
use cabinet_core::refs::{self, ItemRef, Position};
use serde::Deserialize;
use utoipa::ToSchema;

use super::ApiError;
use super::auth::ApiKeyAuth;
use super::error::ErrorBody;
use crate::AppState;

impl From<OrganizeError> for ApiError {
    fn from(err: OrganizeError) -> Self {
        match err {
            OrganizeError::NotFound => ApiError::NotFound,
            OrganizeError::Invalid(m) => ApiError::BadRequest(m),
            OrganizeError::Conflict(m) => ApiError::Conflict(m),
            OrganizeError::Io(e) => ApiError::Internal(e.to_string()),
            OrganizeError::Db(e) => ApiError::Internal(e.to_string()),
        }
    }
}

#[derive(Deserialize, ToSchema)]
pub struct ImportRequest {
    /// Absolute path of the file on the server.
    path: String,
    /// The episode, movie or series: `item:<id>` or `<provider>:<id>`.
    item: String,
    /// Episodes of that series, e.g. `["S01E02", "S01E03"]` for a double episode.
    /// Without them, a series' episodes are read from the file name.
    #[serde(default)]
    episodes: Vec<String>,
    /// Ordering the episode numbers are in; default the series' default.
    ordering: Option<String>,
    #[serde(default)]
    mode: TransferMode,
    /// Remove the items' existing files (an upgrade). Off by default.
    #[serde(default)]
    replace: bool,
    /// Report where it would go without touching anything.
    #[serde(default)]
    dry_run: bool,
}

/// Hands a file over: places it by the destination root's write policy and
/// template, transfers it with its sidecars, and links it. Requires an API key.
#[utoipa::path(
    post,
    path = "/imports",
    tag = "integration",
    request_body = ImportRequest,
    responses(
        (status = 200, body = ImportResult),
        (status = 400, body = ErrorBody),
        (status = 401, body = ErrorBody),
        (status = 409, body = ErrorBody)
    )
)]
pub async fn import(
    _: ApiKeyAuth,
    State(state): State<AppState>,
    Json(body): Json<ImportRequest>,
) -> Result<Json<ImportResult>, ApiError> {
    let reference = ItemRef::parse(&body.item)?;
    let ordering = body.ordering.as_deref().unwrap_or("default");
    let top = refs::resolve(&state.db, &reference, None).await?;
    let items = if !body.episodes.is_empty() {
        let mut items = Vec::new();
        for e in &body.episodes {
            items.push(
                refs::resolve(&state.db, &reference, Some(&Position::parse(ordering, e)?)).await?,
            );
        }
        items
    } else {
        let kind: String = sqlx::query_scalar("SELECT kind FROM item WHERE id = ?")
            .bind(top)
            .fetch_one(&state.db)
            .await?;
        if kind == "series" {
            let name = std::path::Path::new(&body.path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            let index = load_index(&state.db, top).await?;
            episodes::resolve(&index, &parse(name), None, None)
                .map(|r| r.items)
                .ok_or_else(|| {
                    ApiError::BadRequest(format!(
                        "{name} doesn't name an episode of this series; pass episodes."
                    ))
                })?
        } else {
            vec![top]
        }
    };
    let prober = cabinet_core::scan::Prober {
        program: std::env::var("CABINET_FFPROBE")
            .unwrap_or_else(|_| "ffprobe".into())
            .into(),
        ..Default::default()
    };
    let result = organize::import_file(
        &state.db,
        &prober,
        &PathBuf::from(&body.path),
        &items,
        body.mode,
        body.replace,
        body.dry_run,
    )
    .await?;
    Ok(Json(result))
}

#[derive(Deserialize, ToSchema)]
pub struct DryRun {
    #[serde(default)]
    dry_run: bool,
}

/// Renames a series' or movie's files on managed roots to match the template.
#[utoipa::path(
    post,
    path = "/items/{id}/organize",
    tag = "items",
    params(("id" = i64, Path)),
    request_body = DryRun,
    responses((status = 200, body = MovePlan), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody))
)]
pub async fn organize_item(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<DryRun>,
) -> Result<Json<MovePlan>, ApiError> {
    Ok(Json(organize::organize(&state.db, id, body.dry_run).await?))
}

#[derive(Deserialize, ToSchema)]
pub struct MoveSeries {
    root_id: i64,
    #[serde(default)]
    dry_run: bool,
}

/// Moves a series' files to another root; the new folder becomes primary.
#[utoipa::path(
    post,
    path = "/series/{id}/move",
    tag = "items",
    params(("id" = i64, Path)),
    request_body = MoveSeries,
    responses((status = 200, body = MovePlan), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody))
)]
pub async fn move_series(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<MoveSeries>,
) -> Result<Json<MovePlan>, ApiError> {
    Ok(Json(
        organize::move_series(&state.db, id, body.root_id, body.dry_run).await?,
    ))
}

#[derive(Deserialize, ToSchema)]
pub struct FixFolderIds {
    root_id: i64,
    /// Rename it even on a read-only root.
    #[serde(default)]
    force: bool,
}

/// Renames a series' folder on a root so the provider IDs in its name, such as
/// `[tmdb-1399]`, are the series' own. Read-only roots need `force`.
#[utoipa::path(
    post,
    path = "/series/{id}/fix-folder-ids",
    tag = "items",
    params(("id" = i64, Path)),
    request_body = FixFolderIds,
    responses(
        (status = 200, body = FolderRename),
        (status = 400, body = ErrorBody),
        (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody)
    )
)]
pub async fn fix_folder_ids(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<FixFolderIds>,
) -> Result<Json<FolderRename>, ApiError> {
    Ok(Json(
        organize::fix_folder_ids(&state.db, id, body.root_id, body.force).await?,
    ))
}
