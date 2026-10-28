//! Imports provider data into the library model, and refreshes it.
//!
//! Import and refresh are one operation: refreshing is importing onto an
//! existing series. The rules, from the spec:
//!
//! - Episodes are matched by the primary provider's episode ID. If an ID no
//!   longer resolves, fall back to aired position plus title or air date, so an
//!   item is never duplicated (that would orphan links and watch history).
//! - New episodes become new items.
//! - Episodes the provider dropped are flagged (`removed_upstream_at`) if they
//!   have links or watch state, and deleted otherwise.
//! - Fields listed in `metadata_locked` are never overwritten.

use std::collections::{HashMap, HashSet};

use serde::Serialize;
use serde_json::json;
use sqlx::{SqliteConnection, SqlitePool};
use utoipa::ToSchema;

use super::fake::normalize;
use super::model::*;
use crate::events;

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("{0}")]
    Conflict(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, ToSchema)]
pub struct ImportReport {
    pub item_id: i64,
    /// True when the series or movie was new.
    pub created: bool,
    pub episodes_added: usize,
    pub episodes_updated: usize,
    /// Dropped by the provider but kept: they have files or watch history.
    pub episodes_flagged: usize,
    pub episodes_removed: usize,
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

/// The series or movie in `library` with this primary provider ID.
async fn find_primary(
    conn: &mut SqliteConnection,
    library: i64,
    provider: &str,
    id: &str,
    kind: &str,
) -> Result<Option<i64>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT item_id FROM external_id
         WHERE provider = ? AND value = ? AND is_primary AND kind = ? AND library_id = ?",
    )
    .bind(provider)
    .bind(id)
    .bind(kind)
    .bind(library)
    .fetch_optional(conn)
    .await
}

async fn locked(conn: &mut SqliteConnection, item: i64) -> Result<HashSet<String>, sqlx::Error> {
    let raw: String = sqlx::query_scalar("SELECT metadata_locked FROM item WHERE id = ?")
        .bind(item)
        .fetch_one(conn)
        .await?;
    Ok(serde_json::from_str::<Vec<String>>(&raw)
        .unwrap_or_default()
        .into_iter()
        .collect())
}

/// Sets each (column, value) unless the field is locked.
async fn update_fields(
    conn: &mut SqliteConnection,
    item: i64,
    fields: &[(&'static str, Value)],
) -> Result<bool, sqlx::Error> {
    let locks = locked(conn, item).await?;
    let mut changed = false;
    for (column, value) in fields {
        if locks.contains(*column) {
            continue;
        }
        // Column names come from this file only; values are bound.
        let sql: &'static str = match *column {
            "title" => "UPDATE item SET title = ? WHERE id = ? AND title IS NOT ?",
            "original_title" => {
                "UPDATE item SET original_title = ? WHERE id = ? AND original_title IS NOT ?"
            }
            "year" => "UPDATE item SET year = ? WHERE id = ? AND year IS NOT ?",
            "overview" => "UPDATE item SET overview = ? WHERE id = ? AND overview IS NOT ?",
            "runtime_seconds" => {
                "UPDATE item SET runtime_seconds = ? WHERE id = ? AND runtime_seconds IS NOT ?"
            }
            "airs_at" => "UPDATE item SET airs_at = ? WHERE id = ? AND airs_at IS NOT ?",
            "airs_tz" => "UPDATE item SET airs_tz = ? WHERE id = ? AND airs_tz IS NOT ?",
            "series_status" => "UPDATE series SET status = ? WHERE item_id = ? AND status IS NOT ?",
            _ => unreachable!("unknown field {column}"),
        };
        let query = sqlx::query(sql);
        let query = match value {
            Value::Text(v) => query.bind(v.clone()).bind(item).bind(v.clone()),
            Value::Int(v) => query.bind(*v).bind(item).bind(*v),
        };
        changed |= query.execute(&mut *conn).await?.rows_affected() > 0;
    }
    if changed {
        sqlx::query("UPDATE item SET updated_at = unixepoch() WHERE id = ?")
            .bind(item)
            .execute(&mut *conn)
            .await?;
    }
    Ok(changed)
}

enum Value {
    Text(Option<String>),
    Int(Option<i64>),
}

async fn set_external_ids(
    conn: &mut SqliteConnection,
    item: i64,
    provider: &str,
    id: &str,
    others: &[(String, String)],
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM external_id WHERE item_id = ? AND is_primary AND (provider <> ? OR value <> ?)")
        .bind(item)
        .bind(provider)
        .bind(id)
        .execute(&mut *conn)
        .await?;
    // Only an existing row is skipped: a primary ID taken by another item is an error.
    sqlx::query(
        "INSERT INTO external_id (item_id, library_id, kind, provider, value, is_primary)
         SELECT id, library_id, kind, ?2, ?3, 1 FROM item WHERE id = ?1
         ON CONFLICT (item_id, provider, value) DO NOTHING",
    )
    .bind(item)
    .bind(provider)
    .bind(id)
    .execute(&mut *conn)
    .await?;
    for (p, v) in others {
        sqlx::query(
            "INSERT INTO external_id (item_id, library_id, kind, provider, value)
             SELECT id, library_id, kind, ?2, ?3 FROM item WHERE id = ?1
             ON CONFLICT (item_id, provider, value) DO NOTHING",
        )
        .bind(item)
        .bind(p)
        .bind(v)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

async fn set_genres(
    conn: &mut SqliteConnection,
    item: i64,
    provider: &str,
    genres: &[String],
) -> Result<(), sqlx::Error> {
    let source = format!("{provider}:genre");
    sqlx::query("DELETE FROM item_tag WHERE item_id = ? AND source = ?")
        .bind(item)
        .bind(&source)
        .execute(&mut *conn)
        .await?;
    for g in genres {
        sqlx::query("INSERT OR IGNORE INTO item_tag (item_id, tag, source) VALUES (?, ?, ?)")
            .bind(item)
            .bind(format!("genre:{g}"))
            .bind(&source)
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}

async fn set_artwork(
    conn: &mut SqliteConnection,
    item: i64,
    provider: &str,
    art: &[(&str, &Option<String>)],
) -> Result<(), sqlx::Error> {
    for (kind, path) in art {
        match path {
            Some(path) => {
                sqlx::query(
                    "INSERT INTO item_artwork (item_id, kind, provider, path) VALUES (?, ?, ?, ?)
                     ON CONFLICT (item_id, kind) DO UPDATE SET provider = excluded.provider, path = excluded.path",
                )
                .bind(item)
                .bind(*kind)
                .bind(provider)
                .bind(path)
                .execute(&mut *conn)
                .await?;
            }
            None => {
                sqlx::query(
                    "DELETE FROM item_artwork WHERE item_id = ? AND kind = ? AND provider = ?",
                )
                .bind(item)
                .bind(*kind)
                .bind(provider)
                .execute(&mut *conn)
                .await?;
            }
        }
    }
    Ok(())
}

/// The library an item belongs to.
pub async fn item_library(conn: &mut SqliteConnection, item: i64) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT library_id FROM item WHERE id = ?")
        .bind(item)
        .fetch_one(conn)
        .await
}

// ── Series ─────────────────────────────────────────────────────────────────

/// Imports a series into `library`, or refreshes it if it's already there.
pub async fn import_series(
    pool: &SqlitePool,
    library: i64,
    data: &SeriesData,
) -> Result<ImportReport, ImportError> {
    let mut tx = crate::db::begin_write(pool).await?;
    let existing = find_primary(&mut tx, library, &data.provider, &data.id, "series").await?;
    let report = sync_series(&mut tx, library, existing, data).await?;
    tx.commit().await?;
    Ok(report)
}

/// Refreshes `series_id` from `data`, which must be the same series.
pub async fn refresh_series(
    pool: &SqlitePool,
    series_id: i64,
    data: &SeriesData,
) -> Result<ImportReport, ImportError> {
    let mut tx = crate::db::begin_write(pool).await?;
    let library = item_library(&mut tx, series_id).await?;
    if let Some(other) = find_primary(&mut tx, library, &data.provider, &data.id, "series").await?
        && other != series_id
    {
        return Err(ImportError::Conflict(format!(
            "{}:{} already belongs to another series in the library",
            data.provider, data.id
        )));
    }
    let report = sync_series(&mut tx, library, Some(series_id), data).await?;
    tx.commit().await?;
    Ok(report)
}

#[derive(sqlx::FromRow)]
struct KnownEpisode {
    id: i64,
    title: String,
    airs_at: Option<i64>,
    provider_id: Option<String>,
    season: Option<i64>,
    number: Option<i64>,
    removed_upstream_at: Option<i64>,
    has_history: bool,
}

async fn sync_series(
    tx: &mut SqliteConnection,
    library: i64,
    existing: Option<i64>,
    data: &SeriesData,
) -> Result<ImportReport, ImportError> {
    let provider = data.provider.as_str();
    let mut report = ImportReport::default();
    let series = match existing {
        Some(id) => id,
        None => {
            report.created = true;
            let id: i64 = sqlx::query_scalar(
                "INSERT INTO item (kind, title, primary_provider, library_id)
                 VALUES ('series', ?, ?, ?) RETURNING id",
            )
            .bind(&data.title)
            .bind(provider)
            .bind(library)
            .fetch_one(&mut *tx)
            .await?;
            sqlx::query("INSERT INTO series (item_id) VALUES (?)")
                .bind(id)
                .execute(&mut *tx)
                .await?;
            id
        }
    };
    report.item_id = series;
    update_fields(
        tx,
        series,
        &[
            ("title", Value::Text(Some(data.title.clone()))),
            ("original_title", Value::Text(data.original_title.clone())),
            ("year", Value::Int(data.year)),
            ("overview", Value::Text(data.overview.clone())),
            (
                "series_status",
                Value::Text(data.status.map(|s| s.as_str().to_string())),
            ),
        ],
    )
    .await?;
    set_external_ids(tx, series, provider, &data.id, &data.external_ids).await?;
    set_genres(tx, series, provider, &data.genres).await?;
    set_artwork(
        tx,
        series,
        provider,
        &[("poster", &data.poster), ("backdrop", &data.backdrop)],
    )
    .await?;

    // Episodes.
    let known: Vec<KnownEpisode> = sqlx::query_as(
        "SELECT e.id, e.title, e.airs_at,
                (SELECT value FROM external_id WHERE item_id = e.id AND provider = ?1 AND is_primary) AS provider_id,
                os.number AS season, oe.episode_number AS number, e.removed_upstream_at,
                EXISTS (SELECT 1 FROM file_link WHERE item_id = e.id)
                    OR EXISTS (SELECT 1 FROM user_item_state WHERE item_id = e.id) AS has_history
         FROM item e
         LEFT JOIN ordering o ON o.series_id = e.parent_id AND o.kind = 'aired' AND o.source = ?1
         LEFT JOIN ordering_entry oe ON oe.ordering_id = o.id AND oe.item_id = e.id
         LEFT JOIN ordering_season os ON os.id = oe.ordering_season_id
         WHERE e.parent_id = ?2
         GROUP BY e.id",
    )
    .bind(provider)
    .bind(series)
    .fetch_all(&mut *tx)
    .await?;

    let by_provider_id: HashMap<&str, &KnownEpisode> = known
        .iter()
        .filter_map(|k| k.provider_id.as_deref().map(|p| (p, k)))
        .collect();
    let incoming_ids: HashSet<&str> = data.episodes.iter().map(|e| e.id.as_str()).collect();
    let mut claimed: HashSet<i64> = HashSet::new();
    let mut item_of: HashMap<&str, i64> = HashMap::new();

    for e in &data.episodes {
        let mut item = by_provider_id.get(e.id.as_str()).map(|k| k.id);
        if item.is_none() {
            // The ID changed upstream: reconcile by aired position plus title or date,
            // among known episodes whose own ID no longer resolves.
            item = known
                .iter()
                .filter(|k| !claimed.contains(&k.id))
                .filter(|k| {
                    k.provider_id
                        .as_deref()
                        .is_none_or(|p| !incoming_ids.contains(p))
                })
                .find(|k| {
                    let same_place = k.season == Some(e.season) && k.number == Some(e.number);
                    let same_title = normalize(&k.title) == normalize(&e.title);
                    let same_day = match (k.airs_at, e.airs) {
                        (Some(a), Some(b)) => (a - b.at).abs() < 86400,
                        _ => false,
                    };
                    same_place && (same_title || same_day)
                })
                .map(|k| k.id);
        }
        let id = match item {
            Some(id) => {
                if update_episode(tx, id, e).await? {
                    report.episodes_updated += 1;
                }
                id
            }
            None => {
                let id: i64 = sqlx::query_scalar(
                    "INSERT INTO item (parent_id, kind, title, library_id)
                     VALUES (?, 'episode', ?, ?) RETURNING id",
                )
                .bind(series)
                .bind(&e.title)
                .bind(library)
                .fetch_one(&mut *tx)
                .await?;
                update_episode(tx, id, e).await?;
                events::emit(
                    tx,
                    events::ITEM_ADDED,
                    json!({ "item_id": id, "series_id": series }),
                )
                .await?;
                report.episodes_added += 1;
                id
            }
        };
        set_external_ids(tx, id, provider, &e.id, &[]).await?;
        sqlx::query("UPDATE item SET removed_upstream_at = NULL WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        claimed.insert(id);
        item_of.insert(e.id.as_str(), id);
    }

    for k in known.iter().filter(|k| !claimed.contains(&k.id)) {
        if k.has_history {
            if k.removed_upstream_at.is_none() {
                sqlx::query("UPDATE item SET removed_upstream_at = unixepoch() WHERE id = ?")
                    .bind(k.id)
                    .execute(&mut *tx)
                    .await?;
                report.episodes_flagged += 1;
            }
        } else {
            sqlx::query("DELETE FROM item WHERE id = ?")
                .bind(k.id)
                .execute(&mut *tx)
                .await?;
            report.episodes_removed += 1;
        }
    }

    sync_orderings(tx, series, data, &item_of).await?;

    sqlx::query("UPDATE item SET refreshed_at = unixepoch(), refresh_error = NULL WHERE id = ?")
        .bind(series)
        .execute(&mut *tx)
        .await?;
    let kind = if report.created {
        events::ITEM_ADDED
    } else {
        events::SERIES_REFRESHED
    };
    events::emit(tx, kind, json!({ "item_id": series, "report": report })).await?;
    Ok(report)
}

async fn update_episode(
    tx: &mut SqliteConnection,
    id: i64,
    e: &EpisodeData,
) -> Result<bool, sqlx::Error> {
    update_fields(
        tx,
        id,
        &[
            ("title", Value::Text(Some(e.title.clone()))),
            ("overview", Value::Text(e.overview.clone())),
            ("airs_at", Value::Int(e.airs.map(|a| a.at))),
            ("airs_tz", Value::Text(e.airs_tz.clone())),
            (
                "runtime_seconds",
                Value::Int(e.runtime_minutes.map(|m| m * 60)),
            ),
        ],
    )
    .await
}

/// Rebuilds the provider's orderings. Ordering IDs are kept, since user
/// preferences and playlists point at them.
async fn sync_orderings(
    tx: &mut SqliteConnection,
    series: i64,
    data: &SeriesData,
    item_of: &HashMap<&str, i64>,
) -> Result<(), sqlx::Error> {
    let provider = data.provider.as_str();

    // Aired order from the episodes; specials last so they never read as gaps.
    let mut aired: Vec<&EpisodeData> = data.episodes.iter().collect();
    aired.sort_by_key(|e| (e.season == 0, e.season, e.number));
    let entries: Vec<(i64, Option<i64>, i64)> = aired
        .iter()
        .map(|e| (item_of[e.id.as_str()], Some(e.season), e.number))
        .collect();
    let has_default: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM ordering WHERE series_id = ? AND is_default)",
    )
    .bind(series)
    .fetch_one(&mut *tx)
    .await?;
    write_ordering(
        tx,
        series,
        "Aired",
        "aired",
        provider,
        !has_default,
        &entries,
        &data.seasons,
    )
    .await?;

    let mut has_absolute = false;
    for o in &data.orderings {
        has_absolute |= o.kind == "absolute";
        let entries: Vec<(i64, Option<i64>, i64)> = o
            .entries
            .iter()
            .filter_map(|e| {
                item_of
                    .get(e.episode_id.as_str())
                    .map(|id| (*id, e.season, e.number))
            })
            .collect();
        write_ordering(tx, series, &o.name, &o.kind, provider, false, &entries, &[]).await?;
    }

    // Anime files are often numbered absolutely; derive that order if the provider has none.
    let regular: Vec<&&EpisodeData> = aired.iter().filter(|e| e.season > 0).collect();
    let seasons: HashSet<i64> = regular.iter().map(|e| e.season).collect();
    if !has_absolute && seasons.len() > 1 {
        let entries: Vec<(i64, Option<i64>, i64)> = regular
            .iter()
            .enumerate()
            .map(|(i, e)| (item_of[e.id.as_str()], None, i as i64 + 1))
            .collect();
        write_ordering(
            tx,
            series,
            "Absolute",
            "absolute",
            "computed",
            false,
            &entries,
            &[],
        )
        .await?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn write_ordering(
    tx: &mut SqliteConnection,
    series: i64,
    name: &str,
    kind: &str,
    source: &str,
    make_default: bool,
    entries: &[(i64, Option<i64>, i64)],
    seasons: &[SeasonData],
) -> Result<i64, sqlx::Error> {
    let existing: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM ordering WHERE series_id = ? AND name = ? AND source = ?",
    )
    .bind(series)
    .bind(name)
    .bind(source)
    .fetch_optional(&mut *tx)
    .await?;
    let ordering = match existing {
        Some(id) => {
            sqlx::query("UPDATE ordering SET kind = ? WHERE id = ?")
                .bind(kind)
                .bind(id)
                .execute(&mut *tx)
                .await?;
            sqlx::query("DELETE FROM ordering_entry WHERE ordering_id = ?")
                .bind(id)
                .execute(&mut *tx)
                .await?;
            sqlx::query("DELETE FROM ordering_season WHERE ordering_id = ?")
                .bind(id)
                .execute(&mut *tx)
                .await?;
            id
        }
        None => {
            // Another ordering may already use the name (e.g. a user's); keep both apart.
            let taken: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM ordering WHERE series_id = ? AND name = ?)",
            )
            .bind(series)
            .bind(name)
            .fetch_one(&mut *tx)
            .await?;
            let name = if taken {
                format!("{name} ({source})")
            } else {
                name.to_string()
            };
            sqlx::query_scalar(
                "INSERT INTO ordering (series_id, name, kind, source, is_default) VALUES (?, ?, ?, ?, ?) RETURNING id",
            )
            .bind(series)
            .bind(name)
            .bind(kind)
            .bind(source)
            .bind(make_default)
            .fetch_one(&mut *tx)
            .await?
        }
    };

    let mut season_ids: HashMap<i64, i64> = HashMap::new();
    for (position, (item, season, number)) in entries.iter().enumerate() {
        let season_id = match season {
            Some(s) => {
                if let Some(id) = season_ids.get(s) {
                    Some(*id)
                } else {
                    let meta = seasons.iter().find(|x| x.number == *s);
                    let id: i64 = sqlx::query_scalar(
                        "INSERT INTO ordering_season (ordering_id, number, title, overview, poster)
                         VALUES (?, ?, ?, ?, ?) RETURNING id",
                    )
                    .bind(ordering)
                    .bind(s)
                    .bind(meta.and_then(|m| m.title.clone()))
                    .bind(meta.and_then(|m| m.overview.clone()))
                    .bind(meta.and_then(|m| m.poster.clone()))
                    .fetch_one(&mut *tx)
                    .await?;
                    season_ids.insert(*s, id);
                    Some(id)
                }
            }
            None => None,
        };
        sqlx::query(
            "INSERT INTO ordering_entry (ordering_id, position, item_id, ordering_season_id, episode_number)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(ordering)
        .bind(position as i64 + 1)
        .bind(item)
        .bind(season_id)
        .bind(number)
        .execute(&mut *tx)
        .await?;
    }
    Ok(ordering)
}

// ── Movies ─────────────────────────────────────────────────────────────────

/// Imports a movie into `library`, or refreshes it if it's already there.
pub async fn import_movie(
    pool: &SqlitePool,
    library: i64,
    data: &MovieData,
) -> Result<ImportReport, ImportError> {
    let mut tx = crate::db::begin_write(pool).await?;
    let mut report = ImportReport::default();
    let id = match find_primary(&mut tx, library, &data.provider, &data.id, "movie").await? {
        Some(id) => id,
        None => {
            report.created = true;
            sqlx::query_scalar(
                "INSERT INTO item (kind, title, primary_provider, library_id)
                 VALUES ('movie', ?, ?, ?) RETURNING id",
            )
            .bind(&data.title)
            .bind(&data.provider)
            .bind(library)
            .fetch_one(&mut *tx)
            .await?
        }
    };
    report.item_id = id;
    update_fields(
        &mut tx,
        id,
        &[
            ("title", Value::Text(Some(data.title.clone()))),
            ("original_title", Value::Text(data.original_title.clone())),
            ("year", Value::Int(data.year)),
            ("overview", Value::Text(data.overview.clone())),
            (
                "runtime_seconds",
                Value::Int(data.runtime_minutes.map(|m| m * 60)),
            ),
            ("airs_at", Value::Int(data.release.map(|r| r.at))),
        ],
    )
    .await?;
    set_external_ids(&mut tx, id, &data.provider, &data.id, &data.external_ids).await?;
    set_genres(&mut tx, id, &data.provider, &data.genres).await?;
    set_artwork(
        &mut tx,
        id,
        &data.provider,
        &[("poster", &data.poster), ("backdrop", &data.backdrop)],
    )
    .await?;
    sqlx::query("UPDATE item SET refreshed_at = unixepoch(), refresh_error = NULL WHERE id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    let kind = if report.created {
        events::ITEM_ADDED
    } else {
        events::ITEM_REFRESHED
    };
    events::emit(&mut tx, kind, json!({ "item_id": id })).await?;
    tx.commit().await?;
    Ok(report)
}

/// Records a failed refresh without touching the metadata already there.
pub async fn record_refresh_error(
    pool: &SqlitePool,
    item: i64,
    error: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE item SET refreshed_at = ?, refresh_error = ? WHERE id = ?")
        .bind(now())
        .bind(error)
        .bind(item)
        .execute(pool)
        .await?;
    Ok(())
}
