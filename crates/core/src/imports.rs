//! Finding files to import: what's in a folder outside the library, which of it
//! looks finished, and which series episode or movie each video probably is.
//! Importing one is `organize::import_file`. Design: `docs/library-management.md`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::Serialize;
use sqlx::SqlitePool;
use utoipa::ToSchema;

use crate::matcher::episodes::{self, SeriesIndex};
use crate::matcher::score::similarity;
use crate::parse::{Parsed, parse};
use crate::scan;

#[derive(Debug, thiserror::Error)]
pub enum ImportsError {
    #[error("not found")]
    NotFound,
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

/// How long a file must stay unchanged before it counts as finished, by default.
pub const SETTLE_SECONDS: i64 = 120;

/// Suffixes download tools give a file while they write it.
const UNFINISHED_SUFFIXES: &[&str] = &[
    ".part",
    ".partial",
    ".!qb",
    ".!ut",
    ".!bt",
    ".crdownload",
    ".tmp",
];

/// Folders download tools unpack or park failures in (SABnzbd, NZBGet).
fn is_work_folder(name: &str) -> bool {
    name.starts_with("_UNPACK_") || name.starts_with("_FAILED_")
}

/// Whether a name marks a file still being written, whatever it ends up as.
pub fn has_unfinished_marker(name: &str) -> bool {
    let lower = name.to_lowercase();
    name.starts_with('.') || UNFINISHED_SUFFIXES.iter().any(|s| lower.ends_with(s))
}

/// Why a video doesn't look finished yet, if it doesn't: a download tool's
/// control file next to it, or it changed within `settle` seconds of `now`.
pub fn not_finished(path: &Path, mtime: i64, now: i64, settle: i64) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    if path.with_file_name(format!("{name}.aria2")).exists() {
        return Some("aria2 is still downloading it".into());
    }
    let age = now - mtime;
    if age < settle {
        return Some(format!(
            "It changed {} ago; waiting until it has been still for {}",
            seconds(age.max(0)),
            seconds(settle)
        ));
    }
    None
}

fn seconds(s: i64) -> String {
    if s < 90 {
        format!("{s} s")
    } else {
        format!("{} min", (s + 30) / 60)
    }
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct GuessedEpisode {
    pub id: i64,
    /// `S01E02`, or `E12` without a season.
    pub label: String,
    pub title: String,
}

/// Which series episode(s) or movie a file probably is.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ImportGuess {
    /// The series or movie.
    pub item_id: i64,
    /// `series` or `movie`.
    pub kind: String,
    pub title: String,
    pub year: Option<i64>,
    /// For a series: the episodes, if the name says which; empty otherwise.
    pub episodes: Vec<GuessedEpisode>,
    /// 0–1.
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ImportCandidate {
    /// Absolute.
    pub path: String,
    /// Relative to the folder looked at.
    pub relative_path: String,
    pub size: i64,
    pub mtime: i64,
    /// Why it doesn't look finished yet.
    pub waiting: Option<String>,
    /// On the same filesystem as a writable folder of the library, so a hard
    /// link can work.
    pub same_disk: bool,
    pub guess: Option<ImportGuess>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ImportListing {
    pub path: String,
    pub files: Vec<ImportCandidate>,
    /// Files skipped because their name says they're still being written.
    pub unfinished: usize,
    /// More videos than are listed.
    pub truncated: bool,
}

/// At most this many videos are listed.
const MAX_FILES: usize = 1000;
/// Nor deeper than this.
const MAX_DEPTH: usize = 6;

struct LibraryItem {
    id: i64,
    kind: String,
    title: String,
    year: Option<i64>,
}

/// The videos under `folder` that could be imported into `library_id`, each
/// with a guess at what it is.
pub async fn list(
    pool: &SqlitePool,
    library_id: i64,
    folder: &str,
    settle: i64,
) -> Result<ImportListing, ImportsError> {
    let kind: String = sqlx::query_scalar("SELECT kind FROM library WHERE id = ?")
        .bind(library_id)
        .fetch_optional(pool)
        .await?
        .ok_or(ImportsError::NotFound)?;
    if kind == "folders" {
        return Err(ImportsError::Invalid(
            "A folder library doesn't import: add the folder to it instead.".into(),
        ));
    }
    let raw = Path::new(folder.trim());
    if !raw.is_absolute() {
        return Err(ImportsError::Invalid(
            "Use a full path, starting with /.".into(),
        ));
    }
    let dir = scan::canonical(raw).await.map_err(|_| {
        ImportsError::Invalid(
            "That folder doesn't exist or can't be reached from the server.".into(),
        )
    })?;
    let roots: Vec<(i64, String, String, i64)> =
        sqlx::query_as("SELECT id, path, write_policy, library_id FROM library_root")
            .fetch_all(pool)
            .await?;
    if let Some((_, root, _, _)) = roots.iter().find(|(_, root, _, _)| dir.starts_with(root)) {
        return Err(ImportsError::Invalid(format!(
            "That folder is inside {root}, which is already in a library. Use Organise to change files there."
        )));
    }
    let skip: Vec<PathBuf> = roots.iter().map(|r| PathBuf::from(&r.1)).collect();
    let devices: HashSet<u64> = roots
        .iter()
        .filter(|(_, _, policy, library)| *library == library_id && policy != "read_only")
        .filter_map(|(_, path, _, _)| device(Path::new(path)))
        .collect();

    let walk_dir = dir.clone();
    let (found, unfinished, truncated) =
        tokio::task::spawn_blocking(move || walk(&walk_dir, &skip))
            .await
            .map_err(|e| ImportsError::Invalid(e.to_string()))?;

    let wanted_kind = if kind == "movies" {
        "movie"
    } else {
        "series"
    };
    let items: Vec<LibraryItem> = sqlx::query_as::<_, (i64, String, String, Option<i64>)>(
        "SELECT id, kind, title, year FROM item WHERE library_id = ? AND kind = ?",
    )
    .bind(library_id)
    .bind(wanted_kind)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|(id, kind, title, year)| LibraryItem {
        id,
        kind,
        title,
        year,
    })
    .collect();

    let now = chrono::Utc::now().timestamp();
    let mut indexes: HashMap<i64, SeriesIndex> = HashMap::new();
    let mut files = Vec::new();
    for (path, size, mtime) in found {
        let relative_path = scan::relative_path(&dir, &path).unwrap_or_default();
        let guess = guess(pool, &items, &mut indexes, &relative_path).await?;
        files.push(ImportCandidate {
            waiting: not_finished(&path, mtime, now, settle),
            same_disk: device(&path).is_some_and(|d| devices.contains(&d)),
            path: path.to_string_lossy().into_owned(),
            relative_path,
            size,
            mtime,
            guess,
        });
    }
    Ok(ImportListing {
        path: dir.to_string_lossy().into_owned(),
        files,
        unfinished,
        truncated,
    })
}

#[cfg(unix)]
fn device(path: &Path) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path).ok().map(|m| m.dev())
}

/// The drive or share, which is what a hard link can't cross. (A volume
/// mounted in a folder is missed; linking then fails and falls back to a copy.)
#[cfg(windows)]
fn device(path: &Path) -> Option<u64> {
    use std::hash::{DefaultHasher, Hash, Hasher};
    use std::path::{Component, Prefix};
    let Some(Component::Prefix(prefix)) = path.components().next() else {
        return None;
    };
    let key = match prefix.kind() {
        Prefix::Disk(d) | Prefix::VerbatimDisk(d) => d.to_ascii_uppercase().to_string(),
        Prefix::UNC(server, share) | Prefix::VerbatimUNC(server, share) => format!(
            "{}\\{}",
            server.to_string_lossy().to_lowercase(),
            share.to_string_lossy().to_lowercase()
        ),
        _ => return None,
    };
    let mut h = DefaultHasher::new();
    key.hash(&mut h);
    Some(h.finish())
}

#[cfg(not(any(unix, windows)))]
fn device(_: &Path) -> Option<u64> {
    None
}

/// Videos under `dir` as (path, size, mtime), sorted; how many were skipped as
/// unfinished; and whether there were more than `MAX_FILES`.
fn walk(dir: &Path, skip: &[PathBuf]) -> (Vec<(PathBuf, i64, i64)>, usize, bool) {
    let mut found = Vec::new();
    let mut unfinished = 0;
    let mut stack = vec![(dir.to_path_buf(), 0)];
    while let Some((folder, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            let Ok(meta) = std::fs::metadata(&path) else {
                continue;
            };
            if meta.is_dir() {
                if name.starts_with('.') || is_work_folder(&name) || skip.contains(&path) {
                    continue;
                }
                if depth < MAX_DEPTH {
                    stack.push((path, depth + 1));
                }
                continue;
            }
            if has_unfinished_marker(&name) {
                // `.part` and the like: a video in the making, or a stray.
                unfinished += usize::from(!name.starts_with('.'));
                continue;
            }
            if !scan::is_video(&path) || parse(&name).extra.is_some() {
                continue;
            }
            let mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_secs() as i64);
            found.push((path, meta.len() as i64, mtime));
        }
    }
    found.sort();
    let truncated = found.len() > MAX_FILES;
    found.truncate(MAX_FILES);
    (found, unfinished, truncated)
}

/// The name to judge a file by: its own, or its folder's when its own has no
/// title (`Show S01/01.mkv`, `Movie (2020)/movie.mkv`).
fn parsed_with_folder(relative_path: &str) -> (Parsed, Option<Parsed>) {
    let mut parts = relative_path.rsplit('/');
    let name = parts.next().unwrap_or(relative_path);
    let folder = parts.next().map(parse);
    (parse(name), folder)
}

async fn guess(
    pool: &SqlitePool,
    items: &[LibraryItem],
    indexes: &mut HashMap<i64, SeriesIndex>,
    relative_path: &str,
) -> Result<Option<ImportGuess>, sqlx::Error> {
    let (file, folder) = parsed_with_folder(relative_path);
    let titles = [
        file.title.as_deref(),
        folder.as_ref().and_then(|f| f.title.as_deref()),
    ];
    let year = file.year.or(folder.as_ref().and_then(|f| f.year));
    let mut best: Option<(&LibraryItem, f64)> = None;
    for title in titles.into_iter().flatten() {
        for item in items {
            let mut score = similarity(title, &item.title);
            if let (Some(a), Some(b)) = (year, item.year) {
                score += if a == b {
                    0.05
                } else {
                    -0.15
                };
            }
            if best.is_none_or(|(_, s)| score > s) {
                best = Some((item, score));
            }
        }
    }
    let Some((item, score)) = best.filter(|(_, s)| *s >= 0.8) else {
        return Ok(None);
    };
    let mut guess = ImportGuess {
        item_id: item.id,
        kind: item.kind.clone(),
        title: item.title.clone(),
        year: item.year,
        episodes: Vec::new(),
        confidence: score.min(1.0),
    };
    if item.kind == "series" {
        if let std::collections::hash_map::Entry::Vacant(e) = indexes.entry(item.id) {
            e.insert(crate::matcher::load_index(pool, item.id).await?);
        }
        let folder_season = folder.as_ref().and_then(|f| f.season);
        if let Some(found) = episodes::resolve(&indexes[&item.id], &file, folder_season, None) {
            for id in found.items {
                guess.episodes.push(episode(pool, id).await?);
            }
            guess.confidence = guess.confidence.min(found.confidence);
        }
    }
    Ok(Some(guess))
}

/// An episode with its label in the series' default ordering.
pub async fn episode(pool: &SqlitePool, id: i64) -> Result<GuessedEpisode, sqlx::Error> {
    let (title, season, number): (String, Option<i64>, Option<i64>) = sqlx::query_as(
        "SELECT e.title, os.number, oe.episode_number FROM item e
         LEFT JOIN ordering_entry oe ON oe.item_id = e.id
             AND oe.ordering_id = (SELECT id FROM ordering WHERE series_id = e.parent_id AND is_default)
         LEFT JOIN ordering_season os ON os.id = oe.ordering_season_id
         WHERE e.id = ?",
    )
    .bind(id)
    .fetch_one(pool)
    .await?;
    let label = match (season, number) {
        (Some(s), Some(n)) => format!("S{s:02}E{n:02}"),
        (None, Some(n)) => format!("E{n:02}"),
        _ => String::new(),
    };
    Ok(GuessedEpisode { id, label, title })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markers_and_settling() {
        assert!(has_unfinished_marker("Show.S01E01.mkv.part"));
        assert!(has_unfinished_marker("Show.S01E01.mkv.!qB"));
        assert!(has_unfinished_marker("movie.mp4.crdownload"));
        assert!(has_unfinished_marker(".Show.S01E01.mkv.Xa81b2"), "rsync");
        assert!(!has_unfinished_marker("Show.S01E01.mkv"));
        assert!(is_work_folder("_UNPACK_Show.S01E01"));

        let dir = tempfile::tempdir().unwrap();
        let video = dir.path().join("a.mkv");
        std::fs::write(&video, "x").unwrap();
        assert!(not_finished(&video, 1000, 1000 + SETTLE_SECONDS, SETTLE_SECONDS).is_none());
        let waiting = not_finished(&video, 1000, 1030, SETTLE_SECONDS).unwrap();
        assert!(waiting.contains("30 s ago"), "{waiting}");
        std::fs::write(dir.path().join("a.mkv.aria2"), "").unwrap();
        assert!(not_finished(&video, 0, 1_000_000, SETTLE_SECONDS).is_some());
    }

    #[test]
    fn walks_videos_and_skips_what_is_unfinished() {
        let dir = tempfile::tempdir().unwrap();
        for f in [
            "Show.S01E01.mkv",
            "Show.S01E02.mkv.part",
            "_UNPACK_Show.S01E03/Show.S01E03.mkv",
            "Show S01/Show.S01E04.mkv",
            "Show S01/Show.S01E04.en.srt",
        ] {
            let p = dir.path().join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, "x").unwrap();
        }
        let (found, unfinished, truncated) = walk(dir.path(), &[]);
        let names: Vec<String> = found
            .iter()
            .map(|(p, _, _)| scan::relative_path(dir.path(), p).unwrap())
            .collect();
        assert_eq!(names, ["Show S01/Show.S01E04.mkv", "Show.S01E01.mkv"]);
        assert_eq!((unfinished, truncated), (1, false));
    }
}
