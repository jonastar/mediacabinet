//! Changes people and external tools make to items: edited details, locked
//! fields, tags, the matching ordering, and re-matching a series.

use std::collections::BTreeSet;

use serde::Deserialize;
use serde_json::json;
use sqlx::SqlitePool;
use utoipa::ToSchema;

use crate::events;
use crate::metadata::Provider;
use crate::metadata::import::{ImportReport, refresh_series};

#[derive(Debug, thiserror::Error)]
pub enum ItemError {
    #[error("not found")]
    NotFound,
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Provider(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

/// Fields people can edit. Absent fields are left alone.
#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct ItemEdit {
    pub title: Option<String>,
    pub sort_title: Option<String>,
    pub original_title: Option<String>,
    pub year: Option<i64>,
    pub overview: Option<String>,
    /// Lock the edited fields so metadata refreshes never overwrite them.
    #[serde(default)]
    pub lock: bool,
}

async fn locks(pool: &SqlitePool, item: i64) -> Result<BTreeSet<String>, ItemError> {
    let raw: Option<String> = sqlx::query_scalar("SELECT metadata_locked FROM item WHERE id = ?")
        .bind(item)
        .fetch_optional(pool)
        .await?;
    let raw = raw.ok_or(ItemError::NotFound)?;
    Ok(serde_json::from_str::<Vec<String>>(&raw)
        .unwrap_or_default()
        .into_iter()
        .collect())
}

pub async fn edit(pool: &SqlitePool, item: i64, e: ItemEdit) -> Result<(), ItemError> {
    let mut locked = locks(pool, item).await?;
    if e.title.as_deref().is_some_and(|t| t.trim().is_empty()) {
        return Err(ItemError::Invalid("The title can't be empty.".into()));
    }
    let mut tx = crate::db::begin_write(pool).await?;
    let mut edited = Vec::new();
    macro_rules! set {
        ($field:literal, $value:expr) => {
            if let Some(v) = $value {
                sqlx::query(concat!(
                    "UPDATE item SET ",
                    $field,
                    " = ?, updated_at = unixepoch() WHERE id = ?"
                ))
                .bind(v)
                .bind(item)
                .execute(&mut *tx)
                .await?;
                edited.push($field);
            }
        };
    }
    set!("title", e.title.map(|t| t.trim().to_string()));
    set!("sort_title", e.sort_title.filter(|t| !t.trim().is_empty()));
    set!("original_title", e.original_title);
    set!("year", e.year);
    set!("overview", e.overview);
    if e.lock {
        locked.extend(edited.iter().map(|f| f.to_string()));
        sqlx::query("UPDATE item SET metadata_locked = ? WHERE id = ?")
            .bind(json!(locked).to_string())
            .bind(item)
            .execute(&mut *tx)
            .await?;
    }
    events::emit(
        &mut tx,
        events::ITEM_EDITED,
        json!({ "item_id": item, "fields": edited }),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Replaces the locked field list. Empty unlocks everything.
pub async fn set_locks(pool: &SqlitePool, item: i64, fields: &[String]) -> Result<(), ItemError> {
    const LOCKABLE: &[&str] = &[
        "title",
        "sort_title",
        "original_title",
        "year",
        "overview",
        "runtime_seconds",
        "airs_at",
    ];
    if let Some(bad) = fields.iter().find(|f| !LOCKABLE.contains(&f.as_str())) {
        return Err(ItemError::Invalid(format!(
            "{bad} can't be locked; lockable fields: {}",
            LOCKABLE.join(", ")
        )));
    }
    locks(pool, item).await?;
    let set: BTreeSet<&String> = fields.iter().collect();
    sqlx::query("UPDATE item SET metadata_locked = ? WHERE id = ?")
        .bind(json!(set).to_string())
        .bind(item)
        .execute(pool)
        .await?;
    Ok(())
}

/// Replaces an item's tags from one source, e.g. a filler list fetched by an
/// external tool: `("animefillerlist", ["filler"])`.
pub async fn set_tags(
    pool: &SqlitePool,
    item: i64,
    source: &str,
    tags: &[String],
) -> Result<(), ItemError> {
    if source.trim().is_empty() {
        return Err(ItemError::Invalid("Tags need a source.".into()));
    }
    locks(pool, item).await?;
    let mut tx = crate::db::begin_write(pool).await?;
    sqlx::query("DELETE FROM item_tag WHERE item_id = ? AND source = ?")
        .bind(item)
        .bind(source)
        .execute(&mut *tx)
        .await?;
    for tag in tags.iter().map(|t| t.trim()).filter(|t| !t.is_empty()) {
        sqlx::query("INSERT OR IGNORE INTO item_tag (item_id, tag, source) VALUES (?, ?, ?)")
            .bind(item)
            .bind(tag)
            .bind(source)
            .execute(&mut *tx)
            .await?;
    }
    events::emit(
        &mut tx,
        events::ITEM_EDITED,
        json!({ "item_id": item, "tags": tags, "source": source }),
    )
    .await?;
    // Tags decide smart playlist membership, and so what a playlist-following series wants.
    crate::wanted::sync_events(&mut tx).await?;
    tx.commit().await?;
    Ok(())
}

/// Sets the ordering filenames are numbered in (null: the default first).
pub async fn set_match_ordering(
    pool: &SqlitePool,
    series: i64,
    ordering: Option<i64>,
) -> Result<(), ItemError> {
    if let Some(o) = ordering {
        let belongs: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM ordering WHERE id = ? AND series_id = ?)",
        )
        .bind(o)
        .bind(series)
        .fetch_one(pool)
        .await?;
        if !belongs {
            return Err(ItemError::Invalid(
                "That ordering doesn't belong to this series.".into(),
            ));
        }
    }
    let done = sqlx::query("UPDATE series SET match_ordering_id = ? WHERE item_id = ?")
        .bind(ordering)
        .bind(series)
        .execute(pool)
        .await?;
    if done.rows_affected() == 0 {
        return Err(ItemError::NotFound);
    }
    Ok(())
}

/// Points a series at a different provider entry and refreshes it from there.
/// Episodes are reconciled by position, title and air date, so links and watch
/// history carry over where the new entry lines up.
pub async fn rematch_series(
    pool: &SqlitePool,
    provider: &Provider,
    series: i64,
    provider_id: &str,
) -> Result<ImportReport, ItemError> {
    let data = provider
        .series(provider_id)
        .await
        .map_err(|e| ItemError::Provider(e.to_string()))?;
    refresh_series(pool, series, &data)
        .await
        .map_err(|e| ItemError::Invalid(e.to_string()))
}

/// The library to add a `kind` item to: `library` if given, else the library
/// of that kind with an import target, else the first one.
async fn library_for(
    pool: &SqlitePool,
    kind: &str,
    library: Option<i64>,
) -> Result<i64, ItemError> {
    let library_kind = match kind {
        "series" => "tv",
        "movie" => "movies",
        _ => return Err(ItemError::Invalid("kind is series or movie".into())),
    };
    if let Some(id) = library {
        let found: Option<String> = sqlx::query_scalar("SELECT kind FROM library WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await?;
        return match found {
            Some(k) if k == library_kind => Ok(id),
            Some(_) => Err(ItemError::Invalid(format!(
                "That library doesn't hold {kind}s."
            ))),
            None => Err(ItemError::Invalid("No such library.".into())),
        };
    }
    sqlx::query_scalar(
        "SELECT l.id FROM library l WHERE l.kind = ?
         ORDER BY EXISTS (SELECT 1 FROM library_root r WHERE r.library_id = l.id AND r.is_import_target) DESC, l.id
         LIMIT 1",
    )
    .bind(library_kind)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| {
        let what = if kind == "series" {
            "TV"
        } else {
            "movie"
        };
        ItemError::Invalid(format!("Add a {what} library first."))
    })
}

/// Adds a series or movie by its provider ID, as an *arr or a person would:
/// imports it (no files yet) into `library` (see [`library_for`]) and sets
/// monitoring. Already in that library: it's refreshed and the monitoring is
/// updated.
pub async fn add(
    pool: &SqlitePool,
    provider: &Provider,
    kind: &str,
    provider_id: &str,
    library: Option<i64>,
    monitored: bool,
    quality_profile_id: Option<i64>,
) -> Result<ImportReport, ItemError> {
    let library = library_for(pool, kind, library).await?;
    let report = match kind {
        "series" => {
            let data = provider
                .series(provider_id)
                .await
                .map_err(|e| ItemError::Provider(e.to_string()))?;
            crate::metadata::import::import_series(pool, library, &data)
                .await
                .map_err(|e| ItemError::Invalid(e.to_string()))?
        }
        "movie" => {
            let data = provider
                .movie(provider_id)
                .await
                .map_err(|e| ItemError::Provider(e.to_string()))?;
            crate::metadata::import::import_movie(pool, library, &data)
                .await
                .map_err(|e| ItemError::Invalid(e.to_string()))?
        }
        _ => return Err(ItemError::Invalid("kind is series or movie".into())),
    };
    let mut tx = crate::db::begin_write(pool).await?;
    sqlx::query(
        "INSERT INTO monitor (item_id, monitored, quality_profile_id) VALUES (?1, ?2, ?3)
         ON CONFLICT (item_id) DO UPDATE SET monitored = ?2, quality_profile_id = coalesce(?3, quality_profile_id)",
    )
    .bind(report.item_id)
    .bind(monitored)
    .bind(quality_profile_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(d) if d.is_foreign_key_violation() => ItemError::Invalid("No such quality profile.".into()),
        _ => e.into(),
    })?;
    // Like the *arrs, specials aren't monitored unless someone asks: many are
    // minisodes and extras. Only on first add, so later choices stay.
    if kind == "series" && report.created && monitored {
        sqlx::query(
            "INSERT OR IGNORE INTO monitor (item_id, monitored)
             SELECT oe.item_id, 0 FROM ordering_entry oe
             JOIN ordering o ON o.id = oe.ordering_id AND o.is_default AND o.series_id = ?
             JOIN ordering_season os ON os.id = oe.ordering_season_id AND os.number = 0",
        )
        .bind(report.item_id)
        .execute(&mut *tx)
        .await?;
    }
    crate::wanted::sync_events(&mut tx).await?;
    tx.commit().await?;
    Ok(report)
}
