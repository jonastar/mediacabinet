//! Ratings from OMDb (omdbapi.com): IMDb, Rotten Tomatoes (the Tomatometer) and
//! Metacritic, looked up by a series' or movie's IMDb ID. Optional, with
//! `CABINET_OMDB_KEY`; TMDB's own score comes with the metadata.
//!
//! OMDb's free keys allow 1,000 requests a day, so ratings refresh weekly in the
//! background, oldest first, and stop for the round when the limit is reached.

use std::time::Duration;

use serde::Deserialize;
use sqlx::SqlitePool;
use tokio::task::JoinHandle;

use super::MetadataError;
use super::model::RatingData;

const BASE: &str = "https://www.omdbapi.com/";
const WEEK: i64 = 7 * 24 * 3600;

/// The rating sources OMDb fills in; anything else in `item_rating` is left alone.
pub const SOURCES: &[&str] = &["imdb", "rotten_tomatoes", "metacritic"];

#[derive(Clone)]
pub struct Omdb {
    http: reqwest::Client,
    key: String,
    base: String,
}

impl Omdb {
    pub fn new(key: impl Into<String>) -> Omdb {
        Omdb::with_base(key, BASE)
    }

    /// Against another base URL, for tests.
    pub fn with_base(key: impl Into<String>, base: impl Into<String>) -> Omdb {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .user_agent(concat!("MediaCabinet/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("HTTP client builds");
        Omdb {
            http,
            key: key.into(),
            base: base.into(),
        }
    }

    /// Ratings for an IMDb ID (`tt…`), by source. Empty when OMDb has none.
    pub async fn ratings(&self, imdb: &str) -> Result<Vec<(String, RatingData)>, MetadataError> {
        let response = self
            .http
            .get(&self.base)
            .query(&[("apikey", self.key.as_str()), ("i", imdb)])
            .send()
            .await
            .map_err(|e| MetadataError::Unreachable(e.without_url().to_string()))?;
        let status = response.status().as_u16();
        if status >= 500 {
            return Err(MetadataError::Unreachable(format!(
                "OMDb answered {status}"
            )));
        }
        // Errors come as {"Response": "False", "Error": "..."}, with 200 or 401.
        let body: RawTitle = response
            .json()
            .await
            .map_err(|e| MetadataError::Invalid(e.without_url().to_string()))?;
        if body.response.as_deref() == Some("False") {
            let error = body.error.unwrap_or_default();
            return Err(if error.contains("limit") {
                MetadataError::Unreachable("OMDb's daily request limit is reached".into())
            } else if error.contains("API key") || status == 401 {
                MetadataError::Auth(format!("OMDb: {error}"))
            } else {
                MetadataError::NotFound
            });
        }
        Ok(ratings_from(body))
    }
}

/// "8.1/10", "94%", "74/100" → (value, scale).
fn score(raw: &str) -> Option<(f64, f64)> {
    let raw = raw.trim();
    if let Some(percent) = raw.strip_suffix('%') {
        return Some((percent.trim().parse().ok()?, 100.0));
    }
    let (value, scale) = raw.split_once('/')?;
    Some((value.trim().parse().ok()?, scale.trim().parse().ok()?))
}

fn ratings_from(t: RawTitle) -> Vec<(String, RatingData)> {
    let votes = t
        .imdb_votes
        .as_deref()
        .and_then(|v| v.replace(',', "").parse().ok());
    let mut out = Vec::new();
    for r in t.ratings {
        let source = match r.source.as_str() {
            "Internet Movie Database" => "imdb",
            "Rotten Tomatoes" => "rotten_tomatoes",
            "Metacritic" => "metacritic",
            _ => continue,
        };
        if let Some((value, scale)) = score(&r.value) {
            out.push((
                source.to_string(),
                RatingData {
                    value,
                    scale,
                    votes: if source == "imdb" {
                        votes
                    } else {
                        None
                    },
                },
            ));
        }
    }
    // Series often only have the top-level IMDb fields.
    if !out.iter().any(|(s, _)| s == "imdb")
        && let Some(value) = t.imdb_rating.as_deref().and_then(|v| v.parse().ok())
    {
        out.push((
            "imdb".into(),
            RatingData {
                value,
                scale: 10.0,
                votes,
            },
        ));
    }
    out
}

#[derive(Deserialize)]
struct RawTitle {
    #[serde(rename = "Response")]
    response: Option<String>,
    #[serde(rename = "Error")]
    error: Option<String>,
    #[serde(rename = "Ratings", default)]
    ratings: Vec<RawRating>,
    #[serde(rename = "imdbRating")]
    imdb_rating: Option<String>,
    #[serde(rename = "imdbVotes")]
    imdb_votes: Option<String>,
}

#[derive(Deserialize)]
struct RawRating {
    #[serde(rename = "Source")]
    source: String,
    #[serde(rename = "Value")]
    value: String,
}

/// Replaces an item's OMDb ratings and marks them checked, found or not.
pub async fn store(
    pool: &SqlitePool,
    item: i64,
    ratings: &[(String, RatingData)],
) -> Result<(), sqlx::Error> {
    let mut tx = crate::db::begin_write(pool).await?;
    for source in SOURCES {
        sqlx::query("DELETE FROM item_rating WHERE item_id = ? AND source = ?")
            .bind(item)
            .bind(source)
            .execute(&mut *tx)
            .await?;
    }
    for (source, r) in ratings {
        sqlx::query(
            "INSERT INTO item_rating (item_id, source, value, scale, votes) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(item)
        .bind(source)
        .bind(r.value)
        .bind(r.scale)
        .bind(r.votes)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query("UPDATE item SET ratings_checked_at = unixepoch() WHERE id = ?")
        .bind(item)
        .execute(&mut *tx)
        .await?;
    tx.commit().await
}

/// Fetches and stores one series' or movie's ratings. Items without an IMDb ID
/// are skipped.
pub async fn refresh(pool: &SqlitePool, omdb: &Omdb, item: i64) -> Result<(), MetadataError> {
    let imdb: Option<String> = sqlx::query_scalar(
        "SELECT value FROM external_id WHERE item_id = ? AND provider = 'imdb' ORDER BY value LIMIT 1",
    )
    .bind(item)
    .fetch_optional(pool)
    .await
    .map_err(|e| MetadataError::Invalid(e.to_string()))?;
    let Some(imdb) = imdb else {
        return Ok(());
    };
    let ratings = match omdb.ratings(&imdb).await {
        Ok(ratings) => ratings,
        Err(MetadataError::NotFound) => Vec::new(),
        Err(e) => return Err(e),
    };
    store(pool, item, &ratings)
        .await
        .map_err(|e| MetadataError::Invalid(e.to_string()))
}

/// Series and movies with an IMDb ID whose ratings are a week old or missing, oldest first.
pub async fn due(pool: &SqlitePool, now: i64) -> Result<Vec<i64>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT i.id FROM item i
         WHERE i.kind IN ('series', 'movie')
           AND EXISTS (SELECT 1 FROM external_id x WHERE x.item_id = i.id AND x.provider = 'imdb')
           AND coalesce(i.ratings_checked_at, 0) < ?
         ORDER BY coalesce(i.ratings_checked_at, 0), i.id",
    )
    .bind(now - WEEK)
    .fetch_all(pool)
    .await
}

/// Refreshes due ratings every `every`, one item at a time.
pub fn spawn_refresher(pool: SqlitePool, omdb: Omdb, every: Duration) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(every);
        loop {
            tick.tick().await;
            let now = chrono::Utc::now().timestamp();
            let items = match due(&pool, now).await {
                Ok(items) => items,
                Err(e) => {
                    tracing::error!(error = %e, "can't list items due a ratings refresh");
                    continue;
                }
            };
            for item in items {
                match refresh(&pool, &omdb, item).await {
                    Ok(()) => {}
                    Err(MetadataError::Unreachable(e) | MetadataError::Auth(e)) => {
                        // Offline, over the daily limit or a bad key: try again next tick.
                        tracing::warn!(error = e, "OMDb unavailable; ratings refresh paused");
                        break;
                    }
                    Err(e) => tracing::warn!(item, error = %e, "ratings refresh failed"),
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_each_source() {
        let t: RawTitle = serde_json::from_str(
            r#"{"Title": "Harbour Lights", "imdbRating": "8.1", "imdbVotes": "1,234,567",
                "Ratings": [
                    {"Source": "Internet Movie Database", "Value": "8.1/10"},
                    {"Source": "Rotten Tomatoes", "Value": "94%"},
                    {"Source": "Metacritic", "Value": "74/100"}],
                "Response": "True"}"#,
        )
        .unwrap();
        let r = ratings_from(t);
        let got: Vec<(&str, f64, f64, Option<i64>)> = r
            .iter()
            .map(|(s, r)| (s.as_str(), r.value, r.scale, r.votes))
            .collect();
        assert_eq!(
            got,
            [
                ("imdb", 8.1, 10.0, Some(1_234_567)),
                ("rotten_tomatoes", 94.0, 100.0, None),
                ("metacritic", 74.0, 100.0, None),
            ]
        );
    }

    #[test]
    fn series_with_only_top_level_imdb_fields() {
        let t: RawTitle = serde_json::from_str(
            r#"{"imdbRating": "7.0", "imdbVotes": "N/A", "Ratings": [], "Response": "True"}"#,
        )
        .unwrap();
        let r = ratings_from(t);
        assert_eq!(r.len(), 1);
        assert_eq!(
            (r[0].0.as_str(), r[0].1.value, r[0].1.votes),
            ("imdb", 7.0, None)
        );

        let none: RawTitle =
            serde_json::from_str(r#"{"imdbRating": "N/A", "Response": "True"}"#).unwrap();
        assert!(ratings_from(none).is_empty());
    }
}
