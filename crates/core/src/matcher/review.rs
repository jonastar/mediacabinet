//! The review queue: uncertain matches waiting for a person, and what they decide.

use std::collections::HashMap;

use serde::Serialize;
use serde_json::json;
use sqlx::SqlitePool;
use utoipa::ToSchema;

use super::{MatchSettings, link, match_root, store_location};
use crate::events;
use crate::layout::{self, LibraryKind, Template, TemplateRole};
use crate::metadata::Provider;
use crate::metadata::import::{import_movie, import_series};
use crate::parse::{Parsed, parse};

#[derive(Debug, thiserror::Error)]
pub enum ReviewError {
    #[error("not found")]
    NotFound,
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Provider(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PathPiece {
    pub text: String,
    /// The template level this path component sits at; `file` for the file name.
    pub role: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Candidate {
    pub id: i64,
    /// Folder candidates: the provider and its ID.
    pub provider: Option<String>,
    pub provider_value: Option<String>,
    /// File candidates: the item.
    pub item_id: Option<i64>,
    /// A title to show: the provider's, or the episode's with its number.
    pub title: String,
    pub confidence: f64,
    /// `path_id`, `nfo`, `parser`, `search` or `ai`.
    pub source: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct FolderReview {
    pub root_id: i64,
    pub root_path: String,
    pub folder_path: String,
    /// `tv` or `movies`.
    pub kind: String,
    pub path_pieces: Vec<PathPiece>,
    pub hints: layout::Hints,
    pub files: i64,
    pub candidates: Vec<Candidate>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct FileReview {
    pub file_id: i64,
    pub root_id: i64,
    pub root_path: String,
    /// `tv` or `movies`.
    pub kind: String,
    /// The series or movie folder the template puts it in; for a loose movie, the
    /// file itself. None for a TV file outside any series folder.
    pub item_folder: Option<String>,
    /// What the folder name says, for searching.
    pub folder_hints: Option<layout::Hints>,
    pub relative_path: String,
    pub path_pieces: Vec<PathPiece>,
    pub parsed: Parsed,
    pub duration_seconds: Option<f64>,
    /// The series or movie its folder belongs to, if known.
    pub parent_id: Option<i64>,
    pub parent_title: Option<String>,
    pub candidates: Vec<Candidate>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ReviewQueue {
    pub folders: Vec<FolderReview>,
    /// Files with pending candidates.
    pub files: Vec<FileReview>,
    /// Files with nothing to propose: no numbers that fit, or no identified folder.
    pub unmatched: Vec<FileReview>,
}

fn pieces(template: Option<&Template>, path: &str) -> Vec<PathPiece> {
    let parts: Vec<&str> = path.split('/').collect();
    parts
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let role = if i == parts.len() - 1 && path.contains('.') {
                "file".to_string()
            } else {
                match template.and_then(|t| {
                    t.pieces()
                        .into_iter()
                        .filter(|x| x.role != TemplateRole::Separator)
                        .nth(i)
                }) {
                    Some(x) => serde_json::to_value(x.role)
                        .ok()
                        .and_then(|v| v.as_str().map(String::from))
                        .unwrap_or_default(),
                    None => "below".to_string(),
                }
            };
            PathPiece {
                text: p.to_string(),
                role,
            }
        })
        .collect()
}

#[derive(sqlx::FromRow)]
struct RootInfo {
    id: i64,
    path: String,
    layout_template: String,
    kind: String,
}

async fn roots(
    pool: &SqlitePool,
) -> Result<HashMap<i64, (RootInfo, Option<Template>)>, sqlx::Error> {
    let rows: Vec<RootInfo> = sqlx::query_as(
        "SELECT r.id, r.path, r.layout_template, l.kind FROM library_root r JOIN library l ON l.id = r.library_id",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            let t = Template::parse(
                &r.layout_template,
                LibraryKind::parse(&r.kind).unwrap_or(LibraryKind::Tv),
            )
            .ok();
            (r.id, (r, t))
        })
        .collect())
}

fn number_label(season: Option<i64>, number: Option<i64>) -> String {
    match (season, number) {
        (Some(s), Some(e)) => format!("S{s:02}E{e:02}"),
        (None, Some(e)) => format!("{e:02}"),
        _ => String::new(),
    }
}

#[derive(sqlx::FromRow)]
struct FolderCandidateRow {
    id: i64,
    root_id: i64,
    folder_path: String,
    provider: String,
    provider_value: String,
    provider_title: Option<String>,
    confidence: f64,
    source: String,
    reason: String,
}

#[derive(sqlx::FromRow)]
struct FileCandidateRow {
    id: i64,
    file: i64,
    item: i64,
    title: String,
    confidence: f64,
    source: String,
    reason: String,
    season: Option<i64>,
    number: Option<i64>,
    parent: i64,
}

#[derive(sqlx::FromRow)]
struct CandidateRow {
    file_id: Option<i64>,
    item_id: Option<i64>,
    root_id: Option<i64>,
    folder_path: Option<String>,
    provider_value: Option<String>,
    state: String,
}

pub async fn queue(pool: &SqlitePool) -> Result<ReviewQueue, ReviewError> {
    let roots = roots(pool).await?;

    // Folders.
    let rows: Vec<FolderCandidateRow> = sqlx::query_as(
        "SELECT id, root_id, folder_path, provider, provider_value, provider_title, confidence, source, reason
         FROM match_candidate WHERE state = 'pending' AND folder_path IS NOT NULL
         ORDER BY root_id, folder_path, confidence DESC, id",
    )
    .fetch_all(pool)
    .await?;
    let mut folders: Vec<FolderReview> = Vec::new();
    for FolderCandidateRow {
        id,
        root_id,
        folder_path,
        provider,
        provider_value: value,
        provider_title: title,
        confidence,
        source,
        reason,
    } in rows
    {
        let Some((root, template)) = roots.get(&root_id) else {
            continue;
        };
        if folders
            .last()
            .is_none_or(|f| f.root_id != root_id || f.folder_path != folder_path)
        {
            let files: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM media_file WHERE root_id = ? AND status = 'present'
                   AND (relative_path LIKE ?2 || '/%' OR relative_path = ?2)",
            )
            .bind(root_id)
            .bind(&folder_path)
            .fetch_one(pool)
            .await?;
            let hints = template
                .as_ref()
                .and_then(|t| layout::group(t, &format!("{folder_path}/x.mkv")))
                .map(|g| g.hints)
                .unwrap_or_default();
            folders.push(FolderReview {
                root_id,
                root_path: root.path.clone(),
                path_pieces: pieces(template.as_ref(), &folder_path),
                folder_path: folder_path.clone(),
                kind: root.kind.clone(),
                hints,
                files,
                candidates: Vec::new(),
            });
        }
        folders.last_mut().unwrap().candidates.push(Candidate {
            id,
            provider: Some(provider),
            provider_value: Some(value),
            item_id: None,
            title: title.unwrap_or_default(),
            confidence,
            source,
            reason,
        });
    }

    // Files: pending candidates, and present unlinked files with none.
    let file_rows: Vec<(i64, i64, String, Option<f64>, bool)> = sqlx::query_as(
        "SELECT f.id, f.root_id, f.relative_path, f.duration_seconds,
                EXISTS (SELECT 1 FROM match_candidate c WHERE c.file_id = f.id AND c.state = 'pending')
         FROM media_file f
         WHERE f.status = 'present' AND NOT EXISTS (SELECT 1 FROM file_link WHERE file_id = f.id)
         ORDER BY f.root_id, f.relative_path",
    )
    .fetch_all(pool)
    .await?;
    let candidate_rows: Vec<FileCandidateRow> = sqlx::query_as(
        "SELECT c.id, c.file_id AS file, c.item_id AS item, i.title, c.confidence, c.source, c.reason,
                os.number AS season, oe.episode_number AS number, coalesce(i.parent_id, i.id) AS parent
         FROM match_candidate c
         JOIN item i ON i.id = c.item_id
         LEFT JOIN ordering o ON o.series_id = i.parent_id AND o.is_default
         LEFT JOIN ordering_entry oe ON oe.ordering_id = o.id AND oe.item_id = i.id
         LEFT JOIN ordering_season os ON os.id = oe.ordering_season_id
         WHERE c.state = 'pending' AND c.file_id IS NOT NULL
         GROUP BY c.id ORDER BY c.confidence DESC, c.id",
    )
    .fetch_all(pool)
    .await?;
    let mut candidates: HashMap<i64, Vec<Candidate>> = HashMap::new();
    let mut parent_of: HashMap<i64, i64> = HashMap::new();
    for FileCandidateRow {
        id,
        file,
        item,
        title,
        confidence,
        source,
        reason,
        season,
        number,
        parent,
    } in candidate_rows
    {
        parent_of.insert(file, parent);
        let label = number_label(season, number);
        candidates.entry(file).or_default().push(Candidate {
            id,
            provider: None,
            provider_value: None,
            item_id: Some(item),
            title: if label.is_empty() {
                title
            } else {
                format!("{label} · {title}")
            },
            confidence,
            source,
            reason,
        });
    }
    let locations: Vec<(i64, String, i64, String)> = sqlx::query_as(
        "SELECT sl.root_id, sl.path, sl.series_item_id, i.title FROM series_location sl JOIN item i ON i.id = sl.series_item_id",
    )
    .fetch_all(pool)
    .await?;
    let titles: HashMap<i64, String> = locations
        .iter()
        .map(|(_, _, id, t)| (*id, t.clone()))
        .collect();

    let mut files = Vec::new();
    let mut unmatched = Vec::new();
    for (file_id, root_id, relative_path, duration, pending) in file_rows {
        let Some((root, template)) = roots.get(&root_id) else {
            continue;
        };
        let name = relative_path.rsplit('/').next().unwrap_or(&relative_path);
        let parent_id = parent_of.get(&file_id).copied().or_else(|| {
            locations
                .iter()
                .find(|(r, path, _, _)| {
                    *r == root_id && relative_path.starts_with(&format!("{path}/"))
                })
                .map(|(_, _, id, _)| *id)
        });
        // Files whose folder is itself waiting for review show under the folder.
        let in_queued_folder = folders.iter().any(|f| {
            f.root_id == root_id
                && (relative_path.starts_with(&format!("{}/", f.folder_path))
                    || relative_path == f.folder_path)
        });
        if !pending && in_queued_folder {
            continue;
        }
        let group = template
            .as_ref()
            .and_then(|t| layout::group(t, &relative_path));
        let (item_folder, folder_hints) = match group {
            Some(g) => (Some(g.item_folder), Some(g.hints)),
            None if root.kind == "movies" => (Some(relative_path.clone()), None),
            None => (None, None),
        };
        let review = FileReview {
            file_id,
            root_id,
            root_path: root.path.clone(),
            kind: root.kind.clone(),
            item_folder,
            folder_hints,
            path_pieces: pieces(template.as_ref(), &relative_path),
            relative_path: relative_path.clone(),
            parsed: parse(name),
            duration_seconds: duration,
            parent_title: parent_id.and_then(|p| titles.get(&p).cloned()),
            parent_id,
            candidates: candidates.remove(&file_id).unwrap_or_default(),
        };
        if pending {
            files.push(review);
        } else {
            unmatched.push(review);
        }
    }
    Ok(ReviewQueue {
        folders,
        files,
        unmatched,
    })
}

async fn decide(pool: &SqlitePool, accepted: i64) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE match_candidate SET state = CASE WHEN id = ?1 THEN 'accepted' ELSE 'rejected' END,
             decided_at = unixepoch()
         WHERE state = 'pending' AND (
             (file_id IS NOT NULL AND file_id = (SELECT file_id FROM match_candidate WHERE id = ?1))
             OR (folder_path IS NOT NULL AND root_id = (SELECT root_id FROM match_candidate WHERE id = ?1)
                 AND folder_path = (SELECT folder_path FROM match_candidate WHERE id = ?1)))",
    )
    .bind(accepted)
    .execute(pool)
    .await?;
    Ok(())
}

/// Accepts a candidate. A file candidate becomes a user link (pinned if asked);
/// a folder candidate imports the item, stores the folder and matches its files.
pub async fn accept(
    pool: &SqlitePool,
    candidate: i64,
    pin: bool,
    provider: &Provider,
    settings: MatchSettings,
) -> Result<(), ReviewError> {
    let row: Option<CandidateRow> = sqlx::query_as(
        "SELECT file_id, item_id, root_id, folder_path, provider_value, state FROM match_candidate WHERE id = ?",
    )
    .bind(candidate)
    .fetch_optional(pool)
    .await?;
    let Some(CandidateRow {
        file_id: file,
        item_id: item,
        root_id: root,
        folder_path: folder,
        provider_value: value,
        state,
    }) = row
    else {
        return Err(ReviewError::NotFound);
    };
    if state != "pending" {
        return Err(ReviewError::Invalid(
            "That candidate was already decided.".into(),
        ));
    }
    match (file, item, root, folder, value) {
        (Some(file), Some(item), ..) => {
            link_file(pool, file, &[item], pin).await?;
            decide(pool, candidate).await?;
        }
        (_, _, Some(root), Some(folder), Some(value)) => {
            identify_as(pool, root, &folder, &value, provider, settings).await?;
            decide(pool, candidate).await?;
        }
        _ => return Err(ReviewError::Invalid("Malformed candidate.".into())),
    }
    Ok(())
}

pub async fn reject(pool: &SqlitePool, candidate: i64) -> Result<(), ReviewError> {
    let done = sqlx::query("UPDATE match_candidate SET state = 'rejected', decided_at = unixepoch() WHERE id = ? AND state = 'pending'")
        .bind(candidate)
        .execute(pool)
        .await?;
    if done.rows_affected() == 0 {
        return Err(ReviewError::NotFound);
    }
    Ok(())
}

/// Links a file to items chosen by a person. User links are never changed by scans.
pub async fn link_file(
    pool: &SqlitePool,
    file: i64,
    items: &[i64],
    pin: bool,
) -> Result<(), ReviewError> {
    if items.is_empty() {
        return Err(ReviewError::Invalid("Choose at least one item.".into()));
    }
    let edition =
        sqlx::query_scalar::<_, String>("SELECT relative_path FROM media_file WHERE id = ?")
            .bind(file)
            .fetch_optional(pool)
            .await?
            .ok_or(ReviewError::NotFound)?;
    let parsed = parse(edition.rsplit('/').next().unwrap_or(&edition));
    link(
        pool,
        file,
        items,
        1.0,
        "user",
        parsed.edition.as_deref(),
        parsed.part,
    )
    .await?;
    sqlx::query("UPDATE file_link SET pinned = ? WHERE file_id = ? AND source = 'user'")
        .bind(pin)
        .bind(file)
        .execute(pool)
        .await?;
    sqlx::query("UPDATE match_candidate SET state = 'rejected', decided_at = unixepoch() WHERE file_id = ? AND state = 'pending'")
        .bind(file)
        .execute(pool)
        .await?;
    Ok(())
}

/// Marks a file as not media to match (samples, junk). Its candidates are closed.
pub async fn ignore_file(pool: &SqlitePool, file: i64) -> Result<(), ReviewError> {
    let mut tx = crate::db::begin_write(pool).await?;
    let done =
        sqlx::query("UPDATE media_file SET status = 'ignored' WHERE id = ? AND status = 'present'")
            .bind(file)
            .execute(&mut *tx)
            .await?;
    if done.rows_affected() == 0 {
        return Err(ReviewError::NotFound);
    }
    sqlx::query("UPDATE match_candidate SET state = 'rejected', decided_at = unixepoch() WHERE file_id = ? AND state = 'pending'")
        .bind(file)
        .execute(&mut *tx)
        .await?;
    events::emit(&mut tx, events::FILE_IGNORED, json!({ "file_id": file })).await?;
    tx.commit().await?;
    Ok(())
}

/// Identifies a folder as a provider item chosen by a person: imports it, stores
/// the folder and matches its files.
pub async fn identify_as(
    pool: &SqlitePool,
    root: i64,
    folder: &str,
    provider_id: &str,
    provider: &Provider,
    settings: MatchSettings,
) -> Result<i64, ReviewError> {
    let kind: String = sqlx::query_scalar(
        "SELECT l.kind FROM library_root r JOIN library l ON l.id = r.library_id WHERE r.id = ?",
    )
    .bind(root)
    .fetch_optional(pool)
    .await?
    .ok_or(ReviewError::NotFound)?;
    let item = if kind == "movies" {
        let data = provider
            .movie(provider_id)
            .await
            .map_err(|e| ReviewError::Provider(e.to_string()))?;
        let item = import_movie(pool, &data)
            .await
            .map_err(|e| ReviewError::Provider(e.to_string()))?
            .item_id;
        let files: Vec<i64> = sqlx::query_scalar(
            "SELECT id FROM media_file WHERE root_id = ?1 AND status = 'present'
               AND (relative_path = ?2 OR relative_path LIKE ?2 || '/%')",
        )
        .bind(root)
        .bind(folder)
        .fetch_all(pool)
        .await?;
        for f in files {
            link_file(pool, f, &[item], false).await?;
        }
        item
    } else {
        let data = provider
            .series(provider_id)
            .await
            .map_err(|e| ReviewError::Provider(e.to_string()))?;
        let item = import_series(pool, &data)
            .await
            .map_err(|e| ReviewError::Provider(e.to_string()))?
            .item_id;
        store_location(pool, item, root, folder).await?;
        item
    };
    sqlx::query("UPDATE match_candidate SET state = 'rejected', decided_at = unixepoch() WHERE root_id = ? AND folder_path = ? AND state = 'pending'")
        .bind(root)
        .bind(folder)
        .execute(pool)
        .await?;
    match_root(pool, root, provider, settings)
        .await
        .map_err(|e| ReviewError::Invalid(e.to_string()))?;
    Ok(item)
}
