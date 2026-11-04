//! Browsing a folder library. The tree isn't stored: it's read from the paths of
//! the library's files, so it's always what's on disk, and folders with the same
//! path on different roots show as one.

use std::collections::{BTreeMap, HashMap};

use serde::Serialize;
use serde_json::json;
use sqlx::SqlitePool;
use utoipa::ToSchema;

use super::CatalogError;
use super::movies::{MOVIE_SELECT, MovieRow, MovieSummary, summary};

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct FolderEntry {
    pub name: String,
    /// Relative to the library's roots; pass it to browse this folder.
    pub path: String,
    /// Videos present in it and everything below it.
    pub videos: i64,
    /// Of those, the ones the viewer has watched.
    pub watched: i64,
    /// Videos in it and below it whose file has gone missing.
    pub missing: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct FolderVideo {
    #[serde(flatten)]
    pub summary: MovieSummary,
    pub file_name: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct FolderPage {
    pub library_id: i64,
    /// This folder, relative to the library's roots. Empty at the top.
    pub path: String,
    pub folders: Vec<FolderEntry>,
    pub videos: Vec<FolderVideo>,
}

/// One folder of a folder library. Not found if `library` isn't a folder library,
/// or holds nothing at `path`; the top of a library always exists.
pub async fn browse(
    pool: &SqlitePool,
    library: i64,
    path: &str,
    user: Option<i64>,
) -> Result<FolderPage, CatalogError> {
    let is_folder_library: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM library WHERE id = ? AND kind = 'folders')",
    )
    .bind(library)
    .fetch_one(pool)
    .await?;
    if !is_folder_library {
        return Err(CatalogError::NotFound);
    }
    let path = path.trim_matches('/');
    let prefix = if path.is_empty() {
        String::new()
    } else {
        format!("{path}/")
    };

    let files: Vec<(String, String, i64, bool)> = sqlx::query_as(
        "SELECT f.relative_path, f.status, l.item_id, coalesce(s.played, 0)
         FROM media_file f
         JOIN file_link l ON l.file_id = f.id
         LEFT JOIN user_item_state s ON s.item_id = l.item_id AND s.user_id = ?1
         WHERE f.library_id = ?2 AND f.status IN ('present', 'missing')
           AND substr(f.relative_path, 1, length(?3)) = ?3",
    )
    .bind(user)
    .bind(library)
    .bind(&prefix)
    .fetch_all(pool)
    .await?;
    if files.is_empty() && !path.is_empty() {
        return Err(CatalogError::NotFound);
    }

    let mut folders: BTreeMap<String, FolderEntry> = BTreeMap::new();
    let mut names: HashMap<i64, String> = HashMap::new();
    for (relative_path, status, item, played) in files {
        let below = &relative_path[prefix.len()..];
        let Some((name, _)) = below.split_once('/') else {
            names.insert(item, below.to_string());
            continue;
        };
        let folder = folders
            .entry(name.to_string())
            .or_insert_with(|| FolderEntry {
                name: name.to_string(),
                path: format!("{prefix}{name}"),
                videos: 0,
                watched: 0,
                missing: 0,
            });
        if status == "present" {
            folder.videos += 1;
            folder.watched += i64::from(played);
        } else {
            folder.missing += 1;
        }
    }

    let mut conn = pool.acquire().await?;
    let items: Vec<i64> = names.keys().copied().collect();
    let rows: Vec<MovieRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{MOVIE_SELECT} AND i.id IN (SELECT value FROM json_each(?2))"
    )))
    .bind(user)
    .bind(json!(items).to_string())
    .fetch_all(&mut *conn)
    .await?;
    let mut videos = Vec::with_capacity(rows.len());
    for row in rows {
        let summary = summary(&mut conn, row).await?;
        let file_name = names.remove(&summary.id).unwrap_or_default();
        videos.push(FolderVideo { summary, file_name });
    }

    Ok(FolderPage {
        library_id: library,
        path: path.to_string(),
        folders: folders.into_values().collect(),
        videos,
    })
}
