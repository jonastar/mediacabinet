//! Libraries and their roots: what the Libraries & roots screen shows and changes.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::json;
use sqlx::SqlitePool;
use utoipa::ToSchema;

use crate::events;
use crate::layout::{self, Incoming, LibraryKind, Placement, Template, TemplatePiece, WritePolicy};

#[derive(Debug, thiserror::Error)]
pub enum LibraryError {
    #[error("not found")]
    NotFound,
    /// The request can't be applied as given; the message says why.
    #[error("{0}")]
    Invalid(String),
    /// It conflicts with existing data, e.g. removing a root that has linked files.
    #[error("{0}")]
    Conflict(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

fn invalid<T>(message: impl Into<String>) -> Result<T, LibraryError> {
    Err(LibraryError::Invalid(message.into()))
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Library {
    pub id: i64,
    pub name: String,
    /// `tv` or `movies`.
    pub kind: String,
    pub roots: Vec<Root>,
}

#[derive(Debug, Clone, Default, Serialize, ToSchema)]
pub struct FileCounts {
    pub present: i64,
    pub missing: i64,
    pub ignored: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ScanRun {
    pub id: i64,
    /// `added`, `manual`, `scheduled` or `watch`.
    pub trigger: String,
    /// `running`, `ok` or `failed`.
    pub state: String,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    #[schema(value_type = Option<Object>)]
    pub report: Option<serde_json::Value>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Root {
    pub id: i64,
    pub library_id: i64,
    pub path: String,
    /// `managed`, `adaptive` or `read_only`.
    pub write_policy: String,
    pub layout_template: String,
    /// The template split for display.
    pub template_pieces: Vec<TemplatePiece>,
    pub is_import_target: bool,
    /// Whether the folder is reachable right now.
    pub online: bool,
    /// When a scan last found it unreachable; cleared by a successful scan.
    pub offline_since: Option<i64>,
    pub files: FileCounts,
    pub last_scan: Option<ScanRun>,
    /// Set while a scan runs; filled in by whoever runs scans.
    pub scanning: Option<crate::scan::ScanProgress>,
}

#[derive(sqlx::FromRow)]
struct RootRow {
    id: i64,
    library_id: i64,
    path: String,
    write_policy: String,
    layout_template: String,
    is_import_target: bool,
    offline_since: Option<i64>,
}

#[derive(sqlx::FromRow)]
struct ScanRunRow {
    id: i64,
    root_id: i64,
    trigger: String,
    state: String,
    started_at: i64,
    finished_at: Option<i64>,
    report: Option<String>,
    error: Option<String>,
}

impl From<ScanRunRow> for ScanRun {
    fn from(r: ScanRunRow) -> Self {
        ScanRun {
            id: r.id,
            trigger: r.trigger,
            state: r.state,
            started_at: r.started_at,
            finished_at: r.finished_at,
            report: r.report.and_then(|s| serde_json::from_str(&s).ok()),
            error: r.error,
        }
    }
}

async fn is_dir(path: &str) -> bool {
    tokio::fs::metadata(path).await.is_ok_and(|m| m.is_dir())
}

fn kind_of(kind: &str) -> LibraryKind {
    LibraryKind::parse(kind).unwrap_or(LibraryKind::Tv)
}

pub async fn list(pool: &SqlitePool) -> Result<Vec<Library>, LibraryError> {
    let libraries: Vec<(i64, String, String)> =
        sqlx::query_as("SELECT id, name, kind FROM library ORDER BY name COLLATE NOCASE, id")
            .fetch_all(pool)
            .await?;
    let roots: Vec<RootRow> = sqlx::query_as(
        "SELECT id, library_id, path, write_policy, layout_template, is_import_target, offline_since
         FROM library_root ORDER BY is_import_target DESC, path",
    )
    .fetch_all(pool)
    .await?;
    let counts: Vec<(i64, String, i64)> = sqlx::query_as(
        "SELECT root_id, status, count(*) FROM media_file
         WHERE status <> 'removed' GROUP BY root_id, status",
    )
    .fetch_all(pool)
    .await?;
    let mut files: HashMap<i64, FileCounts> = HashMap::new();
    for (root, status, n) in counts {
        let c = files.entry(root).or_default();
        match status.as_str() {
            "present" => c.present = n,
            "missing" => c.missing = n,
            _ => c.ignored = n,
        }
    }
    let runs: Vec<ScanRunRow> = sqlx::query_as(
        "SELECT r.id, r.root_id, r.trigger, r.state, r.started_at, r.finished_at, r.report, r.error
         FROM scan_run r
         WHERE r.id = (SELECT max(id) FROM scan_run WHERE root_id = r.root_id)",
    )
    .fetch_all(pool)
    .await?;
    let mut last_scan: HashMap<i64, ScanRun> =
        runs.into_iter().map(|r| (r.root_id, r.into())).collect();

    let kinds: HashMap<i64, LibraryKind> = libraries
        .iter()
        .map(|(id, _, k)| (*id, kind_of(k)))
        .collect();
    let mut by_library: HashMap<i64, Vec<Root>> = HashMap::new();
    for r in roots {
        let pieces = Template::parse(&r.layout_template, kinds[&r.library_id])
            .map(|t| t.pieces())
            .unwrap_or_default();
        let online = is_dir(&r.path).await;
        by_library.entry(r.library_id).or_default().push(Root {
            id: r.id,
            library_id: r.library_id,
            online,
            template_pieces: pieces,
            files: files.remove(&r.id).unwrap_or_default(),
            last_scan: last_scan.remove(&r.id),
            scanning: None,
            path: r.path,
            write_policy: r.write_policy,
            layout_template: r.layout_template,
            is_import_target: r.is_import_target,
            offline_since: r.offline_since,
        });
    }
    Ok(libraries
        .into_iter()
        .map(|(id, name, kind)| Library {
            id,
            name,
            kind,
            roots: by_library.remove(&id).unwrap_or_default(),
        })
        .collect())
}

pub async fn get(pool: &SqlitePool, id: i64) -> Result<Library, LibraryError> {
    list(pool)
        .await?
        .into_iter()
        .find(|l| l.id == id)
        .ok_or(LibraryError::NotFound)
}

pub async fn create(pool: &SqlitePool, name: &str, kind: &str) -> Result<Library, LibraryError> {
    let name = name.trim();
    if name.is_empty() {
        return invalid("Give the library a name.");
    }
    if LibraryKind::parse(kind).is_none() {
        return invalid("A library holds either tv or movies.");
    }
    let mut tx = crate::db::begin_write(pool).await?;
    let id: i64 = sqlx::query_scalar("INSERT INTO library (name, kind) VALUES (?, ?) RETURNING id")
        .bind(name)
        .bind(kind)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| unique_violation(e, "A library with that name already exists."))?;
    events::emit(&mut tx, events::LIBRARY_ADDED, json!({ "library_id": id })).await?;
    tx.commit().await?;
    get(pool, id).await
}

pub async fn rename(pool: &SqlitePool, id: i64, name: &str) -> Result<Library, LibraryError> {
    let name = name.trim();
    if name.is_empty() {
        return invalid("Give the library a name.");
    }
    let done = sqlx::query("UPDATE library SET name = ? WHERE id = ?")
        .bind(name)
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| unique_violation(e, "A library with that name already exists."))?;
    if done.rows_affected() == 0 {
        return Err(LibraryError::NotFound);
    }
    get(pool, id).await
}

pub async fn delete(pool: &SqlitePool, id: i64) -> Result<(), LibraryError> {
    let roots: i64 = sqlx::query_scalar("SELECT count(*) FROM library_root WHERE library_id = ?")
        .bind(id)
        .fetch_one(pool)
        .await?;
    if roots > 0 {
        return Err(LibraryError::Conflict(
            "Remove the library's folders first.".into(),
        ));
    }
    let items: i64 = sqlx::query_scalar("SELECT count(*) FROM item WHERE library_id = ?")
        .bind(id)
        .fetch_one(pool)
        .await?;
    if items > 0 {
        return Err(LibraryError::Conflict(format!(
            "It still holds {items} series or movies, with their watch history."
        )));
    }
    let mut tx = crate::db::begin_write(pool).await?;
    let done = sqlx::query("DELETE FROM library WHERE id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    if done.rows_affected() == 0 {
        return Err(LibraryError::NotFound);
    }
    events::emit(
        &mut tx,
        events::LIBRARY_REMOVED,
        json!({ "library_id": id }),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

fn unique_violation(err: sqlx::Error, message: &str) -> LibraryError {
    match &err {
        sqlx::Error::Database(db) if db.is_unique_violation() => {
            LibraryError::Conflict(message.into())
        }
        _ => err.into(),
    }
}

/// Checks a folder can become a root: absolute, reachable, and not inside or
/// around another root (a file must belong to exactly one root). Returns the
/// canonical path.
pub async fn check_folder(
    pool: &SqlitePool,
    path: &str,
    except_root: Option<i64>,
) -> Result<PathBuf, LibraryError> {
    let raw = Path::new(path.trim());
    if !raw.is_absolute() {
        return invalid("Use a full path, starting with /.");
    }
    let canonical = match tokio::fs::canonicalize(raw).await {
        Ok(p) if is_dir(&p.to_string_lossy()).await => p,
        _ => return invalid("That folder doesn't exist or can't be reached from the server."),
    };
    let roots: Vec<(i64, String)> = sqlx::query_as("SELECT id, path FROM library_root")
        .fetch_all(pool)
        .await?;
    for (id, other) in roots {
        if Some(id) == except_root {
            continue;
        }
        let other = Path::new(&other);
        if canonical == other {
            return Err(LibraryError::Conflict(
                "That folder is already in a library.".into(),
            ));
        }
        if canonical.starts_with(other) || other.starts_with(&canonical) {
            return Err(LibraryError::Conflict(format!(
                "That folder overlaps {}, which is already in a library. Each file can belong to one folder only.",
                other.display()
            )));
        }
    }
    Ok(canonical)
}

pub struct NewRoot<'a> {
    pub path: &'a str,
    pub write_policy: &'a str,
    pub layout_template: &'a str,
    pub is_import_target: bool,
}

pub async fn add_root(
    pool: &SqlitePool,
    library_id: i64,
    new: NewRoot<'_>,
) -> Result<Root, LibraryError> {
    let kind: String = sqlx::query_scalar("SELECT kind FROM library WHERE id = ?")
        .bind(library_id)
        .fetch_optional(pool)
        .await?
        .ok_or(LibraryError::NotFound)?;
    let template = Template::parse(new.layout_template, kind_of(&kind))
        .map_err(|e| LibraryError::Invalid(e.0))?;
    let policy = WritePolicy::parse(new.write_policy).ok_or_else(|| {
        LibraryError::Invalid("Write policy is managed, adaptive or read_only.".into())
    })?;
    if new.is_import_target && policy == WritePolicy::ReadOnly {
        return invalid("A read-only folder can't be the import target.");
    }
    let path = check_folder(pool, new.path, None).await?;

    let mut tx = crate::db::begin_write(pool).await?;
    if new.is_import_target {
        sqlx::query("UPDATE library_root SET is_import_target = 0 WHERE library_id = ?")
            .bind(library_id)
            .execute(&mut *tx)
            .await?;
    }
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO library_root (library_id, path, write_policy, layout_template, is_import_target)
         VALUES (?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(library_id)
    .bind(path.to_string_lossy().into_owned())
    .bind(policy.as_str())
    .bind(template.source())
    .bind(new.is_import_target)
    .fetch_one(&mut *tx)
    .await?;
    events::emit(
        &mut tx,
        events::ROOT_ADDED,
        json!({ "root_id": id, "library_id": library_id }),
    )
    .await?;
    tx.commit().await?;
    root(pool, id).await
}

pub async fn root(pool: &SqlitePool, id: i64) -> Result<Root, LibraryError> {
    list(pool)
        .await?
        .into_iter()
        .flat_map(|l| l.roots)
        .find(|r| r.id == id)
        .ok_or(LibraryError::NotFound)
}

#[derive(Default)]
pub struct RootChanges<'a> {
    pub write_policy: Option<&'a str>,
    pub layout_template: Option<&'a str>,
    pub is_import_target: Option<bool>,
}

pub async fn update_root(
    pool: &SqlitePool,
    id: i64,
    changes: RootChanges<'_>,
) -> Result<Root, LibraryError> {
    let current = root(pool, id).await?;
    let library = get(pool, current.library_id).await?;
    let template = match changes.layout_template {
        Some(t) => Template::parse(t, kind_of(&library.kind))
            .map_err(|e| LibraryError::Invalid(e.0))?
            .source(),
        None => current.layout_template.clone(),
    };
    let policy = match changes.write_policy {
        Some(p) => WritePolicy::parse(p).ok_or_else(|| {
            LibraryError::Invalid("Write policy is managed, adaptive or read_only.".into())
        })?,
        None => WritePolicy::parse(&current.write_policy).unwrap_or(WritePolicy::Adaptive),
    };
    let import_target = changes.is_import_target.unwrap_or(current.is_import_target);
    if import_target && policy == WritePolicy::ReadOnly {
        return invalid("A read-only folder can't be the import target.");
    }

    let mut tx = crate::db::begin_write(pool).await?;
    if import_target && !current.is_import_target {
        sqlx::query("UPDATE library_root SET is_import_target = 0 WHERE library_id = ?")
            .bind(current.library_id)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query(
        "UPDATE library_root SET write_policy = ?, layout_template = ?, is_import_target = ? WHERE id = ?",
    )
    .bind(policy.as_str())
    .bind(&template)
    .bind(import_target)
    .bind(id)
    .execute(&mut *tx)
    .await?;
    events::emit(&mut tx, events::ROOT_CHANGED, json!({ "root_id": id })).await?;
    tx.commit().await?;
    root(pool, id).await
}

/// Removes a root and the files recorded on it. Refused while any of its files
/// are linked to items or a series has a folder on it: removing it would lose
/// matches the user may have made.
pub async fn remove_root(pool: &SqlitePool, id: i64) -> Result<(), LibraryError> {
    let (linked, locations): (i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM file_link l JOIN media_file f ON f.id = l.file_id WHERE f.root_id = ?1),
                (SELECT count(*) FROM series_location WHERE root_id = ?1)",
    )
    .bind(id)
    .fetch_one(pool)
    .await?;
    if linked > 0 || locations > 0 {
        return Err(LibraryError::Conflict(format!(
            "This folder holds {linked} linked files and {locations} series folders. \
             Move or unlink them first; removing it would lose those matches."
        )));
    }
    let mut tx = crate::db::begin_write(pool).await?;
    sqlx::query("DELETE FROM match_candidate WHERE root_id = ?1 OR file_id IN (SELECT id FROM media_file WHERE root_id = ?1)")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM media_file WHERE root_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    let done = sqlx::query("DELETE FROM library_root WHERE id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    if done.rows_affected() == 0 {
        return Err(LibraryError::NotFound);
    }
    events::emit(&mut tx, events::ROOT_REMOVED, json!({ "root_id": id })).await?;
    tx.commit().await?;
    Ok(())
}

// ── Placement example ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PolicyPlacement {
    /// `managed`, `adaptive` or `read_only`.
    pub policy: String,
    #[serde(flatten)]
    pub placement: Placement,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PlacementExample {
    pub series_title: String,
    /// e.g. `S02E01`.
    pub episode_label: String,
    pub episode_title: String,
    /// The name the file arrives with.
    pub filename: String,
    pub root_path: String,
    /// The series' stored folder on that root.
    pub location: String,
    pub outcomes: Vec<PolicyPlacement>,
}

#[derive(sqlx::FromRow)]
struct ExampleRow {
    series_id: i64,
    series: String,
    year: Option<i64>,
    location: String,
    root_id: i64,
    episode_id: i64,
    episode_title: String,
}

/// "Where does a new file go?": the next episode a series in this library doesn't
/// have, placed under each write policy on the root holding the series' folder.
pub async fn placement_example(
    pool: &SqlitePool,
    library_id: i64,
) -> Result<Option<PlacementExample>, LibraryError> {
    let library = get(pool, library_id).await?;
    let kind = kind_of(&library.kind);
    let candidate: Option<ExampleRow> = sqlx::query_as(
        "SELECT s.id AS series_id, s.title AS series, s.year, sl.path AS location, r.id AS root_id,
                e.id AS episode_id, e.title AS episode_title
         FROM series_location sl
         JOIN library_root r ON r.id = sl.root_id
         JOIN item s ON s.id = sl.series_item_id
         JOIN ordering o ON o.series_id = s.id AND o.is_default
         JOIN ordering_entry oe ON oe.ordering_id = o.id
         JOIN item e ON e.id = oe.item_id
         WHERE r.library_id = ? AND sl.is_primary
           AND NOT EXISTS (SELECT 1 FROM file_link l JOIN media_file f ON f.id = l.file_id
                           WHERE l.item_id = e.id AND f.status = 'present')
         ORDER BY e.airs_at IS NULL, e.airs_at, oe.position
         LIMIT 1",
    )
    .bind(library_id)
    .fetch_optional(pool)
    .await?;
    let Some(ExampleRow {
        series_id,
        series,
        year,
        location,
        root_id,
        episode_id,
        episode_title,
    }) = candidate
    else {
        return Ok(None);
    };
    let (season, number): (Option<i64>, Option<i64>) = sqlx::query_as(
        "SELECT os.number, oe.episode_number FROM ordering_entry oe
         JOIN ordering o ON o.id = oe.ordering_id AND o.is_default AND o.series_id = ?
         LEFT JOIN ordering_season os ON os.id = oe.ordering_season_id
         WHERE oe.item_id = ? LIMIT 1",
    )
    .bind(series_id)
    .bind(episode_id)
    .fetch_one(pool)
    .await?;
    let root = library
        .roots
        .iter()
        .find(|r| r.id == root_id)
        .ok_or(LibraryError::NotFound)?;
    let template =
        Template::parse(&root.layout_template, kind).map_err(|e| LibraryError::Invalid(e.0))?;

    // A folder already holding this season's files, for adaptive placement.
    let files: Vec<String> = sqlx::query_scalar(
        "SELECT f.relative_path FROM media_file f
         JOIN file_link l ON l.file_id = f.id JOIN item e ON e.id = l.item_id
         WHERE e.parent_id = ? AND f.root_id = ? AND f.status = 'present'",
    )
    .bind(series_id)
    .bind(root_id)
    .fetch_all(pool)
    .await?;
    let sibling = files.iter().find_map(|path| {
        let g = layout::group(&template, path)?;
        (g.hints.season == season && season.is_some())
            .then(|| path.rsplit_once('/').map(|(dir, _)| dir.to_string()))?
    });

    let label = match (season, number) {
        (Some(s), Some(e)) => format!("S{s:02}E{e:02}"),
        (None, Some(e)) => format!("E{e:02}"),
        _ => String::new(),
    };
    let filename = format!(
        "{}.{label}.1080p.WEB-DL.mkv",
        series.split_whitespace().collect::<Vec<_>>().join(".")
    );
    let mut values = HashMap::from([("series", series.clone()), ("title", episode_title.clone())]);
    if let Some(y) = year {
        values.insert("year", y.to_string());
    }
    if let Some(s) = season {
        values.insert("season", s.to_string());
    }
    if let Some(e) = number {
        values.insert("episode", e.to_string());
        values.insert("absolute", e.to_string());
    }
    values.insert("resolution", "1080p".into());
    let incoming = Incoming {
        filename: &filename,
        values,
    };

    let mut outcomes = Vec::new();
    for policy in [
        WritePolicy::Adaptive,
        WritePolicy::Managed,
        WritePolicy::ReadOnly,
    ] {
        let placement = layout::place(
            policy,
            &template,
            Some(&location),
            sibling.as_deref(),
            &incoming,
        )
        .unwrap_or_else(|e| Placement {
            path: None,
            note: format!("The template can't name this file: {}", e.0),
        });
        outcomes.push(PolicyPlacement {
            policy: policy.as_str().into(),
            placement,
        });
    }
    Ok(Some(PlacementExample {
        series_title: series,
        episode_label: label,
        episode_title,
        filename,
        root_path: root.path.clone(),
        location,
        outcomes,
    }))
}

// ── Folder browsing, for picking a root ────────────────────────────────────

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct FolderListing {
    pub path: String,
    pub parent: Option<String>,
    /// Subfolder names, hidden ones left out.
    pub folders: Vec<String>,
}

pub async fn list_folders(path: &str) -> Result<FolderListing, LibraryError> {
    let path = if path.trim().is_empty() {
        "/"
    } else {
        path.trim()
    };
    let dir = Path::new(path);
    if !dir.is_absolute() {
        return invalid("Use a full path, starting with /.");
    }
    let mut entries = tokio::fs::read_dir(dir)
        .await
        .map_err(|e| LibraryError::Invalid(format!("Can't open {path}: {e}")))?;
    let mut folders = Vec::new();
    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with('.')
            && entry
                .file_type()
                .await
                .is_ok_and(|t| t.is_dir() || t.is_symlink())
            && is_dir(&entry.path().to_string_lossy()).await
        {
            folders.push(name);
        }
    }
    folders.sort_by_key(|f| f.to_lowercase());
    Ok(FolderListing {
        path: dir.to_string_lossy().into_owned(),
        parent: dir.parent().map(|p| p.to_string_lossy().into_owned()),
        folders,
    })
}
