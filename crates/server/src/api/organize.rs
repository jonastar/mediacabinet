//! Writing to the library: imports, organising by template, moving series.

use std::path::PathBuf;

use axum::Json;
use axum::extract::{Path, State};
use cabinet_core::imports::{self, ImportListing, ImportsError};
use cabinet_core::matcher::{episodes, load_index};
use cabinet_core::organize::{
    self, FolderRename, ImportOptions, ImportResult, MovePlan, Naming, OrganizeApplied,
    OrganizeError, OrganizePreview, OrganizeScope, PlannedMove, TransferMode,
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

impl From<ImportsError> for ApiError {
    fn from(err: ImportsError) -> Self {
        match err {
            ImportsError::NotFound => ApiError::NotFound,
            ImportsError::Invalid(m) => ApiError::BadRequest(m),
            ImportsError::Db(e) => ApiError::Internal(e.to_string()),
        }
    }
}

fn prober() -> cabinet_core::scan::Prober {
    cabinet_core::scan::Prober {
        program: std::env::var("CABINET_FFPROBE")
            .unwrap_or_else(|_| "ffprobe".into())
            .into(),
        ..Default::default()
    }
}

/// Seconds a file must stay unchanged to count as finished downloading.
fn settle_seconds() -> i64 {
    std::env::var("CABINET_IMPORT_SETTLE_SECONDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(imports::SETTLE_SECONDS)
}

#[derive(Deserialize, ToSchema)]
pub struct ListImports {
    library_id: i64,
    /// Absolute path of a folder on the server, outside every library folder.
    path: String,
}

/// The videos in a folder that could be imported into a library, each with a
/// guess at which series episode or movie of the library it is, and whether it
/// looks finished downloading.
#[utoipa::path(
    post,
    path = "/imports/list",
    tag = "libraries",
    request_body = ListImports,
    responses((status = 200, body = ImportListing), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody))
)]
pub async fn list_imports(
    State(state): State<AppState>,
    Json(body): Json<ListImports>,
) -> Result<Json<ImportListing>, ApiError> {
    Ok(Json(
        imports::list(&state.db, body.library_id, &body.path, settle_seconds()).await?,
    ))
}

#[derive(Deserialize, ToSchema)]
pub struct ImportFile {
    /// Absolute path of the video on the server.
    path: String,
    /// The episodes of one series, or a movie.
    items: Vec<i64>,
    /// `copy`, `hardlink` or `move`.
    mode: TransferMode,
    /// `layout` (default) renames by the folder's layout; `keep` keeps the name.
    #[serde(default = "layout")]
    naming: Naming,
    /// Remove the items' existing files (an upgrade). Off by default.
    #[serde(default)]
    replace: bool,
    #[serde(default)]
    dry_run: bool,
}

fn layout() -> Naming {
    Naming::Layout
}

/// Imports one file by hand, as the import tool does: into the series' folder,
/// or the library's folder for new items, with its sidecars.
#[utoipa::path(
    post,
    path = "/imports/file",
    tag = "libraries",
    request_body = ImportFile,
    responses(
        (status = 200, body = ImportResult),
        (status = 400, body = ErrorBody),
        (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody)
    )
)]
pub async fn import_file(
    State(state): State<AppState>,
    Json(body): Json<ImportFile>,
) -> Result<Json<ImportResult>, ApiError> {
    let source = PathBuf::from(&body.path);
    let roots: Vec<String> = sqlx::query_scalar("SELECT path FROM library_root")
        .fetch_all(&state.db)
        .await?;
    if roots.iter().any(|r| source.starts_with(r)) {
        return Err(ApiError::BadRequest(
            "That file is already in a library folder. Use Organise to move it.".into(),
        ));
    }
    let result = organize::import_file(
        &state.db,
        &prober(),
        &source,
        &body.items,
        ImportOptions {
            mode: body.mode,
            naming: Some(body.naming),
            replace: body.replace,
            dry_run: body.dry_run,
        },
    )
    .await?;
    Ok(Json(result))
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
    /// `hardlink` (default), `copy` or `move`.
    #[serde(default)]
    mode: TransferMode,
    /// `layout` renames the file by the root's layout; `keep` keeps its name.
    /// Left out, the destination root's write policy decides (`managed` renames).
    naming: Option<Naming>,
    /// Remove the items' existing files (an upgrade). Off by default.
    #[serde(default)]
    replace: bool,
    /// Report where it would go without touching anything.
    #[serde(default)]
    dry_run: bool,
}

/// Hands a file over: places it by the naming asked for and the destination
/// root's layout, transfers it with its sidecars, and links it. Requires an API key.
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
    let result = organize::import_file(
        &state.db,
        &prober(),
        &PathBuf::from(&body.path),
        &items,
        ImportOptions {
            mode: body.mode,
            naming: body.naming,
            replace: body.replace,
            dry_run: body.dry_run,
        },
    )
    .await?;
    Ok(Json(result))
}

#[derive(Deserialize, ToSchema)]
pub struct DryRun {
    #[serde(default)]
    dry_run: bool,
}

/// Renames a series' or movie's files on writable roots to match the layout.
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
pub struct PreviewOrganize {
    /// Every writable folder of this library.
    library_id: Option<i64>,
    /// One writable folder.
    root_id: Option<i64>,
    /// One series or movie.
    item_id: Option<i64>,
}

/// What renaming files to their folder's layout would change, for a library, a
/// folder, or a series or movie. Changes nothing; apply what you want to keep
/// with `/organize/apply`.
#[utoipa::path(
    post,
    path = "/organize/preview",
    tag = "libraries",
    request_body = PreviewOrganize,
    responses((status = 200, body = OrganizePreview), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody))
)]
pub async fn preview_organize(
    State(state): State<AppState>,
    Json(body): Json<PreviewOrganize>,
) -> Result<Json<OrganizePreview>, ApiError> {
    let scope = match (body.library_id, body.root_id, body.item_id) {
        (Some(id), None, None) => OrganizeScope::Library(id),
        (None, Some(id), None) => OrganizeScope::Root(id),
        (None, None, Some(id)) => OrganizeScope::Item(id),
        _ => {
            return Err(ApiError::BadRequest(
                "Give one of library_id, root_id or item_id.".into(),
            ));
        }
    };
    Ok(Json(organize::preview_organize(&state.db, scope).await?))
}

#[derive(Deserialize, ToSchema)]
pub struct ApplyOrganize {
    /// Moves from a preview, as they were given.
    moves: Vec<PlannedMove>,
}

/// Applies moves from an Organise preview. Each is checked against its file as it
/// is now, so a stale preview fails per file instead of moving the wrong thing.
#[utoipa::path(
    post,
    path = "/organize/apply",
    tag = "libraries",
    request_body = ApplyOrganize,
    responses((status = 200, body = OrganizeApplied), (status = 400, body = ErrorBody))
)]
pub async fn apply_organize(
    State(state): State<AppState>,
    Json(body): Json<ApplyOrganize>,
) -> Result<Json<OrganizeApplied>, ApiError> {
    if body.moves.len() > 500 {
        return Err(ApiError::BadRequest(
            "Apply at most 500 moves at a time.".into(),
        ));
    }
    Ok(Json(
        organize::apply_organize(&state.db, &body.moves).await?,
    ))
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
    /// Ignored: read-only roots are never renamed. Kept so older clients still parse.
    #[serde(default)]
    #[allow(dead_code)]
    force: bool,
}

/// Renames a series' folder on a root so the provider IDs in its name, such as
/// `[tmdb-1399]`, are the series' own. Refused on read-only roots.
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
        organize::fix_folder_ids(&state.db, id, body.root_id).await?,
    ))
}
