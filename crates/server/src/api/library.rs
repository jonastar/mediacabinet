//! Libraries, their roots, layout templates, scans and metadata refreshes.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use cabinet_core::catalog::folders::{self, FolderPage};
use cabinet_core::layout::{LibraryKind, Proposal, Template, TemplatePiece};
use cabinet_core::library::{
    self, FolderListing, Library, NewRoot, PlacementExample, Root, RootChanges,
};
use cabinet_core::metadata::refresh::RefreshProgress;
use cabinet_core::scan::{self, StoredProbe};
use cabinet_core::tasks::{Kind, Scope, Trigger};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use super::ApiError;
use super::activity::config_changed;
use super::auth::{ActingUser, Viewer};
use super::error::ErrorBody;
use crate::AppState;

fn with_progress(state: &AppState, mut libraries: Vec<Library>) -> Vec<Library> {
    let running = state.scans.all_progress();
    for library in &mut libraries {
        library.refreshing = refreshing(state, library.id);
        for root in &mut library.roots {
            root.scanning = running.get(&root.id).cloned();
        }
    }
    libraries
}

fn refreshing(state: &AppState, library: i64) -> Option<RefreshProgress> {
    state
        .tasks
        .running(Kind::MetadataRefresh, Scope::Library(library))
        .map(RefreshProgress::from)
}

fn kind(kind: &str) -> Result<LibraryKind, ApiError> {
    LibraryKind::parse(kind)
        .ok_or_else(|| ApiError::BadRequest("kind is tv, movies or folders".into()))
}

/// Every library with its roots, file counts and scan state.
#[utoipa::path(get, path = "/libraries", tag = "libraries", responses((status = 200, body = Vec<Library>)))]
pub async fn list_libraries(State(state): State<AppState>) -> Result<Json<Vec<Library>>, ApiError> {
    let libraries = library::list(&state.db).await?;
    Ok(Json(with_progress(&state, libraries)))
}

#[derive(Deserialize, IntoParams)]
pub struct BrowseQuery {
    /// A folder relative to the library's roots; the top when left out.
    #[serde(default)]
    path: String,
}

/// One folder of a folder library: its subfolders and videos, with the viewer's
/// watch state. Folders with the same path on different roots show as one.
#[utoipa::path(
    get,
    path = "/libraries/{id}/browse",
    tag = "libraries",
    params(("id" = i64, Path), BrowseQuery),
    responses((status = 200, body = FolderPage), (status = 404, body = ErrorBody))
)]
pub async fn browse_library(
    State(state): State<AppState>,
    viewer: Viewer,
    Path(id): Path<i64>,
    Query(query): Query<BrowseQuery>,
) -> Result<Json<FolderPage>, ApiError> {
    Ok(Json(
        folders::browse(&state.db, id, &query.path, Some(viewer.0)).await?,
    ))
}

#[derive(Deserialize, ToSchema)]
pub struct CreateLibrary {
    name: String,
    /// `tv`, `movies` or `folders`.
    kind: String,
}

#[utoipa::path(
    post,
    path = "/libraries",
    tag = "libraries",
    request_body = CreateLibrary,
    responses((status = 201, body = Library), (status = 400, body = ErrorBody), (status = 409, body = ErrorBody))
)]
pub async fn create_library(
    State(state): State<AppState>,
    ActingUser(actor): ActingUser,
    Json(body): Json<CreateLibrary>,
) -> Result<(StatusCode, Json<Library>), ApiError> {
    let created = library::create(&state.db, &body.name, &body.kind).await?;
    let name = Some(created.name.as_str());
    config_changed(&state, actor, "library", "created", created.id, name, &[]).await;
    Ok((StatusCode::CREATED, Json(created)))
}

#[derive(Deserialize, ToSchema)]
pub struct RenameLibrary {
    name: String,
}

#[utoipa::path(
    patch,
    path = "/libraries/{id}",
    tag = "libraries",
    params(("id" = i64, Path)),
    request_body = RenameLibrary,
    responses((status = 200, body = Library), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody))
)]
pub async fn rename_library(
    State(state): State<AppState>,
    ActingUser(actor): ActingUser,
    Path(id): Path<i64>,
    Json(body): Json<RenameLibrary>,
) -> Result<Json<Library>, ApiError> {
    let renamed = library::rename(&state.db, id, &body.name).await?;
    let name = Some(renamed.name.as_str());
    config_changed(&state, actor, "library", "changed", id, name, &["name"]).await;
    Ok(Json(renamed))
}

/// Deletes an empty library. Its folders must be removed first.
#[utoipa::path(
    delete,
    path = "/libraries/{id}",
    tag = "libraries",
    params(("id" = i64, Path)),
    responses((status = 204), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody))
)]
pub async fn delete_library(
    State(state): State<AppState>,
    ActingUser(actor): ActingUser,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    let name = library::get(&state.db, id).await?.name;
    library::delete(&state.db, id).await?;
    config_changed(&state, actor, "library", "removed", id, Some(&name), &[]).await;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, ToSchema)]
pub struct AddRoot {
    /// Absolute path on the server.
    path: String,
    /// `managed`, `adaptive` (default) or `read_only`.
    #[serde(default = "adaptive")]
    write_policy: String,
    layout_template: String,
    #[serde(default)]
    is_import_target: bool,
}

fn adaptive() -> String {
    "adaptive".into()
}

/// Adds a folder to a library and starts its first scan.
#[utoipa::path(
    post,
    path = "/libraries/{id}/roots",
    tag = "libraries",
    params(("id" = i64, Path)),
    request_body = AddRoot,
    responses(
        (status = 201, body = Root),
        (status = 400, body = ErrorBody),
        (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody)
    )
)]
pub async fn add_root(
    State(state): State<AppState>,
    ActingUser(actor): ActingUser,
    Path(id): Path<i64>,
    Json(body): Json<AddRoot>,
) -> Result<(StatusCode, Json<Root>), ApiError> {
    let mut root = library::add_root(
        &state.db,
        id,
        NewRoot {
            path: &body.path,
            write_policy: &body.write_policy,
            layout_template: &body.layout_template,
            is_import_target: body.is_import_target,
        },
    )
    .await?;
    config_changed(
        &state,
        actor,
        "root",
        "created",
        root.id,
        Some(&root.path),
        &[],
    )
    .await;
    state.scans.request(root.id, Trigger::Added).await?;
    root.scanning = state.scans.progress(root.id);
    Ok((StatusCode::CREATED, Json(root)))
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateRoot {
    write_policy: Option<String>,
    layout_template: Option<String>,
    is_import_target: Option<bool>,
}

/// Changes a root's write policy, layout template or import target flag.
#[utoipa::path(
    patch,
    path = "/roots/{id}",
    tag = "libraries",
    params(("id" = i64, Path)),
    request_body = UpdateRoot,
    responses((status = 200, body = Root), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody))
)]
pub async fn update_root(
    State(state): State<AppState>,
    ActingUser(actor): ActingUser,
    Path(id): Path<i64>,
    Json(body): Json<UpdateRoot>,
) -> Result<Json<Root>, ApiError> {
    let given = [
        ("write_policy", body.write_policy.is_some()),
        ("layout_template", body.layout_template.is_some()),
        ("is_import_target", body.is_import_target.is_some()),
    ];
    let fields: Vec<&str> = given.iter().filter(|f| f.1).map(|f| f.0).collect();
    let mut root = library::update_root(
        &state.db,
        id,
        RootChanges {
            write_policy: body.write_policy.as_deref(),
            layout_template: body.layout_template.as_deref(),
            is_import_target: body.is_import_target,
        },
    )
    .await?;
    config_changed(
        &state,
        actor,
        "root",
        "changed",
        id,
        Some(&root.path),
        &fields,
    )
    .await;
    root.scanning = state.scans.progress(root.id);
    Ok(Json(root))
}

/// Removes a folder from its library. Refused while its files are linked to items.
#[utoipa::path(
    delete,
    path = "/roots/{id}",
    tag = "libraries",
    params(("id" = i64, Path)),
    responses((status = 204), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody))
)]
pub async fn remove_root(
    State(state): State<AppState>,
    ActingUser(actor): ActingUser,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    if state.scans.progress(id).is_some() {
        return Err(ApiError::Conflict("Wait for the scan to finish.".into()));
    }
    let path = library::root(&state.db, id).await?.path;
    library::remove_root(&state.db, id).await?;
    config_changed(&state, actor, "root", "removed", id, Some(&path), &[]).await;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize, ToSchema)]
pub struct ScanStarted {
    /// False when a scan was already running; one more runs after it.
    started: bool,
}

/// Scans a root now.
#[utoipa::path(
    post,
    path = "/roots/{id}/scan",
    tag = "libraries",
    params(("id" = i64, Path)),
    responses((status = 202, body = ScanStarted), (status = 404, body = ErrorBody))
)]
pub async fn scan_root(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<(StatusCode, Json<ScanStarted>), ApiError> {
    library::root(&state.db, id).await?;
    let started = state.scans.request(id, Trigger::Manual).await?;
    Ok((StatusCode::ACCEPTED, Json(ScanStarted { started })))
}

#[derive(Serialize, ToSchema)]
pub struct RefreshStarted {
    /// False when a refresh was already running; one more runs after it.
    started: bool,
}

/// Fetches the metadata of every series and movie in a library from its provider
/// again, with ratings when OMDb is set up. Runs as a task; progress shows on the
/// library as `refreshing`.
#[utoipa::path(
    post,
    path = "/libraries/{id}/refresh",
    tag = "libraries",
    params(("id" = i64, Path)),
    responses(
        (status = 202, body = RefreshStarted),
        (status = 400, body = ErrorBody),
        (status = 404, body = ErrorBody),
        (status = 503, body = ErrorBody)
    )
)]
pub async fn refresh_library(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<(StatusCode, Json<RefreshStarted>), ApiError> {
    let library = library::get(&state.db, id).await?;
    if library.kind == "folders" {
        return Err(ApiError::BadRequest(
            "A folder library has no metadata to refresh.".into(),
        ));
    }
    if !state.provider.is_configured() {
        return Err(ApiError::Unavailable(
            cabinet_core::metadata::MetadataError::NotConfigured.to_string(),
        ));
    }
    let started = state
        .tasks
        .request(Kind::MetadataRefresh, Scope::Library(id), Trigger::Manual)
        .await?;
    Ok((StatusCode::ACCEPTED, Json(RefreshStarted { started })))
}

#[derive(Deserialize, ToSchema)]
pub struct ProposeTemplate {
    path: String,
    /// `tv`, `movies` or `folders`.
    kind: String,
}

#[derive(Serialize, ToSchema)]
pub struct FolderProposal {
    /// The folder, resolved.
    path: String,
    #[serde(flatten)]
    proposal: Proposal,
    pieces: Vec<TemplatePiece>,
}

/// Checks a folder can be added and proposes a layout template from what's in it.
#[utoipa::path(
    post,
    path = "/folders/propose",
    tag = "libraries",
    request_body = ProposeTemplate,
    responses((status = 200, body = FolderProposal), (status = 400, body = ErrorBody), (status = 409, body = ErrorBody))
)]
pub async fn propose_template(
    State(state): State<AppState>,
    Json(body): Json<ProposeTemplate>,
) -> Result<Json<FolderProposal>, ApiError> {
    let kind = kind(&body.kind)?;
    let path = library::check_folder(&state.db, &body.path, None).await?;
    let walk_path = path.clone();
    let walk = tokio::task::spawn_blocking(move || cabinet_core::scan::walk(&walk_path))
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .map_err(|e| ApiError::BadRequest(format!("Can't read that folder: {e}")))?;
    let paths: Vec<String> = walk.files.into_iter().map(|f| f.relative_path).collect();
    let proposal = cabinet_core::layout::propose(&paths, kind);
    let pieces = Template::parse(&proposal.template, kind)
        .map(|t| t.pieces())
        .unwrap_or_default();
    Ok(Json(FolderProposal {
        path: path.to_string_lossy().into_owned(),
        proposal,
        pieces,
    }))
}

#[derive(Deserialize, ToSchema)]
pub struct CheckTemplate {
    template: String,
    /// `tv`, `movies` or `folders`.
    kind: String,
}

#[derive(Serialize, ToSchema)]
pub struct TemplateCheck {
    valid: bool,
    /// What's wrong, when it isn't valid.
    error: Option<String>,
    /// The normalised template, when valid.
    template: Option<String>,
    pieces: Vec<TemplatePiece>,
}

/// Validates a layout template and splits it for display, for live editing.
#[utoipa::path(
    post,
    path = "/templates/check",
    tag = "libraries",
    request_body = CheckTemplate,
    responses((status = 200, body = TemplateCheck), (status = 400, body = ErrorBody))
)]
pub async fn check_template(
    Json(body): Json<CheckTemplate>,
) -> Result<Json<TemplateCheck>, ApiError> {
    let kind = kind(&body.kind)?;
    Ok(Json(match Template::parse(&body.template, kind) {
        Ok(t) => TemplateCheck {
            valid: true,
            error: None,
            template: Some(t.source()),
            pieces: t.pieces(),
        },
        Err(e) => TemplateCheck {
            valid: false,
            error: Some(e.0),
            template: None,
            pieces: Vec::new(),
        },
    }))
}

#[derive(Deserialize, IntoParams)]
pub struct FoldersQuery {
    /// Absolute path; defaults to `/`.
    #[serde(default)]
    path: String,
}

/// Lists the subfolders of a server folder, for picking a root.
#[utoipa::path(
    get,
    path = "/folders",
    tag = "libraries",
    params(FoldersQuery),
    responses((status = 200, body = FolderListing), (status = 400, body = ErrorBody))
)]
pub async fn list_folders(
    Query(query): Query<FoldersQuery>,
) -> Result<Json<FolderListing>, ApiError> {
    Ok(Json(library::list_folders(&query.path).await?))
}

/// Where the next file for a series in this library would go, under each write policy.
#[utoipa::path(
    get,
    path = "/libraries/{id}/placement-example",
    tag = "libraries",
    params(("id" = i64, Path)),
    responses((status = 200, body = Option<PlacementExample>), (status = 404, body = ErrorBody))
)]
pub async fn placement_example(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Option<PlacementExample>>, ApiError> {
    Ok(Json(library::placement_example(&state.db, id).await?))
}

/// What the last probe found in a file: its streams in order, the container and
/// duration, any probe error, and the sidecars next to it.
#[utoipa::path(
    get,
    path = "/files/{id}/probe",
    tag = "libraries",
    params(("id" = i64, Path)),
    responses((status = 200, body = StoredProbe), (status = 404, body = ErrorBody))
)]
pub async fn file_probe(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<StoredProbe>, ApiError> {
    scan::stored_probe(&state.db, id)
        .await?
        .map(Json)
        .ok_or(ApiError::NotFound)
}
