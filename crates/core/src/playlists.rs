//! Playlists: a user's (or everyone's) own path through a series.
//!
//! Smart playlists follow a base ordering, leaving out episodes with any of the
//! excluded tags (`filler`, say), so they stay right as episodes are added.
//! Manual playlists list episodes in the order given. Either kind belongs to
//! one series. Shared playlists have no owner and only admins change them.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::SqlitePool;
use utoipa::ToSchema;

#[derive(Debug, thiserror::Error)]
pub enum PlaylistError {
    #[error("not found")]
    NotFound,
    #[error("Only an admin can change shared playlists.")]
    Forbidden,
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Playlist {
    pub id: i64,
    pub name: String,
    /// `smart` or `manual`.
    pub kind: String,
    pub series_id: i64,
    /// Everyone sees it; otherwise only its owner.
    pub shared: bool,
    pub owner_user_id: Option<i64>,
    /// Smart: the ordering it follows.
    pub base_ordering_id: Option<i64>,
    /// Smart: episodes with any of these tags are left out.
    pub exclude_tags: Vec<String>,
    /// Manual: the episodes, in order.
    pub items: Vec<i64>,
}

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct PlaylistInput {
    pub name: Option<String>,
    /// `smart` or `manual` (create only).
    pub kind: Option<String>,
    /// Make it everyone's (admins only; create only).
    #[serde(default)]
    pub shared: bool,
    /// Smart: an ordering of the series.
    pub base_ordering_id: Option<i64>,
    pub exclude_tags: Option<Vec<String>>,
    /// Manual: episode IDs of one series, in order.
    pub items: Option<Vec<i64>>,
}

/// Who is asking: their ID and whether they're an admin.
#[derive(Debug, Clone, Copy)]
pub struct Actor {
    pub user: i64,
    pub admin: bool,
}

#[derive(sqlx::FromRow)]
struct Row {
    id: i64,
    name: String,
    kind: String,
    owner_user_id: Option<i64>,
    base_ordering_id: Option<i64>,
    rules: Option<String>,
    series_id: Option<i64>,
}

const SELECT: &str = "SELECT p.id, p.name, p.kind, p.owner_user_id, p.base_ordering_id, p.rules,
        coalesce(o.series_id,
                 (SELECT i.parent_id FROM playlist_entry pe JOIN item i ON i.id = pe.item_id
                  WHERE pe.playlist_id = p.id LIMIT 1)) AS series_id
    FROM playlist p LEFT JOIN ordering o ON o.id = p.base_ordering_id";

async fn load(pool: &SqlitePool, row: Row) -> Result<Option<Playlist>, sqlx::Error> {
    let Some(series_id) = row.series_id else {
        return Ok(None);
    };
    let items = sqlx::query_scalar(
        "SELECT item_id FROM playlist_entry WHERE playlist_id = ? ORDER BY position",
    )
    .bind(row.id)
    .fetch_all(pool)
    .await?;
    let rules: serde_json::Value = row
        .rules
        .and_then(|r| serde_json::from_str(&r).ok())
        .unwrap_or_default();
    let exclude_tags = rules["exclude_tags"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|t| t.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    Ok(Some(Playlist {
        id: row.id,
        name: row.name,
        kind: row.kind,
        series_id,
        shared: row.owner_user_id.is_none(),
        owner_user_id: row.owner_user_id,
        base_ordering_id: row.base_ordering_id,
        exclude_tags,
        items,
    }))
}

/// The playlists a user can see, optionally only one series'.
pub async fn list(
    pool: &SqlitePool,
    user: i64,
    series: Option<i64>,
) -> Result<Vec<Playlist>, sqlx::Error> {
    let rows: Vec<Row> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{SELECT} WHERE p.owner_user_id IS NULL OR p.owner_user_id = ? ORDER BY p.name COLLATE NOCASE, p.id"
    )))
    .bind(user)
    .fetch_all(pool)
    .await?;
    let mut out = Vec::new();
    for row in rows {
        if let Some(playlist) = load(pool, row).await?
            && series.is_none_or(|s| s == playlist.series_id)
        {
            out.push(playlist);
        }
    }
    Ok(out)
}

/// A playlist the user can see.
pub async fn get(pool: &SqlitePool, user: i64, id: i64) -> Result<Playlist, PlaylistError> {
    let row: Option<Row> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{SELECT} WHERE p.id = ? AND (p.owner_user_id IS NULL OR p.owner_user_id = ?)"
    )))
    .bind(id)
    .bind(user)
    .fetch_optional(pool)
    .await?;
    match row {
        Some(row) => load(pool, row).await?.ok_or(PlaylistError::NotFound),
        None => Err(PlaylistError::NotFound),
    }
}

fn check_name(name: Option<&str>) -> Result<String, PlaylistError> {
    let name = name.unwrap_or_default().trim();
    if name.is_empty() || name.len() > 100 {
        return Err(PlaylistError::Invalid(
            "Give the playlist a name of up to 100 characters.".into(),
        ));
    }
    Ok(name.to_string())
}

async fn ordering_series(pool: &SqlitePool, ordering: i64) -> Result<i64, PlaylistError> {
    sqlx::query_scalar("SELECT series_id FROM ordering WHERE id = ?")
        .bind(ordering)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| PlaylistError::Invalid("That ordering doesn't exist.".into()))
}

/// The series the episodes all belong to.
async fn items_series(pool: &SqlitePool, items: &[i64]) -> Result<i64, PlaylistError> {
    if items.is_empty() {
        return Err(PlaylistError::Invalid("Add at least one episode.".into()));
    }
    let series: Vec<(Option<i64>, String)> = sqlx::query_as(
        "SELECT parent_id, kind FROM item WHERE id IN (SELECT value FROM json_each(?))",
    )
    .bind(json!(items).to_string())
    .fetch_all(pool)
    .await?;
    let distinct: HashSet<i64> = items.iter().copied().collect();
    let parents: HashSet<Option<i64>> = series.iter().map(|(p, _)| *p).collect();
    let all_episodes = series.iter().all(|(_, kind)| kind == "episode");
    match (
        series.len() == distinct.len() && all_episodes,
        parents.len(),
    ) {
        (true, 1) => Ok(parents
            .into_iter()
            .next()
            .flatten()
            .expect("episodes have a series")),
        _ => Err(PlaylistError::Invalid(
            "Playlists hold existing episodes of one series.".into(),
        )),
    }
}

fn rules(exclude_tags: &[String]) -> String {
    let tags: Vec<String> = exclude_tags
        .iter()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect();
    json!({ "exclude_tags": tags }).to_string()
}

async fn write_entries(
    tx: &mut sqlx::Transaction<'static, sqlx::Sqlite>,
    id: i64,
    items: &[i64],
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM playlist_entry WHERE playlist_id = ?")
        .bind(id)
        .execute(&mut **tx)
        .await?;
    for (position, item) in items.iter().enumerate() {
        sqlx::query("INSERT INTO playlist_entry (playlist_id, position, item_id) VALUES (?, ?, ?)")
            .bind(id)
            .bind(position as i64 + 1)
            .bind(item)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

pub async fn create(
    pool: &SqlitePool,
    actor: Actor,
    input: PlaylistInput,
) -> Result<Playlist, PlaylistError> {
    if input.shared && !actor.admin {
        return Err(PlaylistError::Forbidden);
    }
    let name = check_name(input.name.as_deref())?;
    let owner = (!input.shared).then_some(actor.user);
    let mut tx;
    let id: i64;
    match input.kind.as_deref() {
        Some("smart") => {
            let Some(base) = input.base_ordering_id else {
                return Err(PlaylistError::Invalid(
                    "A smart playlist follows an ordering.".into(),
                ));
            };
            ordering_series(pool, base).await?;
            tx = crate::db::begin_write(pool).await?;
            id = sqlx::query_scalar(
                "INSERT INTO playlist (owner_user_id, name, kind, base_ordering_id, rules)
                 VALUES (?, ?, 'smart', ?, ?) RETURNING id",
            )
            .bind(owner)
            .bind(&name)
            .bind(base)
            .bind(rules(&input.exclude_tags.unwrap_or_default()))
            .fetch_one(&mut *tx)
            .await?;
        }
        Some("manual") => {
            let items = input.items.unwrap_or_default();
            items_series(pool, &items).await?;
            tx = crate::db::begin_write(pool).await?;
            id = sqlx::query_scalar(
                "INSERT INTO playlist (owner_user_id, name, kind) VALUES (?, ?, 'manual') RETURNING id",
            )
            .bind(owner)
            .bind(&name)
            .fetch_one(&mut *tx)
            .await?;
            write_entries(&mut tx, id, &items).await?;
        }
        _ => {
            return Err(PlaylistError::Invalid(
                "The kind must be smart or manual.".into(),
            ));
        }
    }
    tx.commit().await?;
    get(pool, actor.user, id).await
}

async fn editable(pool: &SqlitePool, actor: Actor, id: i64) -> Result<Playlist, PlaylistError> {
    let playlist = get(pool, actor.user, id).await?;
    if playlist.shared && !actor.admin {
        return Err(PlaylistError::Forbidden);
    }
    Ok(playlist)
}

/// Renames a playlist or changes its rules or episodes. A manual playlist keeps
/// to its series.
pub async fn update(
    pool: &SqlitePool,
    actor: Actor,
    id: i64,
    input: PlaylistInput,
) -> Result<Playlist, PlaylistError> {
    let playlist = editable(pool, actor, id).await?;
    let mut tx = crate::db::begin_write(pool).await?;
    if input.name.is_some() {
        sqlx::query("UPDATE playlist SET name = ? WHERE id = ?")
            .bind(check_name(input.name.as_deref())?)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    if playlist.kind == "smart" {
        if let Some(base) = input.base_ordering_id {
            if ordering_series(pool, base).await? != playlist.series_id {
                return Err(PlaylistError::Invalid(
                    "Pick an ordering of the same series.".into(),
                ));
            }
            sqlx::query("UPDATE playlist SET base_ordering_id = ? WHERE id = ?")
                .bind(base)
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
        if let Some(tags) = &input.exclude_tags {
            sqlx::query("UPDATE playlist SET rules = ? WHERE id = ?")
                .bind(rules(tags))
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
    } else if let Some(items) = &input.items {
        if items_series(pool, items).await? != playlist.series_id {
            return Err(PlaylistError::Invalid(
                "Keep to episodes of the same series.".into(),
            ));
        }
        write_entries(&mut tx, id, items).await?;
    }
    // Wanted may follow this playlist.
    crate::wanted::sync_events(&mut tx).await?;
    tx.commit().await?;
    get(pool, actor.user, id).await
}

pub async fn delete(pool: &SqlitePool, actor: Actor, id: i64) -> Result<(), PlaylistError> {
    editable(pool, actor, id).await?;
    sqlx::query("DELETE FROM playlist WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}
