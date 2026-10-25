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
    let done =
        sqlx::query("UPDATE item SET match_ordering_id = ? WHERE id = ? AND kind = 'series'")
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
