//! Addressing items from outside: by internal ID, or by a provider's ID plus a
//! position in an ordering — how the *arrs think: (tvdb:12345, aired, S02E05).
//!
//! A reference is `item:<id>` (or a bare number), or `<provider>:<id>` for a
//! series or movie. Secondary provider IDs work too, so `tvdb:` IDs that TMDB
//! cross-references resolve. Episodes add a position: an ordering kind (`aired`,
//! `dvd`, `absolute`, …, or `default`) and `S02E05` or an absolute number.

use serde::Serialize;
use sqlx::SqlitePool;
use utoipa::ToSchema;

use crate::catalog::{Availability, ExternalId, ExternalStatus, LinkedFile};

#[derive(Debug, thiserror::Error)]
pub enum RefError {
    #[error("not found")]
    NotFound,
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemRef {
    Id(i64),
    Provider { provider: String, id: String },
}

impl ItemRef {
    pub fn parse(s: &str) -> Result<ItemRef, RefError> {
        let s = s.trim();
        if let Ok(id) = s.parse() {
            return Ok(ItemRef::Id(id));
        }
        match s.split_once(':') {
            Some(("item", id)) => id
                .parse()
                .map(ItemRef::Id)
                .map_err(|_| RefError::Invalid(format!("{s}: item IDs are numbers"))),
            Some((provider, id)) if !provider.is_empty() && !id.is_empty() => {
                Ok(ItemRef::Provider {
                    provider: provider.to_lowercase(),
                    id: id.to_string(),
                })
            }
            _ => Err(RefError::Invalid(format!(
                "{s}: use item:<id> or <provider>:<id>, e.g. tmdb:57243"
            ))),
        }
    }
}

/// A position in an ordering: `S02E05`, `2x05` or an absolute `14`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Position {
    pub ordering: String,
    pub season: Option<i64>,
    pub episode: i64,
}

impl Position {
    pub fn parse(ordering: &str, episode: &str) -> Result<Position, RefError> {
        let e = episode.trim().to_lowercase();
        let bad =
            || RefError::Invalid(format!("{episode}: use S02E05, 2x05 or an absolute number"));
        let (season, number) = if let Some(rest) = e.strip_prefix('s') {
            let (s, n) = rest.split_once('e').ok_or_else(bad)?;
            (
                Some(s.parse().map_err(|_| bad())?),
                n.parse().map_err(|_| bad())?,
            )
        } else if let Some((s, n)) = e.split_once('x') {
            (
                Some(s.parse().map_err(|_| bad())?),
                n.parse().map_err(|_| bad())?,
            )
        } else {
            (None, e.parse().map_err(|_| bad())?)
        };
        Ok(Position {
            ordering: ordering.trim().to_lowercase(),
            season,
            episode: number,
        })
    }
}

/// Resolves a reference (and position, for episodes) to an item ID.
pub async fn resolve(
    pool: &SqlitePool,
    r: &ItemRef,
    position: Option<&Position>,
) -> Result<i64, RefError> {
    let item: i64 = match r {
        ItemRef::Id(id) => sqlx::query_scalar("SELECT id FROM item WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await?
            .ok_or(RefError::NotFound)?,
        ItemRef::Provider { provider, id } => sqlx::query_scalar(
            "SELECT i.id FROM external_id x JOIN item i ON i.id = x.item_id
             WHERE x.provider = ? AND x.value = ? AND i.kind IN ('series', 'movie')
             ORDER BY x.is_primary DESC, i.id LIMIT 1",
        )
        .bind(provider)
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or(RefError::NotFound)?,
    };
    let Some(p) = position else {
        return Ok(item);
    };
    let ordering: i64 = sqlx::query_scalar(
        "SELECT id FROM ordering WHERE series_id = ?1
           AND (?2 = 'default' AND is_default OR kind = ?2 OR lower(name) = ?2)
         ORDER BY is_default DESC, source = 'computed', id LIMIT 1",
    )
    .bind(item)
    .bind(&p.ordering)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| RefError::Invalid(format!("this series has no {} ordering", p.ordering)))?;
    sqlx::query_scalar(
        "SELECT oe.item_id FROM ordering_entry oe
         LEFT JOIN ordering_season os ON os.id = oe.ordering_season_id
         WHERE oe.ordering_id = ? AND oe.episode_number = ? AND os.number IS ?
         ORDER BY oe.position LIMIT 1",
    )
    .bind(ordering)
    .bind(p.episode)
    .bind(p.season)
    .fetch_optional(pool)
    .await?
    .ok_or(RefError::NotFound)
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct OrderingNumber {
    pub ordering: String,
    pub kind: String,
    pub season: Option<i64>,
    pub number: Option<i64>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ItemView {
    pub id: i64,
    /// `series`, `episode` or `movie`.
    pub kind: String,
    pub title: String,
    pub year: Option<i64>,
    pub parent_id: Option<i64>,
    pub parent_title: Option<String>,
    /// Episodes and movies only.
    pub availability: Option<Availability>,
    pub monitored: Option<bool>,
    pub wanted_reason: Option<String>,
    pub airs_at: Option<i64>,
    pub external_ids: Vec<ExternalId>,
    /// Its number in every ordering it appears in.
    pub numbers: Vec<OrderingNumber>,
    pub files: Vec<LinkedFile>,
    pub statuses: Vec<ExternalStatus>,
}

/// Everything an external tool needs to know about one item.
pub async fn item_view(pool: &SqlitePool, id: i64, now: i64) -> Result<ItemView, RefError> {
    let (kind, title, year, parent_id, parent_title, airs_at): (
        String,
        String,
        Option<i64>,
        Option<i64>,
        Option<String>,
        Option<i64>,
    ) = sqlx::query_as(
        "SELECT i.kind, i.title, i.year, i.parent_id, p.title, i.airs_at
             FROM item i LEFT JOIN item p ON p.id = i.parent_id WHERE i.id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or(RefError::NotFound)?;
    let state: Option<(String, bool)> =
        sqlx::query_as("SELECT availability, monitored FROM item_state WHERE item_id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await?;
    let wanted_reason: Option<String> =
        sqlx::query_scalar("SELECT reason FROM item_wanted WHERE item_id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await?;
    let external_ids: Vec<(String, String, bool)> = sqlx::query_as(
        "SELECT provider, value, is_primary FROM external_id WHERE item_id = ? ORDER BY is_primary DESC, provider",
    )
    .bind(id)
    .fetch_all(pool)
    .await?;
    let numbers: Vec<(String, String, Option<i64>, Option<i64>)> = sqlx::query_as(
        "SELECT o.name, o.kind, os.number, oe.episode_number FROM ordering_entry oe
         JOIN ordering o ON o.id = oe.ordering_id
         LEFT JOIN ordering_season os ON os.id = oe.ordering_season_id
         WHERE oe.item_id = ? ORDER BY o.is_default DESC, o.id, oe.position",
    )
    .bind(id)
    .fetch_all(pool)
    .await?;
    let files: Vec<crate::catalog::FileRow> = sqlx::query_as(
        "SELECT l.item_id, f.id AS file_id, r.path AS root_path, f.relative_path, f.status,
                f.size, f.last_seen, f.container, v.codec AS video_codec, v.width, v.height,
                v.hdr_format, l.edition, l.part, l.source AS link_source, l.confidence, l.pinned
         FROM file_link l
         JOIN media_file f ON f.id = l.file_id
         JOIN library_root r ON r.id = f.root_id
         LEFT JOIN media_stream v ON v.file_id = f.id AND v.stream_index =
             (SELECT min(stream_index) FROM media_stream WHERE file_id = f.id AND kind = 'video')
         WHERE l.item_id = ? AND f.status <> 'removed' ORDER BY f.id",
    )
    .bind(id)
    .fetch_all(pool)
    .await?;
    let statuses: Vec<(String, String, Option<String>, i64, i64)> = sqlx::query_as(
        "SELECT source, status, detail, updated_at, expires_at FROM item_status WHERE item_id = ?",
    )
    .bind(id)
    .fetch_all(pool)
    .await?;
    Ok(ItemView {
        id,
        kind,
        title,
        year,
        parent_id,
        parent_title,
        availability: state.as_ref().map(|(a, _)| Availability::parse(a)),
        monitored: state.map(|(_, m)| m),
        wanted_reason,
        airs_at,
        external_ids: external_ids
            .into_iter()
            .map(|(provider, value, is_primary)| ExternalId {
                provider,
                value,
                is_primary,
            })
            .collect(),
        numbers: numbers
            .into_iter()
            .map(|(ordering, kind, season, number)| OrderingNumber {
                ordering,
                kind,
                season,
                number,
            })
            .collect(),
        files: files.into_iter().map(Into::into).collect(),
        statuses: statuses
            .into_iter()
            .map(
                |(source, status, detail, updated_at, expires_at)| ExternalStatus {
                    source,
                    status,
                    detail: detail.and_then(|d| serde_json::from_str(&d).ok()),
                    updated_at,
                    expires_at,
                    stale: expires_at <= now,
                },
            )
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_references_and_positions() {
        assert_eq!(ItemRef::parse("item:12").unwrap(), ItemRef::Id(12));
        assert_eq!(ItemRef::parse("12").unwrap(), ItemRef::Id(12));
        assert_eq!(
            ItemRef::parse("TVDB:78804").unwrap(),
            ItemRef::Provider {
                provider: "tvdb".into(),
                id: "78804".into()
            }
        );
        assert!(ItemRef::parse("nothing").is_err());
        assert_eq!(Position::parse("aired", "S02E05").unwrap().season, Some(2));
        assert_eq!(Position::parse("aired", "2x05").unwrap().episode, 5);
        let abs = Position::parse("Absolute", "14").unwrap();
        assert_eq!(
            (abs.ordering.as_str(), abs.season, abs.episode),
            ("absolute", None, 14)
        );
        assert!(Position::parse("aired", "S0xE1").is_err());
    }
}
