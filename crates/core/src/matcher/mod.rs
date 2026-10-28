//! Matching: identify each item folder, then link its files to items.
//!
//! Deterministic first; anything uncertain goes to the review queue, never
//! silently in. For each folder the template finds:
//!
//! 1. Known already (a stored series location, or a movie its files link to)?
//! 2. Otherwise identify it: provider IDs in the path, then an `.nfo`, then a
//!    provider search scored by title and year. Confident: import it and store
//!    the folder. Not: queue folder → provider candidates for review.
//! 3. Link its files: episodes through the series' orderings (`episodes`), movies
//!    directly with edition and part. Below the threshold, queue file → item
//!    candidates with the reason.
//!
//! Samples, trailers and other extras are marked ignored. Existing links are
//! never touched.

pub mod episodes;
pub mod review;
pub mod score;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::json;
use sqlx::SqlitePool;
use utoipa::ToSchema;

use crate::events;
use crate::layout::{self, LibraryKind, Template};
use crate::metadata::import::{import_movie, import_series};
use crate::metadata::{MetadataError, Provider, SearchResult};
use crate::parse::{Parsed, parse};
use episodes::{EpisodeInfo, OrderingIndex, SeriesIndex, resolve};
use score::{nfo_ids, similarity};

#[derive(Debug, Clone, Copy)]
pub struct MatchSettings {
    /// At or above this, matches are linked; below, they go to review.
    pub accept: f64,
}

impl Default for MatchSettings {
    fn default() -> Self {
        MatchSettings { accept: 0.85 }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, ToSchema)]
pub struct MatchReport {
    /// Folders identified and imported (or already in the library).
    pub identified: usize,
    /// Folders queued for review: no confident identification.
    pub folders_queued: usize,
    pub linked: usize,
    pub files_queued: usize,
    /// Samples, trailers and other extras, marked ignored.
    pub ignored: usize,
    /// Files with nothing to go on: no numbers the series' orderings know.
    pub unmatched: usize,
    /// Files outside any item folder of the template.
    pub unsorted: usize,
    /// Why identification was skipped or partly failed, if it was.
    pub provider_error: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum MatchError {
    #[error("library root {0} does not exist")]
    RootNotFound(i64),
    #[error("{0}")]
    Template(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

#[derive(sqlx::FromRow)]
struct FileRow {
    id: i64,
    relative_path: String,
    duration_seconds: Option<f64>,
}

struct Folder {
    hints: layout::Hints,
    files: Vec<(FileRow, Parsed, Option<i64>)>,
}

/// An identification: which provider item a folder is, and how sure.
struct Identified {
    id: String,
    title: String,
    confidence: f64,
    source: &'static str,
    reason: String,
}

pub async fn match_root(
    pool: &SqlitePool,
    root_id: i64,
    provider: &Provider,
    settings: MatchSettings,
) -> Result<MatchReport, MatchError> {
    match_root_with_progress(pool, root_id, provider, settings, |_, _| ()).await
}

/// `match_root`, calling `progress(done, total)` as folders are matched.
pub async fn match_root_with_progress(
    pool: &SqlitePool,
    root_id: i64,
    provider: &Provider,
    settings: MatchSettings,
    progress: impl Fn(usize, usize),
) -> Result<MatchReport, MatchError> {
    let (root_path, template, kind): (String, String, String) = sqlx::query_as(
        "SELECT r.path, r.layout_template, l.kind FROM library_root r
         JOIN library l ON l.id = r.library_id WHERE r.id = ?",
    )
    .bind(root_id)
    .fetch_optional(pool)
    .await?
    .ok_or(MatchError::RootNotFound(root_id))?;
    let kind = LibraryKind::parse(&kind).unwrap_or(LibraryKind::Tv);
    let template = Template::parse(&template, kind).map_err(|e| MatchError::Template(e.0))?;
    let root_path = PathBuf::from(root_path);
    let mut report = MatchReport::default();

    let files: Vec<FileRow> = sqlx::query_as(
        "SELECT f.id, f.relative_path, f.duration_seconds FROM media_file f
         WHERE f.root_id = ? AND f.status = 'present'
           AND NOT EXISTS (SELECT 1 FROM file_link WHERE file_id = f.id)
         ORDER BY f.relative_path",
    )
    .bind(root_id)
    .fetch_all(pool)
    .await?;

    let mut folders: HashMap<String, Folder> = HashMap::new();
    for file in files {
        let name = file
            .relative_path
            .rsplit('/')
            .next()
            .unwrap_or(&file.relative_path);
        let parsed = parse(name);
        if parsed.extra.is_some() {
            sqlx::query("UPDATE media_file SET status = 'ignored' WHERE id = ?")
                .bind(file.id)
                .execute(pool)
                .await?;
            report.ignored += 1;
            continue;
        }
        match layout::group(&template, &file.relative_path) {
            Some(g) => {
                let season = g.hints.season;
                folders
                    .entry(g.item_folder)
                    .or_insert_with(|| Folder {
                        hints: g.hints,
                        files: Vec::new(),
                    })
                    .files
                    .push((file, parsed, season));
            }
            // A loose movie file is its own folder; a loose TV file has no series.
            None if kind == LibraryKind::Movies => {
                let hints = layout::Hints {
                    title: parsed.title.clone().unwrap_or_default(),
                    year: parsed.year,
                    ids: parsed.ids.clone(),
                    ..Default::default()
                };
                folders.insert(
                    file.relative_path.clone(),
                    Folder {
                        hints,
                        files: vec![(file, parsed, None)],
                    },
                );
            }
            None => report.unsorted += 1,
        }
    }

    let mut paths: Vec<String> = folders.keys().cloned().collect();
    paths.sort();
    let total = paths.len();
    progress(0, total);
    for (done, path) in paths.into_iter().enumerate() {
        progress(done + 1, total);
        let folder = folders.remove(&path).expect("listed folder");
        let item = match known_item(pool, root_id, &path, kind).await? {
            Some(item) => Some((item, 1.0, "path_id")),
            None => {
                identify_folder(
                    pool,
                    root_id,
                    &root_path,
                    &path,
                    &folder,
                    kind,
                    provider,
                    settings,
                    &mut report,
                )
                .await?
            }
        };
        let Some((item, confidence, source)) = item else {
            continue;
        };
        report.identified += 1;
        match kind {
            LibraryKind::Tv => link_episodes(pool, item, folder, settings, &mut report).await?,
            LibraryKind::Movies => {
                link_movie(pool, item, folder, confidence, source, &mut report).await?
            }
        }
    }
    Ok(report)
}

async fn known_item(
    pool: &SqlitePool,
    root: i64,
    path: &str,
    kind: LibraryKind,
) -> Result<Option<i64>, sqlx::Error> {
    match kind {
        LibraryKind::Tv => {
            sqlx::query_scalar(
                "SELECT series_item_id FROM series_location WHERE root_id = ? AND path = ?",
            )
            .bind(root)
            .bind(path)
            .fetch_optional(pool)
            .await
        }
        LibraryKind::Movies => {
            // A movie folder is known once any file in it links to a movie.
            sqlx::query_scalar(
                "SELECT l.item_id FROM file_link l JOIN media_file f ON f.id = l.file_id
                 JOIN item i ON i.id = l.item_id
                 WHERE f.root_id = ?1 AND i.kind = 'movie'
                   AND (f.relative_path = ?2 OR f.relative_path LIKE ?2 || '/%')
                 LIMIT 1",
            )
            .bind(root)
            .bind(path)
            .fetch_optional(pool)
            .await
        }
    }
}

/// Reads `.nfo` IDs for a folder: `tvshow.nfo`/`movie.nfo` in it, and any `.nfo`
/// sidecar of its files.
async fn folder_nfo_ids(
    pool: &SqlitePool,
    root: &Path,
    root_id: i64,
    path: &str,
    kind: LibraryKind,
) -> Vec<(String, String)> {
    let mut candidates: Vec<PathBuf> = match kind {
        LibraryKind::Tv => vec![root.join(path).join("tvshow.nfo")],
        LibraryKind::Movies => vec![root.join(path).join("movie.nfo")],
    };
    if kind == LibraryKind::Movies {
        let sidecars: Vec<String> = sqlx::query_scalar(
            "SELECT relative_path FROM sidecar WHERE root_id = ?1 AND kind = 'nfo'
               AND (relative_path LIKE ?2 || '/%' OR relative_path = ?3)",
        )
        .bind(root_id)
        .bind(path)
        // A loose movie file's own .nfo.
        .bind(
            path.rsplit_once('.')
                .map_or(path, |(stem, _)| stem)
                .to_string()
                + ".nfo",
        )
        .fetch_all(pool)
        .await
        .unwrap_or_default();
        candidates.extend(sidecars.into_iter().map(|s| root.join(s)));
    }
    let mut ids = Vec::new();
    for file in candidates {
        if let Ok(text) = tokio::fs::read_to_string(&file).await {
            ids.extend(nfo_ids(&text));
        }
    }
    ids
}

#[allow(clippy::too_many_arguments)]
async fn identify_folder(
    pool: &SqlitePool,
    root_id: i64,
    root: &Path,
    path: &str,
    folder: &Folder,
    kind: LibraryKind,
    provider: &Provider,
    settings: MatchSettings,
    report: &mut MatchReport,
) -> Result<Option<(i64, f64, &'static str)>, MatchError> {
    // Already proposed: wait for the user rather than asking again.
    let proposed: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM match_candidate WHERE root_id = ? AND folder_path = ? AND state <> 'accepted')",
    )
    .bind(root_id)
    .bind(path)
    .fetch_one(pool)
    .await?;
    if proposed || !provider.is_configured() {
        if !provider.is_configured() {
            report.provider_error = Some(MetadataError::NotConfigured.to_string());
        }
        return Ok(None);
    }

    let found = match identify(pool, root, root_id, path, folder, kind, provider).await {
        Ok(found) => found,
        Err(e) => {
            report.provider_error = Some(e.to_string());
            return Ok(None);
        }
    };
    let Some(best) = found.first() else {
        return Ok(None);
    };
    if best.confidence >= settings.accept {
        let item = match import(pool, provider, root_id, kind, &best.id).await {
            Ok(item) => item,
            Err(e) => {
                report.provider_error = Some(e);
                return Ok(None);
            }
        };
        if kind == LibraryKind::Tv {
            store_location(pool, item, root_id, path).await?;
        }
        return Ok(Some((item, best.confidence, best.source)));
    }

    let mut tx = crate::db::begin_write(pool).await?;
    for c in found.iter().take(3) {
        sqlx::query(
            "INSERT INTO match_candidate (root_id, folder_path, provider, provider_value, provider_title,
                                          confidence, source, reason)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(root_id)
        .bind(path)
        .bind(provider.name())
        .bind(&c.id)
        .bind(&c.title)
        .bind(c.confidence)
        .bind(c.source)
        .bind(&c.reason)
        .execute(&mut *tx)
        .await?;
    }
    events::emit(
        &mut tx,
        events::MATCH_NEEDS_REVIEW,
        json!({ "root_id": root_id, "folder_path": path }),
    )
    .await?;
    tx.commit().await?;
    report.folders_queued += 1;
    Ok(None)
}

/// Candidates for a folder, best first.
async fn identify(
    pool: &SqlitePool,
    root: &Path,
    root_id: i64,
    path: &str,
    folder: &Folder,
    kind: LibraryKind,
    provider: &Provider,
) -> Result<Vec<Identified>, MetadataError> {
    // Explicit IDs: in the folder name or file names, then in an .nfo.
    let mut explicit: Vec<((String, String), &'static str)> = folder
        .hints
        .ids
        .iter()
        .cloned()
        .map(|id| (id, "path_id"))
        .collect();
    for (_, parsed, _) in &folder.files {
        explicit.extend(parsed.ids.iter().cloned().map(|id| (id, "path_id")));
    }
    explicit.extend(
        folder_nfo_ids(pool, root, root_id, path, kind)
            .await
            .into_iter()
            .map(|id| (id, "nfo")),
    );
    let want = match kind {
        LibraryKind::Tv => "series",
        LibraryKind::Movies => "movie",
    };
    for ((p, written), source) in explicit {
        let resolved = if p == provider.name() {
            Some(written.clone())
        } else {
            provider
                .find(&p, &written)
                .await?
                .filter(|(k, _)| k == want)
                .map(|(_, id)| id)
        };
        if let Some(id) = resolved {
            let place = if source == "nfo" {
                "an .nfo file"
            } else {
                "the path"
            };
            return Ok(vec![Identified {
                reason: format!("{p}:{written} in {place}"),
                title: folder.hints.title.clone(),
                id,
                confidence: 1.0,
                source,
            }]);
        }
    }

    // Search by name and year.
    let title = &folder.hints.title;
    if title.is_empty() {
        return Ok(Vec::new());
    }
    let year = folder.hints.year;
    let mut results = match kind {
        LibraryKind::Tv => provider.search_series(title, year).await?,
        LibraryKind::Movies => provider.search_movies(title, year).await?,
    };
    if results.is_empty() && year.is_some() {
        results = match kind {
            LibraryKind::Tv => provider.search_series(title, None).await?,
            LibraryKind::Movies => provider.search_movies(title, None).await?,
        };
    }
    let mut scored: Vec<Identified> = results.iter().map(|r| score(r, title, year)).collect();
    scored.sort_by(|a, b| b.confidence.total_cmp(&a.confidence));
    // Close candidates: nobody should guess between them, so none is confident.
    if scored.len() > 1 && scored[0].confidence - scored[1].confidence < 0.1 {
        let top = scored[0].confidence;
        for c in scored.iter_mut().filter(|c| top - c.confidence < 0.1) {
            c.confidence = c.confidence.min(0.75);
            c.reason.push_str("; another result is nearly as close");
        }
    }
    Ok(scored)
}

fn score(r: &SearchResult, title: &str, year: Option<i64>) -> Identified {
    let sim = similarity(title, &r.title).max(
        r.original_title
            .as_deref()
            .map_or(0.0, |o| similarity(title, o)),
    );
    let (year_factor, year_note) = match (year, r.year) {
        (Some(a), Some(b)) if a == b => (1.0, format!("year {b} matches")),
        (Some(a), Some(b)) if (a - b).abs() == 1 => (0.9, format!("year {b}, folder says {a}")),
        (Some(a), Some(b)) => (0.5, format!("year {b}, folder says {a}")),
        (None, Some(b)) => (0.95, format!("from {b}; no year in the folder name")),
        _ => (0.9, "no year to compare".to_string()),
    };
    let confidence = (sim * year_factor).clamp(0.0, 0.99);
    Identified {
        id: r.id.clone(),
        title: match r.year {
            Some(y) => format!("{} ({y})", r.title),
            None => r.title.clone(),
        },
        confidence,
        source: "parser",
        reason: format!("title {:.0}% similar, {year_note}", sim * 100.0),
    }
}

/// Imports a provider item into the library of `root`.
async fn import(
    pool: &SqlitePool,
    provider: &Provider,
    root: i64,
    kind: LibraryKind,
    id: &str,
) -> Result<i64, String> {
    let library: i64 = sqlx::query_scalar("SELECT library_id FROM library_root WHERE id = ?")
        .bind(root)
        .fetch_one(pool)
        .await
        .map_err(|e| e.to_string())?;
    match kind {
        LibraryKind::Tv => {
            let data = provider.series(id).await.map_err(|e| e.to_string())?;
            import_series(pool, library, &data)
                .await
                .map(|r| r.item_id)
                .map_err(|e| e.to_string())
        }
        LibraryKind::Movies => {
            let data = provider.movie(id).await.map_err(|e| e.to_string())?;
            import_movie(pool, library, &data)
                .await
                .map(|r| r.item_id)
                .map_err(|e| e.to_string())
        }
    }
}

/// Stores a series' folder on a root, once. Primary if it's the series' first.
pub(crate) async fn store_location(
    pool: &SqlitePool,
    series: i64,
    root: i64,
    path: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT OR IGNORE INTO series_location (series_item_id, root_id, path, is_primary)
         VALUES (?1, ?2, ?3, NOT EXISTS (SELECT 1 FROM series_location WHERE series_item_id = ?1))",
    )
    .bind(series)
    .bind(root)
    .bind(path)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn load_index(pool: &SqlitePool, series: i64) -> Result<SeriesIndex, sqlx::Error> {
    let (title, match_ordering): (String, Option<i64>) =
        sqlx::query_as("SELECT title, match_ordering_id FROM item WHERE id = ?")
            .bind(series)
            .fetch_one(pool)
            .await?;
    let orderings: Vec<(i64, String, String, bool)> = sqlx::query_as(
        "SELECT id, name, kind, is_default FROM ordering WHERE series_id = ? ORDER BY id",
    )
    .bind(series)
    .fetch_all(pool)
    .await?;
    let entries: Vec<(i64, i64, Option<i64>, Option<i64>)> = sqlx::query_as(
        "SELECT oe.ordering_id, oe.item_id, os.number, oe.episode_number FROM ordering_entry oe
         JOIN ordering o ON o.id = oe.ordering_id
         LEFT JOIN ordering_season os ON os.id = oe.ordering_season_id
         WHERE o.series_id = ?",
    )
    .bind(series)
    .fetch_all(pool)
    .await?;
    let mut indexes: Vec<OrderingIndex> = orderings
        .into_iter()
        .map(|(id, name, kind, is_default)| OrderingIndex {
            id,
            name,
            kind,
            is_default,
            by_number: HashMap::new(),
        })
        .collect();
    for (ordering, item, season, number) in entries {
        if let (Some(o), Some(n)) = (indexes.iter_mut().find(|o| o.id == ordering), number) {
            o.by_number.entry((season, n)).or_default().push(item);
        }
    }
    let episodes: Vec<(i64, String, Option<i64>, Option<i64>)> =
        sqlx::query_as("SELECT id, title, airs_at, runtime_seconds FROM item WHERE parent_id = ?")
            .bind(series)
            .fetch_all(pool)
            .await?;
    Ok(SeriesIndex {
        series_id: series,
        title,
        match_ordering,
        orderings: indexes,
        episodes: episodes
            .into_iter()
            .map(|(id, title, airs_at, runtime_seconds)| {
                (
                    id,
                    EpisodeInfo {
                        title,
                        airs_at,
                        runtime_seconds,
                    },
                )
            })
            .collect(),
    })
}

async fn link_episodes(
    pool: &SqlitePool,
    series: i64,
    folder: Folder,
    settings: MatchSettings,
    report: &mut MatchReport,
) -> Result<(), sqlx::Error> {
    let index = load_index(pool, series).await?;
    for (file, parsed, season) in folder.files {
        match resolve(&index, &parsed, season, file.duration_seconds) {
            Some(r) if r.confidence >= settings.accept => {
                link(pool, file.id, &r.items, r.confidence, "parser", None, None).await?;
                report.linked += 1;
            }
            Some(r) => {
                queue_file(pool, file.id, &r.items, r.confidence, &r.reason).await?;
                report.files_queued += 1;
            }
            None => report.unmatched += 1,
        }
    }
    Ok(())
}

async fn link_movie(
    pool: &SqlitePool,
    movie: i64,
    folder: Folder,
    confidence: f64,
    source: &str,
    report: &mut MatchReport,
) -> Result<(), sqlx::Error> {
    for (file, parsed, _) in folder.files {
        link(
            pool,
            file.id,
            &[movie],
            confidence,
            source,
            parsed.edition.as_deref(),
            parsed.part,
        )
        .await?;
        report.linked += 1;
    }
    Ok(())
}

/// Links a file to items, with an event per link. Links someone removed by
/// hand are skipped unless a person makes them again (`source` is `user`).
pub(crate) async fn link(
    pool: &SqlitePool,
    file: i64,
    items: &[i64],
    confidence: f64,
    source: &str,
    edition: Option<&str>,
    part: Option<i64>,
) -> Result<(), sqlx::Error> {
    let mut tx = crate::db::begin_write(pool).await?;
    for item in items {
        if source == "user" {
            sqlx::query("DELETE FROM link_rejection WHERE file_id = ? AND item_id = ?")
                .bind(file)
                .bind(item)
                .execute(&mut *tx)
                .await?;
        } else if is_rejected(&mut tx, file, *item).await? {
            continue;
        }
        let inserted = sqlx::query(
            "INSERT OR IGNORE INTO file_link (file_id, item_id, edition, part, confidence, source, pinned)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(file)
        .bind(item)
        .bind(edition)
        .bind(part)
        .bind(confidence)
        .bind(source)
        .bind(source == "user")
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if inserted > 0 {
            events::emit(
                &mut tx,
                events::LINK_CREATED,
                json!({ "file_id": file, "item_id": item, "source": source, "confidence": confidence }),
            )
            .await?;
        }
    }
    tx.commit().await
}

/// Someone removed this link by hand.
async fn is_rejected(
    conn: &mut sqlx::SqliteConnection,
    file: i64,
    item: i64,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM link_rejection WHERE file_id = ? AND item_id = ?)",
    )
    .bind(file)
    .bind(item)
    .fetch_one(conn)
    .await
}

async fn queue_file(
    pool: &SqlitePool,
    file: i64,
    items: &[i64],
    confidence: f64,
    reason: &str,
) -> Result<(), sqlx::Error> {
    let mut tx = crate::db::begin_write(pool).await?;
    let mut added = false;
    let existing: HashSet<i64> = sqlx::query_scalar(
        "SELECT item_id FROM match_candidate WHERE file_id = ? AND item_id IS NOT NULL",
    )
    .bind(file)
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .collect();
    for item in items.iter().filter(|i| !existing.contains(i)) {
        if is_rejected(&mut tx, file, *item).await? {
            continue;
        }
        sqlx::query(
            "INSERT INTO match_candidate (file_id, item_id, confidence, source, reason) VALUES (?, ?, ?, 'parser', ?)",
        )
        .bind(file)
        .bind(item)
        .bind(confidence)
        .bind(reason)
        .execute(&mut *tx)
        .await?;
        added = true;
    }
    if added {
        events::emit(
            &mut tx,
            events::MATCH_NEEDS_REVIEW,
            json!({ "file_id": file }),
        )
        .await?;
    }
    tx.commit().await
}
