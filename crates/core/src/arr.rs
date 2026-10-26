//! Compatibility with today's *arrs, unchanged: lists they poll to know what
//! to fetch, and their Connect webhooks telling us files changed.

use std::collections::BTreeSet;

use serde::Serialize;
use serde_json::Value;
use sqlx::SqlitePool;
use utoipa::ToSchema;

/// One series in Sonarr's "Custom List" format.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SonarrListItem {
    pub title: String,
    pub tvdb_id: Option<i64>,
    pub tmdb_id: Option<i64>,
    pub imdb_id: Option<String>,
}

/// One movie in a format both Radarr list types read: "Radarr List" uses `id`
/// (TMDB), "StevenLu Custom" uses `title` and `imdb_id`.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RadarrListItem {
    pub id: Option<i64>,
    pub title: String,
    pub imdb_id: Option<String>,
}

async fn ids(pool: &SqlitePool, item: i64) -> Result<Vec<(String, String)>, sqlx::Error> {
    sqlx::query_as("SELECT provider, value FROM external_id WHERE item_id = ?")
        .bind(item)
        .fetch_all(pool)
        .await
}

fn id_of(ids: &[(String, String)], provider: &str) -> Option<String> {
    ids.iter()
        .find(|(p, _)| p == provider)
        .map(|(_, v)| v.clone())
}

/// Monitored series that still want something: Sonarr adds them and searches.
pub async fn sonarr_list(pool: &SqlitePool) -> Result<Vec<SonarrListItem>, sqlx::Error> {
    let series: Vec<(i64, String)> = sqlx::query_as(
        "SELECT DISTINCT s.id, s.title FROM item_wanted w JOIN item s ON s.id = w.parent_id
         WHERE w.kind = 'episode' ORDER BY s.title COLLATE NOCASE",
    )
    .fetch_all(pool)
    .await?;
    let mut out = Vec::new();
    for (id, title) in series {
        let ids = ids(pool, id).await?;
        out.push(SonarrListItem {
            title,
            tvdb_id: id_of(&ids, "tvdb").and_then(|v| v.parse().ok()),
            tmdb_id: id_of(&ids, "tmdb").and_then(|v| v.parse().ok()),
            imdb_id: id_of(&ids, "imdb"),
        });
    }
    Ok(out)
}

/// Wanted movies.
pub async fn radarr_list(pool: &SqlitePool) -> Result<Vec<RadarrListItem>, sqlx::Error> {
    let movies: Vec<(i64, String)> = sqlx::query_as(
        "SELECT i.id, i.title FROM item_wanted w JOIN item i ON i.id = w.item_id
         WHERE w.kind = 'movie' ORDER BY i.title COLLATE NOCASE",
    )
    .fetch_all(pool)
    .await?;
    let mut out = Vec::new();
    for (id, title) in movies {
        let ids = ids(pool, id).await?;
        out.push(RadarrListItem {
            id: id_of(&ids, "tmdb").and_then(|v| v.parse().ok()),
            title,
            imdb_id: id_of(&ids, "imdb"),
        });
    }
    Ok(out)
}

/// Absolute paths anywhere in a webhook payload. The *arrs' payloads differ by
/// event and version (series.path, episodeFile.path, movie.folderPath,
/// renamedEpisodeFiles[].path…); collecting every path is robust to all of them.
pub fn paths_in(payload: &Value) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    fn walk(v: &Value, out: &mut BTreeSet<String>) {
        match v {
            Value::String(s) if s.starts_with('/') && s.len() > 1 => {
                out.insert(s.clone());
            }
            Value::Array(items) => items.iter().for_each(|i| walk(i, out)),
            Value::Object(map) => map.values().for_each(|i| walk(i, out)),
            _ => {}
        }
    }
    walk(payload, &mut out);
    out
}

/// Roots that contain any of `paths`.
pub async fn roots_for(
    pool: &SqlitePool,
    paths: &BTreeSet<String>,
) -> Result<Vec<i64>, sqlx::Error> {
    let roots: Vec<(i64, String)> = sqlx::query_as("SELECT id, path FROM library_root")
        .fetch_all(pool)
        .await?;
    let mut hit = BTreeSet::new();
    for (id, root) in roots {
        let root = root.trim_end_matches('/');
        if paths
            .iter()
            .any(|p| p == root || p.starts_with(&format!("{root}/")))
        {
            hit.insert(id);
        }
    }
    Ok(hit.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_paths_in_any_payload_shape() {
        let sonarr = serde_json::json!({
            "eventType": "Download",
            "series": { "title": "Show", "path": "/media/tv/Show" },
            "episodeFile": { "relativePath": "Season 01/e01.mkv", "path": "/media/tv/Show/Season 01/e01.mkv" },
            "renamedEpisodeFiles": [{ "path": "/media/tv/Show/Season 01/old.mkv" }]
        });
        let paths = paths_in(&sonarr);
        assert_eq!(paths.len(), 3);
        assert!(paths.contains("/media/tv/Show"));
        assert!(paths_in(&serde_json::json!({ "eventType": "Test" })).is_empty());
    }
}
