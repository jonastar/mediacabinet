//! Writing to the library: importing files, organising roots by their template,
//! and moving a series to another root. Nothing here runs on its own: every
//! operation is asked for, and can be previewed first (`dry_run`). See
//! `docs/library-management.md`.
//!
//! Safety rules:
//! - Never overwrite. If a destination exists, the operation stops.
//! - Within a filesystem, moves are renames. Across filesystems, files are
//!   copied, the copy's size checked, then the original removed.
//! - File rows are updated in place, so links and watch history follow the file.
//! - Read-only roots are never written; imports go to the library's root for new
//!   items instead.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::SqlitePool;
use utoipa::ToSchema;

use crate::events;
use crate::layout::{self, Incoming, LibraryKind, Template, WritePolicy};
use crate::parse::parse;
use crate::scan::{self, Found};

#[derive(Debug, thiserror::Error)]
pub enum OrganizeError {
    #[error("not found")]
    NotFound,
    #[error("{0}")]
    Invalid(String),
    /// The destination exists, or another conflict on disk.
    #[error("{0}")]
    Conflict(String),
    #[error("file operation failed: {0}")]
    Io(#[from] io::Error),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

fn invalid<T>(m: impl Into<String>) -> Result<T, OrganizeError> {
    Err(OrganizeError::Invalid(m.into()))
}

// ── File operations ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TransferMode {
    /// A hard link: no extra space, the download client keeps seeding. Falls back
    /// to a copy across filesystems.
    #[default]
    Hardlink,
    Copy,
    Move,
}

fn is_cross_device(e: &io::Error) -> bool {
    e.raw_os_error() == Some(18) // EXDEV
}

/// Copies and checks the size, removing a partial copy on failure.
fn copy_checked(from: &Path, to: &Path) -> io::Result<()> {
    let expected = std::fs::metadata(from)?.len();
    let result = std::fs::copy(from, to).and_then(|n| {
        if n == expected && std::fs::metadata(to)?.len() == expected {
            Ok(())
        } else {
            Err(io::Error::other("copy is incomplete"))
        }
    });
    if result.is_err() {
        let _ = std::fs::remove_file(to);
    }
    result
}

/// Transfers one file. The destination must not exist.
pub fn transfer(from: &Path, to: &Path, mode: TransferMode) -> io::Result<()> {
    if to.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("{} already exists", to.display()),
        ));
    }
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    match mode {
        TransferMode::Hardlink => match std::fs::hard_link(from, to) {
            Ok(()) => Ok(()),
            Err(e) if is_cross_device(&e) => copy_checked(from, to),
            Err(e) => Err(e),
        },
        TransferMode::Copy => copy_checked(from, to),
        TransferMode::Move => match std::fs::rename(from, to) {
            Ok(()) => Ok(()),
            Err(e) if is_cross_device(&e) => {
                copy_checked(from, to)?;
                std::fs::remove_file(from)
            }
            Err(e) => Err(e),
        },
    }
}

/// Removes now-empty folders from `dir` up to (not including) `stop`.
fn prune_empty(mut dir: &Path, stop: &Path) {
    while dir != stop && dir.starts_with(stop) {
        if std::fs::remove_dir(dir).is_err() {
            break;
        }
        match dir.parent() {
            Some(p) => dir = p,
            None => break,
        }
    }
}

/// Files next to `video` sharing its name: subtitles, .nfo, thumbnails.
fn sidecars_of(video: &Path) -> Vec<PathBuf> {
    let (Some(dir), Some(stem)) = (video.parent(), video.file_stem().and_then(|s| s.to_str()))
    else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p != video && p.is_file())
        .filter(|p| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
            name.strip_prefix(stem)
                .is_some_and(|rest| rest.starts_with('.') || rest.starts_with('-'))
                && crate::scan::is_sidecar(p)
        })
        .collect()
}

/// Where a sidecar goes when its video moves from `old_video` to `new_video`.
fn sidecar_destination(sidecar: &Path, old_video: &Path, new_video: &Path) -> Option<PathBuf> {
    let old_stem = old_video.file_stem()?.to_str()?;
    let new_stem = new_video.file_stem()?.to_str()?;
    let name = sidecar.file_name()?.to_str()?;
    let rest = name.strip_prefix(old_stem)?;
    Some(new_video.with_file_name(format!("{new_stem}{rest}")))
}

/// How an imported file is named.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Naming {
    /// Renamed and placed by the root's layout.
    Layout,
    /// Keeps the name it arrived with: next to the files of its season if there
    /// are any, otherwise in the series' folder made by the layout.
    Keep,
}

impl Naming {
    fn policy(self) -> WritePolicy {
        match self {
            Naming::Layout => WritePolicy::Managed,
            Naming::Keep => WritePolicy::Adaptive,
        }
    }
}

// ── Roots and locations ────────────────────────────────────────────────────

#[derive(Debug, Clone, sqlx::FromRow)]
struct RootRow {
    id: i64,
    path: String,
    write_policy: String,
    layout_template: String,
    kind: String,
}

impl RootRow {
    fn policy(&self) -> WritePolicy {
        WritePolicy::parse(&self.write_policy).unwrap_or(WritePolicy::Adaptive)
    }

    fn template(&self) -> Result<Template, OrganizeError> {
        Template::parse(
            &self.layout_template,
            LibraryKind::parse(&self.kind).unwrap_or(LibraryKind::Tv),
        )
        .map_err(|e| OrganizeError::Invalid(e.0))
    }
}

const ROOT_SELECT: &str = "SELECT r.id, r.path, r.write_policy, r.layout_template, l.kind
     FROM library_root r JOIN library l ON l.id = r.library_id";

async fn root(pool: &SqlitePool, id: i64) -> Result<RootRow, OrganizeError> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!("{ROOT_SELECT} WHERE r.id = ?")))
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or(OrganizeError::NotFound)
}

/// The root of `library` new items go to (stored as its import target).
async fn import_target(pool: &SqlitePool, library: i64) -> Result<RootRow, OrganizeError> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{ROOT_SELECT} WHERE l.id = ? AND r.is_import_target AND r.write_policy <> 'read_only'"
    )))
    .bind(library)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| {
        OrganizeError::Invalid(
            "Its library has no folder for new items; choose one in the library's settings.".into(),
        )
    })
}

/// Token values for a series or movie and (for episodes) an episode.
async fn values(
    pool: &SqlitePool,
    item: i64,
) -> Result<HashMap<&'static str, String>, OrganizeError> {
    let (kind, title, year, parent): (String, String, Option<i64>, Option<i64>) =
        sqlx::query_as("SELECT kind, title, year, parent_id FROM item WHERE id = ?")
            .bind(item)
            .fetch_optional(pool)
            .await?
            .ok_or(OrganizeError::NotFound)?;
    let mut v = HashMap::new();
    let top = parent.unwrap_or(item);
    let (top_title, top_year): (String, Option<i64>) =
        sqlx::query_as("SELECT title, year FROM item WHERE id = ?")
            .bind(top)
            .fetch_one(pool)
            .await?;
    v.insert(
        if kind == "movie" {
            "movie"
        } else {
            "series"
        },
        top_title,
    );
    if let Some(y) = top_year.or(year) {
        v.insert("year", y.to_string());
    }
    if let Some(genre) = sqlx::query_scalar::<_, String>(
        "SELECT substr(tag, 7) FROM item_tag WHERE item_id = ? AND tag LIKE 'genre:%' ORDER BY rowid LIMIT 1",
    )
    .bind(top)
    .fetch_optional(pool)
    .await?
    {
        v.insert("genre", genre);
    }
    if kind == "episode" {
        v.insert("title", title);
        let numbers: Vec<(String, Option<i64>, Option<i64>)> = sqlx::query_as(
            "SELECT o.kind, os.number, oe.episode_number FROM ordering_entry oe
             JOIN ordering o ON o.id = oe.ordering_id
             LEFT JOIN ordering_season os ON os.id = oe.ordering_season_id
             WHERE oe.item_id = ? ORDER BY o.is_default DESC, o.id",
        )
        .bind(item)
        .fetch_all(pool)
        .await?;
        if let Some((_, season, number)) = numbers.iter().find(|(k, _, _)| k != "absolute") {
            if let Some(s) = season {
                v.insert("season", s.to_string());
            }
            if let Some(n) = number {
                v.insert("episode", n.to_string());
            }
        }
        if let Some((_, _, Some(n))) = numbers.iter().find(|(k, _, _)| k == "absolute") {
            v.insert("absolute", n.to_string());
        }
    }
    Ok(v)
}

/// A series' folder on a root, resolved and stored once if it has none there.
async fn series_folder(
    pool: &SqlitePool,
    series: i64,
    root: &RootRow,
) -> Result<String, OrganizeError> {
    if let Some(path) = sqlx::query_scalar::<_, String>(
        "SELECT path FROM series_location WHERE series_item_id = ? AND root_id = ? ORDER BY is_primary DESC LIMIT 1",
    )
    .bind(series)
    .bind(root.id)
    .fetch_optional(pool)
    .await?
    {
        return Ok(path);
    }
    let template = root.template()?;
    let values = values(pool, series).await?;
    let folder = template
        .render_segments(0..template.item_index + 1, &values)
        .map_err(|e| OrganizeError::Invalid(e.0))?
        .join("/");
    crate::matcher::store_location(pool, series, root.id, &folder).await?;
    Ok(folder)
}

// ── Import ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ImportResult {
    pub file_id: i64,
    pub root_id: i64,
    pub path: String,
    pub items: Vec<i64>,
    pub note: String,
    pub sidecars: Vec<String>,
    /// Files of the same items removed because `replace` was asked.
    pub replaced: Vec<String>,
}

/// What to import and how.
#[derive(Debug, Clone, Copy)]
pub struct ImportOptions {
    pub mode: TransferMode,
    /// None follows the destination root's stored policy: `managed` renames,
    /// `adaptive` keeps names. That keeps older API clients behaving the same.
    pub naming: Option<Naming>,
    /// Remove the items' existing files (an upgrade).
    pub replace: bool,
    pub dry_run: bool,
}

/// Imports a file for `items` (episodes of one series, or a movie): places it by
/// the naming asked for and the destination root's template, transfers it with
/// its sidecars, records, probes and links it.
pub async fn import_file(
    pool: &SqlitePool,
    prober: &scan::Prober,
    source: &Path,
    items: &[i64],
    options: ImportOptions,
) -> Result<ImportResult, OrganizeError> {
    let ImportOptions {
        mode,
        naming,
        replace,
        dry_run,
    } = options;
    if !source.is_absolute() || !source.is_file() {
        return invalid(format!(
            "{} isn't a file the server can read.",
            source.display()
        ));
    }
    if !scan::is_video(source) {
        return invalid("Only video files can be imported; sidecars travel with their video.");
    }
    let Some(&first) = items.first() else {
        return invalid("Name the episode(s) or movie the file is.");
    };
    let (kind, parent): (String, Option<i64>) =
        sqlx::query_as("SELECT kind, parent_id FROM item WHERE id = ?")
            .bind(first)
            .fetch_optional(pool)
            .await?
            .ok_or(OrganizeError::NotFound)?;
    let top = match kind.as_str() {
        "episode" => parent.expect("episodes have a series"),
        "movie" => first,
        _ => return invalid("Import into episodes or a movie, not a series."),
    };
    for item in &items[1..] {
        let p: Option<i64> = sqlx::query_scalar("SELECT parent_id FROM item WHERE id = ?")
            .bind(item)
            .fetch_optional(pool)
            .await?
            .ok_or(OrganizeError::NotFound)?;
        if kind == "movie" || p != Some(top) {
            return invalid("A file can cover several episodes of one series, or one movie.");
        }
    }
    let library: Option<i64> = sqlx::query_scalar("SELECT library_id FROM item WHERE id = ?")
        .bind(top)
        .fetch_one(pool)
        .await?;
    let Some(library) = library else {
        return invalid("It isn't in a library.");
    };

    // Where: the series' primary folder's root if writable, else the root for new items.
    let primary: Option<i64> = sqlx::query_scalar(
        "SELECT root_id FROM series_location WHERE series_item_id = ? ORDER BY is_primary DESC LIMIT 1",
    )
    .bind(top)
    .fetch_optional(pool)
    .await?;
    let movie_root: Option<i64> = if kind == "movie" {
        sqlx::query_scalar(
            "SELECT f.root_id FROM file_link l JOIN media_file f ON f.id = l.file_id
             JOIN library_root r ON r.id = f.root_id
             WHERE l.item_id = ? AND f.status = 'present' AND r.write_policy <> 'read_only' LIMIT 1",
        )
        .bind(top)
        .fetch_optional(pool)
        .await?
    } else {
        None
    };
    let mut dest_root = match primary.or(movie_root) {
        Some(id) => root(pool, id).await?,
        None => import_target(pool, library).await?,
    };
    let mut read_only_note = None;
    if dest_root.policy() == WritePolicy::ReadOnly {
        read_only_note = Some(format!(
            "{} is read-only, so it went to the folder for new items.",
            dest_root.path
        ));
        dest_root = import_target(pool, library).await?;
    }
    let template = dest_root.template()?;

    let filename = source
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("video.mkv")
        .to_string();
    let mut v = values(pool, first).await?;
    let parsed = parse(&filename);
    if let Some(r) = parsed.resolution.clone() {
        v.insert("resolution", r);
    }
    if let Some(e) = parsed.edition.clone() {
        v.insert("edition", e);
    }
    if items.len() > 1 {
        // A multi-episode file: name it after the first, as the template allows.
        v.entry("title")
            .and_modify(|t| t.push_str(" (multi-episode)"));
    }

    let (location, sibling) = if kind == "episode" {
        let folder = if dry_run {
            sqlx::query_scalar::<_, String>(
                "SELECT path FROM series_location WHERE series_item_id = ? AND root_id = ? LIMIT 1",
            )
            .bind(top)
            .bind(dest_root.id)
            .fetch_optional(pool)
            .await?
        } else {
            Some(series_folder(pool, top, &dest_root).await?)
        };
        // A folder already holding the same season, for adaptive placement.
        let season = v.get("season").cloned();
        let files: Vec<String> = sqlx::query_scalar(
            "SELECT f.relative_path FROM media_file f JOIN file_link l ON l.file_id = f.id
             JOIN item e ON e.id = l.item_id
             WHERE e.parent_id = ? AND f.root_id = ? AND f.status = 'present'",
        )
        .bind(top)
        .bind(dest_root.id)
        .fetch_all(pool)
        .await?;
        let sibling = files.iter().find_map(|path| {
            let g = layout::group(&template, path)?;
            (season.is_some() && g.hints.season.map(|s| s.to_string()) == season)
                .then(|| path.rsplit_once('/').map(|(d, _)| d.to_string()))?
        });
        (folder, sibling)
    } else {
        (None, None)
    };

    let policy = match naming {
        Some(n) => n.policy(),
        None => match dest_root.policy() {
            WritePolicy::Managed => WritePolicy::Managed,
            _ => WritePolicy::Adaptive,
        },
    };
    let placement = layout::place(
        policy,
        &template,
        location.as_deref(),
        sibling.as_deref(),
        &Incoming {
            filename: &filename,
            values: v,
        },
    )
    .map_err(|e| OrganizeError::Invalid(e.0))?;
    let Some(relative) = placement.path.clone() else {
        return invalid("The destination root is read-only.");
    };
    let destination = Path::new(&dest_root.path).join(&relative);
    if destination.exists() {
        return Err(OrganizeError::Conflict(format!(
            "{} already exists.",
            destination.display()
        )));
    }
    let note = match read_only_note {
        Some(ro) => format!("{ro} {}", placement.note),
        None => placement.note.clone(),
    };

    // Older files of the same items, to replace if asked.
    let old: Vec<(i64, String, String)> = if replace {
        let list = serde_json::to_string(items).expect("ids serialise");
        sqlx::query_as(
            "SELECT DISTINCT f.id, r.path, f.relative_path FROM file_link l
             JOIN media_file f ON f.id = l.file_id JOIN library_root r ON r.id = f.root_id
             WHERE l.item_id IN (SELECT value FROM json_each(?)) AND f.status = 'present'",
        )
        .bind(list)
        .fetch_all(pool)
        .await?
    } else {
        Vec::new()
    };

    let sidecar_moves: Vec<(PathBuf, PathBuf)> = sidecars_of(source)
        .into_iter()
        .filter_map(|s| sidecar_destination(&s, source, &destination).map(|d| (s, d)))
        .collect();
    let rel = |p: &Path| scan::relative_path(Path::new(&dest_root.path), p).unwrap_or_default();
    let mut result = ImportResult {
        file_id: 0,
        root_id: dest_root.id,
        path: relative.clone(),
        items: items.to_vec(),
        note,
        sidecars: sidecar_moves.iter().map(|(_, d)| rel(d)).collect(),
        replaced: old
            .iter()
            .map(|(_, r, p)| format!("{}/{p}", r.trim_end_matches('/')))
            .collect(),
    };
    if dry_run {
        return Ok(result);
    }

    let (src, dst) = (source.to_path_buf(), destination.clone());
    let moves = sidecar_moves.clone();
    tokio::task::spawn_blocking(move || -> io::Result<()> {
        transfer(&src, &dst, mode)?;
        for (s, d) in &moves {
            if !d.exists() {
                transfer(s, d, mode)?;
            }
        }
        Ok(())
    })
    .await
    .map_err(|e| io::Error::other(e.to_string()))??;

    let file_id = record_file(pool, dest_root.id, &relative).await?;
    let mut tx = crate::db::begin_write(pool).await?;
    for item in items {
        sqlx::query(
            "INSERT INTO file_link (file_id, item_id, library_id, edition, part, confidence, source, pinned)
             SELECT ?1, ?2, library_id, ?3, ?4, 1, 'path_id', 1 FROM media_file WHERE id = ?1
             ON CONFLICT DO NOTHING",
        )
        .bind(file_id)
        .bind(item)
        .bind(&parsed.edition)
        .bind(parsed.part)
        .execute(&mut *tx)
        .await?;
        events::emit(
            &mut tx,
            events::LINK_CREATED,
            json!({ "file_id": file_id, "item_id": item, "source": "import" }),
        )
        .await?;
    }
    for (id, root_path, path) in &old {
        let full = Path::new(root_path).join(path);
        match std::fs::remove_file(&full) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        for s in sidecars_of(&full) {
            let _ = std::fs::remove_file(s);
        }
        sqlx::query("UPDATE media_file SET status = 'removed' WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        events::emit(
            &mut tx,
            events::FILE_REMOVED,
            json!({ "file_id": id, "path": path, "replaced_by": file_id }),
        )
        .await?;
    }
    if !old.is_empty() {
        events::emit(
            &mut tx,
            events::FILE_UPGRADED,
            json!({ "file_id": file_id, "items": items }),
        )
        .await?;
    }
    crate::wanted::sync_events(&mut tx).await?;
    tx.commit().await?;

    // Streams and sidecars, now that the file is in place.
    let _ = scan::probe_root(pool, dest_root.id, prober).await;
    result.file_id = file_id;
    Ok(result)
}

/// Records one new file on a root without a full scan; returns its ID.
async fn record_file(
    pool: &SqlitePool,
    root_id: i64,
    relative: &str,
) -> Result<i64, OrganizeError> {
    let root_path: String = sqlx::query_scalar("SELECT path FROM library_root WHERE id = ?")
        .bind(root_id)
        .fetch_one(pool)
        .await?;
    let full = Path::new(&root_path).join(relative);
    let found: Found = scan::stat(Path::new(&root_path), &full)?;
    let hash = scan::partial_hash(&full)?;
    let mut tx = crate::db::begin_write(pool).await?;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO media_file
             (root_id, library_id, relative_path, size, mtime, device_id, inode, content_hash)
         SELECT ?1, library_id, ?2, ?3, ?4, ?5, ?6, ?7 FROM library_root WHERE id = ?1
         RETURNING id",
    )
    .bind(root_id)
    .bind(relative)
    .bind(found.size)
    .bind(found.mtime)
    .bind(found.device_id)
    .bind(found.inode)
    .bind(hash)
    .fetch_one(&mut *tx)
    .await?;
    events::emit(
        &mut tx,
        events::FILE_ADDED,
        json!({ "file_id": id, "root_id": root_id, "path": relative }),
    )
    .await?;
    tx.commit().await?;
    Ok(id)
}

// ── Organise and move ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PlannedMove {
    pub file_id: i64,
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MovePlan {
    pub moves: Vec<PlannedMove>,
    /// Files left where they are, and why.
    pub skipped: Vec<String>,
    pub applied: bool,
}

#[derive(sqlx::FromRow)]
struct LinkedFileRow {
    id: i64,
    root_id: i64,
    relative_path: String,
    item_id: i64,
}

async fn files_of(pool: &SqlitePool, top: i64) -> Result<Vec<LinkedFileRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT f.id, f.root_id, f.relative_path, min(l.item_id) AS item_id
         FROM media_file f JOIN file_link l ON l.file_id = f.id JOIN item i ON i.id = l.item_id
         WHERE (i.id = ?1 OR i.parent_id = ?1) AND f.status = 'present'
         GROUP BY f.id ORDER BY f.relative_path",
    )
    .bind(top)
    .fetch_all(pool)
    .await
}

/// What Organise works on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrganizeScope {
    Library(i64),
    Root(i64),
    /// A series or a movie.
    Item(i64),
}

/// One series or movie's changes on one root.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct OrganizeGroup {
    pub item_id: i64,
    /// `series` or `movie`.
    pub kind: String,
    pub title: String,
    pub year: Option<i64>,
    pub root_id: i64,
    pub root_path: String,
    /// Paths relative to the root.
    pub moves: Vec<PlannedMove>,
    /// Files left where they are, and why.
    pub skipped: Vec<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct OrganizePreview {
    /// Only series and movies with something to change or skip, by title.
    pub groups: Vec<OrganizeGroup>,
    /// Files looked at.
    pub files: usize,
    /// Of those, already where the layout puts them.
    pub in_place: usize,
}

/// Roots read once per plan.
#[derive(Default)]
struct Roots(HashMap<i64, RootRow>);

impl Roots {
    async fn get(&mut self, pool: &SqlitePool, id: i64) -> Result<&RootRow, OrganizeError> {
        if let std::collections::hash_map::Entry::Vacant(e) = self.0.entry(id) {
            e.insert(root(pool, id).await?);
        }
        Ok(&self.0[&id])
    }
}

/// A series' folder on a root without storing one: the stored folder, or the
/// one the layout would make.
async fn series_folder_preview(
    pool: &SqlitePool,
    series: i64,
    root: &RootRow,
) -> Result<String, OrganizeError> {
    if let Some(path) = sqlx::query_scalar::<_, String>(
        "SELECT path FROM series_location WHERE series_item_id = ? AND root_id = ? ORDER BY is_primary DESC LIMIT 1",
    )
    .bind(series)
    .bind(root.id)
    .fetch_optional(pool)
    .await?
    {
        return Ok(path);
    }
    let template = root.template()?;
    Ok(template
        .render_segments(0..template.item_index + 1, &values(pool, series).await?)
        .map_err(|e| OrganizeError::Invalid(e.0))?
        .join("/"))
}

/// Plans renaming one series' or movie's files to the layout, grouped by root.
/// With `store`, a series with no folder on a root gets one stored, as applying
/// needs; a preview leaves the database alone.
async fn plan_item(
    pool: &SqlitePool,
    roots: &mut Roots,
    top: i64,
    only_root: Option<i64>,
    store: bool,
) -> Result<(Vec<OrganizeGroup>, usize, usize), OrganizeError> {
    let (kind, title, year): (String, String, Option<i64>) =
        sqlx::query_as("SELECT kind, title, year FROM item WHERE id = ?")
            .bind(top)
            .fetch_optional(pool)
            .await?
            .ok_or(OrganizeError::NotFound)?;
    if kind != "series" && kind != "movie" {
        return invalid("Organise a series or a movie.");
    }
    let mut groups: Vec<OrganizeGroup> = Vec::new();
    let mut targets = std::collections::HashSet::new();
    let (mut files, mut in_place) = (0, 0);
    for f in files_of(pool, top).await? {
        if only_root.is_some_and(|r| r != f.root_id) {
            continue;
        }
        files += 1;
        let r = roots.get(pool, f.root_id).await?.clone();
        let group = match groups.iter().position(|g| g.root_id == r.id) {
            Some(i) => &mut groups[i],
            None => {
                groups.push(OrganizeGroup {
                    item_id: top,
                    kind: kind.clone(),
                    title: title.clone(),
                    year,
                    root_id: r.id,
                    root_path: r.path.clone(),
                    moves: Vec::new(),
                    skipped: Vec::new(),
                });
                groups.last_mut().expect("just pushed")
            }
        };
        if r.policy() == WritePolicy::ReadOnly {
            group.skipped.push(format!(
                "{}: on a read-only folder, files aren't renamed",
                f.relative_path
            ));
            continue;
        }
        let template = r.template()?;
        let location = match (kind.as_str(), store) {
            ("series", true) => Some(series_folder(pool, top, &r).await?),
            ("series", false) => Some(series_folder_preview(pool, top, &r).await?),
            _ => None,
        };
        let mut v = values(pool, f.item_id).await?;
        let name = f
            .relative_path
            .rsplit('/')
            .next()
            .unwrap_or(&f.relative_path)
            .to_string();
        let parsed = parse(&name);
        if let Some(res) = parsed.resolution {
            v.insert("resolution", res);
        }
        if let Some(e) = parsed.edition {
            v.insert("edition", e);
        }
        let placed = layout::place(
            WritePolicy::Managed,
            &template,
            location.as_deref(),
            None,
            &Incoming {
                filename: &name,
                values: v,
            },
        );
        let to = match placed {
            Ok(layout::Placement { path: Some(to), .. }) => to,
            Ok(_) => continue,
            Err(e) => {
                group.skipped.push(format!(
                    "{}: the layout can't name it ({})",
                    f.relative_path, e.0
                ));
                continue;
            }
        };
        if to == f.relative_path {
            in_place += 1;
            continue;
        }
        if !targets.insert((r.id, to.clone())) || Path::new(&r.path).join(&to).exists() {
            group
                .skipped
                .push(format!("{}: {to} is taken", f.relative_path));
            continue;
        }
        group.moves.push(PlannedMove {
            file_id: f.id,
            from: f.relative_path,
            to,
        });
    }
    Ok((groups, files, in_place))
}

/// Renames a series' or movie's files on writable roots to match the template.
pub async fn organize(
    pool: &SqlitePool,
    top: i64,
    dry_run: bool,
) -> Result<MovePlan, OrganizeError> {
    let (groups, _, _) = plan_item(pool, &mut Roots::default(), top, None, !dry_run).await?;
    let mut plan = MovePlan {
        moves: Vec::new(),
        skipped: Vec::new(),
        applied: false,
    };
    for g in groups {
        plan.moves.extend(g.moves);
        plan.skipped.extend(g.skipped);
    }
    if !dry_run {
        apply_moves(pool, &plan.moves, None).await?;
        plan.applied = true;
    }
    Ok(plan)
}

/// What renaming the files in `scope` to their root's layout would change.
/// Nothing is written; apply the moves wanted with [`apply_organize`].
pub async fn preview_organize(
    pool: &SqlitePool,
    scope: OrganizeScope,
) -> Result<OrganizePreview, OrganizeError> {
    let (items, only_root): (Vec<i64>, Option<i64>) = match scope {
        OrganizeScope::Item(id) => (vec![id], None),
        OrganizeScope::Library(id) => {
            let kind: String = sqlx::query_scalar("SELECT kind FROM library WHERE id = ?")
                .bind(id)
                .fetch_optional(pool)
                .await?
                .ok_or(OrganizeError::NotFound)?;
            if kind == "folders" {
                return invalid("A folder library is never organised.");
            }
            let items = sqlx::query_scalar(
                "SELECT DISTINCT coalesce(i.parent_id, i.id) FROM media_file f
                 JOIN file_link l ON l.file_id = f.id JOIN item i ON i.id = l.item_id
                 JOIN library_root r ON r.id = f.root_id
                 WHERE r.library_id = ? AND f.status = 'present'",
            )
            .bind(id)
            .fetch_all(pool)
            .await?;
            (items, None)
        }
        OrganizeScope::Root(id) => {
            let r = root(pool, id).await?;
            if r.kind == "folders" {
                return invalid("A folder library is never organised.");
            }
            if r.policy() == WritePolicy::ReadOnly {
                return invalid(format!(
                    "{} is read-only. Make it writable in its settings to organise it.",
                    r.path
                ));
            }
            let items = sqlx::query_scalar(
                "SELECT DISTINCT coalesce(i.parent_id, i.id) FROM media_file f
                 JOIN file_link l ON l.file_id = f.id JOIN item i ON i.id = l.item_id
                 WHERE f.root_id = ? AND f.status = 'present'",
            )
            .bind(id)
            .fetch_all(pool)
            .await?;
            (items, Some(id))
        }
    };
    let mut roots = Roots::default();
    let mut preview = OrganizePreview {
        groups: Vec::new(),
        files: 0,
        in_place: 0,
    };
    for item in items {
        let (groups, files, in_place) = plan_item(pool, &mut roots, item, only_root, false).await?;
        preview.files += files;
        preview.in_place += in_place;
        preview.groups.extend(
            groups
                .into_iter()
                .filter(|g| !g.moves.is_empty() || !g.skipped.is_empty()),
        );
    }
    preview.groups.sort_by(|a, b| {
        a.title
            .to_lowercase()
            .cmp(&b.title.to_lowercase())
            .then(a.year.cmp(&b.year))
            .then(a.root_path.cmp(&b.root_path))
    });
    Ok(preview)
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct FailedMove {
    pub file_id: i64,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct OrganizeApplied {
    pub moved: Vec<PlannedMove>,
    pub failed: Vec<FailedMove>,
}

/// Applies moves from a preview, each within its file's root. Each is checked
/// against the file as it is now: still at `from`, on a writable root, with
/// nothing at `to`. A move that fails is reported and the rest go on.
pub async fn apply_organize(
    pool: &SqlitePool,
    moves: &[PlannedMove],
) -> Result<OrganizeApplied, OrganizeError> {
    let mut roots = Roots::default();
    let mut done = OrganizeApplied {
        moved: Vec::new(),
        failed: Vec::new(),
    };
    let mut series_seen = std::collections::HashSet::new();
    for m in moves {
        let fail = |error: String| FailedMove {
            file_id: m.file_id,
            error,
        };
        let row: Option<(i64, String, String)> =
            sqlx::query_as("SELECT root_id, relative_path, status FROM media_file WHERE id = ?")
                .bind(m.file_id)
                .fetch_optional(pool)
                .await?;
        let Some((root_id, path, status)) = row else {
            done.failed
                .push(fail("The file is no longer known.".into()));
            continue;
        };
        if status != "present" || path != m.from {
            done.failed
                .push(fail(format!("{} changed since the preview.", m.from)));
            continue;
        }
        let r = roots.get(pool, root_id).await?.clone();
        if r.policy() == WritePolicy::ReadOnly {
            done.failed.push(fail(format!("{} is read-only.", r.path)));
            continue;
        }
        if m.to.is_empty()
            || Path::new(&m.to).is_absolute()
            || m.to.split('/').any(|part| part == ".." || part.is_empty())
        {
            done.failed
                .push(fail(format!("{} isn't a path inside the folder.", m.to)));
            continue;
        }
        // A series gets its folder on this root stored, as Organise would.
        if r.kind == "tv" {
            let series: Option<i64> = sqlx::query_scalar(
                "SELECT i.parent_id FROM file_link l JOIN item i ON i.id = l.item_id
                 WHERE l.file_id = ? AND i.parent_id IS NOT NULL LIMIT 1",
            )
            .bind(m.file_id)
            .fetch_optional(pool)
            .await?;
            if let Some(series) = series
                && series_seen.insert((series, root_id))
            {
                series_folder(pool, series, &r).await?;
            }
        }
        match apply_moves(pool, std::slice::from_ref(m), None).await {
            Ok(()) => done.moved.push(m.clone()),
            Err(OrganizeError::Db(e)) => return Err(e.into()),
            Err(e) => done.failed.push(fail(e.to_string())),
        }
    }
    Ok(done)
}

/// Moves a series' files to another root, into its folder there (resolved by
/// the template once), keeping their layout below the series folder. The new
/// folder becomes the primary location.
pub async fn move_series(
    pool: &SqlitePool,
    series: i64,
    to_root: i64,
    dry_run: bool,
) -> Result<MovePlan, OrganizeError> {
    let library: Option<i64> =
        sqlx::query_scalar("SELECT library_id FROM item WHERE id = ? AND kind = 'series'")
            .bind(series)
            .fetch_optional(pool)
            .await?
            .ok_or(OrganizeError::NotFound)?;
    let target = root(pool, to_root).await?;
    let target_library: i64 =
        sqlx::query_scalar("SELECT library_id FROM library_root WHERE id = ?")
            .bind(to_root)
            .fetch_one(pool)
            .await?;
    if library != Some(target_library) {
        return invalid("Move a series into a root of its own library.");
    }
    if target.policy() == WritePolicy::ReadOnly {
        return invalid("That root is read-only.");
    }
    if target.kind != "tv" {
        return invalid("Move a series into a TV library's root.");
    }
    let new_folder = if dry_run {
        match sqlx::query_scalar::<_, String>(
            "SELECT path FROM series_location WHERE series_item_id = ? AND root_id = ?",
        )
        .bind(series)
        .bind(to_root)
        .fetch_optional(pool)
        .await?
        {
            Some(p) => p,
            None => {
                let t = target.template()?;
                t.render_segments(0..t.item_index + 1, &values(pool, series).await?)
                    .map_err(|e| OrganizeError::Invalid(e.0))?
                    .join("/")
            }
        }
    } else {
        series_folder(pool, series, &target).await?
    };
    let locations: HashMap<i64, String> = sqlx::query_as::<_, (i64, String)>(
        "SELECT root_id, path FROM series_location WHERE series_item_id = ?",
    )
    .bind(series)
    .fetch_all(pool)
    .await?
    .into_iter()
    .collect();

    let mut plan = MovePlan {
        moves: Vec::new(),
        skipped: Vec::new(),
        applied: false,
    };
    for f in files_of(pool, series).await? {
        if f.root_id == to_root {
            continue;
        }
        let source_root = root(pool, f.root_id).await?;
        if source_root.policy() == WritePolicy::ReadOnly {
            plan.skipped.push(format!(
                "{}: on a read-only root; it stays, and keeps its location",
                f.relative_path
            ));
            continue;
        }
        // Keep the layout below the old series folder, if the file is inside it.
        let below = locations
            .get(&f.root_id)
            .and_then(|loc| f.relative_path.strip_prefix(&format!("{loc}/")))
            .map(str::to_string)
            .unwrap_or_else(|| {
                f.relative_path
                    .rsplit('/')
                    .next()
                    .unwrap_or(&f.relative_path)
                    .to_string()
            });
        let to = format!("{new_folder}/{below}");
        if Path::new(&target.path).join(&to).exists() {
            plan.skipped
                .push(format!("{}: {to} is taken", f.relative_path));
            continue;
        }
        plan.moves.push(PlannedMove {
            file_id: f.id,
            from: format!(
                "{}/{}",
                source_root.path.trim_end_matches('/'),
                f.relative_path
            ),
            to,
        });
    }
    if !dry_run {
        apply_moves(pool, &plan.moves, Some(&target)).await?;
        // The new folder is the primary location; old ones with no files left go.
        let mut tx = crate::db::begin_write(pool).await?;
        sqlx::query("UPDATE series_location SET is_primary = 0 WHERE series_item_id = ?")
            .bind(series)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE series_location SET is_primary = 1 WHERE series_item_id = ? AND root_id = ? AND path = ?")
            .bind(series)
            .bind(to_root)
            .bind(&new_folder)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "DELETE FROM series_location WHERE series_item_id = ?1 AND NOT (root_id = ?2 AND path = ?3)
               AND NOT EXISTS (SELECT 1 FROM media_file f JOIN file_link l ON l.file_id = f.id
                               JOIN item e ON e.id = l.item_id
                               WHERE e.parent_id = ?1 AND f.root_id = series_location.root_id
                                 AND f.status <> 'removed'
                                 AND f.relative_path LIKE series_location.path || '/%')",
        )
        .bind(series)
        .bind(to_root)
        .bind(&new_folder)
        .execute(&mut *tx)
        .await?;
        events::emit(
            &mut tx,
            events::SERIES_MOVED,
            json!({ "series_id": series, "root_id": to_root, "path": new_folder }),
        )
        .await?;
        tx.commit().await?;
        plan.applied = true;
    }
    Ok(plan)
}

/// A series folder (relative to its root) with the provider IDs in its name set
/// to `ids`; None when they already agree.
pub fn corrected_folder(path: &str, ids: &[(String, String)]) -> Option<String> {
    let (parent, name) = match path.rsplit_once('/') {
        Some((parent, name)) => (Some(parent), name),
        None => (None, path),
    };
    let name = layout::corrected_ids(name, ids)?;
    Some(match parent {
        Some(parent) => format!("{parent}/{name}"),
        None => name,
    })
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct FolderRename {
    pub root_id: i64,
    pub from: String,
    pub to: String,
}

/// Renames a series' folder on a root so the provider IDs in its name are the
/// series' own, e.g. after a wrong `[tmdb-…]` tag led to a wrong match. The
/// folder moves as a whole; file rows follow it. Read-only roots are never
/// written: turn read-only off first.
pub async fn fix_folder_ids(
    pool: &SqlitePool,
    series: i64,
    root_id: i64,
) -> Result<FolderRename, OrganizeError> {
    let from: String = sqlx::query_scalar(
        "SELECT path FROM series_location WHERE series_item_id = ? AND root_id = ?",
    )
    .bind(series)
    .bind(root_id)
    .fetch_optional(pool)
    .await?
    .ok_or(OrganizeError::NotFound)?;
    let r = root(pool, root_id).await?;
    if r.policy() == WritePolicy::ReadOnly {
        return invalid(format!(
            "{} is read-only. Turn read-only off in its settings to rename the folder.",
            r.path
        ));
    }
    let ids: Vec<(String, String)> =
        sqlx::query_as("SELECT provider, value FROM external_id WHERE item_id = ?")
            .bind(series)
            .fetch_all(pool)
            .await?;
    let Some(to) = corrected_folder(&from, &ids) else {
        return invalid("The folder's name already has the series' IDs.");
    };
    let (old, new) = (Path::new(&r.path).join(&from), Path::new(&r.path).join(&to));
    if !old.is_dir() {
        return Err(OrganizeError::Conflict(format!(
            "{} isn't there.",
            old.display()
        )));
    }
    if new.exists() {
        return Err(OrganizeError::Conflict(format!(
            "{} already exists.",
            new.display()
        )));
    }
    tokio::task::spawn_blocking(move || std::fs::rename(old, new))
        .await
        .map_err(|e| io::Error::other(e.to_string()))??;

    // Everything stored below the old folder on this root follows it.
    let (old_prefix, new_prefix) = (format!("{from}/"), format!("{to}/"));
    let mut tx = crate::db::begin_write(pool).await?;
    let moved: Vec<(i64, String)> = sqlx::query_as(
        "UPDATE media_file SET relative_path = ?3 || substr(relative_path, length(?2) + 1)
         WHERE root_id = ?1 AND substr(relative_path, 1, length(?2)) = ?2
         RETURNING id, relative_path",
    )
    .bind(root_id)
    .bind(&old_prefix)
    .bind(&new_prefix)
    .fetch_all(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE sidecar SET relative_path = ?3 || substr(relative_path, length(?2) + 1)
         WHERE root_id = ?1 AND location = 'root' AND substr(relative_path, 1, length(?2)) = ?2",
    )
    .bind(root_id)
    .bind(&old_prefix)
    .bind(&new_prefix)
    .execute(&mut *tx)
    .await?;
    for table in ["series_location", "ignored_folder", "match_candidate"] {
        let column = if table == "series_location" {
            "path"
        } else {
            "folder_path"
        };
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE {table} SET {column} = CASE WHEN {column} = ?2 THEN ?3
                 ELSE ?3 || substr({column}, length(?2) + 1) END
             WHERE root_id = ?1 AND ({column} = ?2 OR substr({column}, 1, length(?2) + 1) = ?2 || '/')"
        )))
        .bind(root_id)
        .bind(&from)
        .bind(&to)
        .execute(&mut *tx)
        .await?;
    }
    for (file_id, path) in &moved {
        let before = format!("{from}{}", &path[to.len()..]);
        events::emit(
            &mut tx,
            events::FILE_MOVED,
            json!({ "file_id": file_id, "root_id": root_id, "path": path, "from": before }),
        )
        .await?;
    }
    events::emit(
        &mut tx,
        events::SERIES_MOVED,
        json!({ "series_id": series, "root_id": root_id, "path": to, "from": from }),
    )
    .await?;
    tx.commit().await?;
    Ok(FolderRename { root_id, from, to })
}

/// Moves files on disk and updates their rows in place. Within a root, `from`
/// and `to` are relative to it; with `target`, `from` is absolute and `to` is
/// relative to the target root.
async fn apply_moves(
    pool: &SqlitePool,
    moves: &[PlannedMove],
    target: Option<&RootRow>,
) -> Result<(), OrganizeError> {
    for m in moves {
        let (root_id, root_path): (i64, String) = sqlx::query_as(
            "SELECT r.id, r.path FROM media_file f JOIN library_root r ON r.id = f.root_id WHERE f.id = ?",
        )
        .bind(m.file_id)
        .fetch_one(pool)
        .await?;
        let from = match target {
            Some(_) => PathBuf::from(&m.from),
            None => Path::new(&root_path).join(&m.from),
        };
        let (dest_root_id, dest_root_path) = match target {
            Some(t) => (t.id, t.path.clone()),
            None => (root_id, root_path.clone()),
        };
        let to = Path::new(&dest_root_path).join(&m.to);
        let sidecars: Vec<(PathBuf, PathBuf)> = sidecars_of(&from)
            .into_iter()
            .filter_map(|s| sidecar_destination(&s, &from, &to).map(|d| (s, d)))
            .collect();
        let (f2, t2) = (from.clone(), to.clone());
        tokio::task::spawn_blocking(move || -> io::Result<()> {
            transfer(&f2, &t2, TransferMode::Move)?;
            for (s, d) in &sidecars {
                if !d.exists() {
                    transfer(s, d, TransferMode::Move)?;
                }
            }
            if let Some(dir) = f2.parent() {
                prune_empty(dir, Path::new(&root_path));
            }
            Ok(())
        })
        .await
        .map_err(|e| io::Error::other(e.to_string()))??;

        let found = scan::stat(Path::new(&dest_root_path), &to)?;
        let mut tx = crate::db::begin_write(pool).await?;
        // Sidecar rows are rebuilt by the next scan; drop the stale ones now, before
        // the file's root changes under them.
        sqlx::query("DELETE FROM sidecar WHERE file_id = ?")
            .bind(m.file_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "UPDATE media_file SET root_id = ?, relative_path = ?, device_id = ?, inode = ?, mtime = ?,
                 last_seen = unixepoch() WHERE id = ?",
        )
        .bind(dest_root_id)
        .bind(&m.to)
        .bind(found.device_id)
        .bind(found.inode)
        .bind(found.mtime)
        .bind(m.file_id)
        .execute(&mut *tx)
        .await?;
        events::emit(
            &mut tx,
            events::FILE_MOVED,
            json!({ "file_id": m.file_id, "root_id": dest_root_id, "path": m.to, "from": m.from }),
        )
        .await?;
        tx.commit().await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transfers_never_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.mkv");
        let b = dir.path().join("sub/b.mkv");
        std::fs::write(&a, "video").unwrap();
        transfer(&a, &b, TransferMode::Hardlink).unwrap();
        assert!(a.exists() && b.exists());
        let err = transfer(&a, &b, TransferMode::Copy).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
        let c = dir.path().join("c.mkv");
        transfer(&a, &c, TransferMode::Move).unwrap();
        assert!(!a.exists() && c.exists());
    }

    #[test]
    fn sidecars_follow_their_video_name() {
        let d = sidecar_destination(
            Path::new("/in/x.en.srt"),
            Path::new("/in/x.mkv"),
            Path::new("/lib/Show - S01E01.mkv"),
        );
        assert_eq!(d.unwrap(), Path::new("/lib/Show - S01E01.en.srt"));
        let dir = tempfile::tempdir().unwrap();
        for f in ["x.mkv", "x.en.srt", "x-thumb.jpg", "xy.srt", "other.srt"] {
            std::fs::write(dir.path().join(f), "").unwrap();
        }
        let mut found: Vec<String> = sidecars_of(&dir.path().join("x.mkv"))
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        found.sort();
        assert_eq!(found, ["x-thumb.jpg", "x.en.srt"]);
    }
}
