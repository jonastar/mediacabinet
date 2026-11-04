//! Folder libraries: every video is its own item, named after its file. Nothing is
//! parsed, searched for or queued for review, and no provider is needed.
//!
//! The item is a `movie` with no provider, so playback, resume, watch state and
//! search work as they do for any movie. Which folder it's in is read from its
//! file's path, so a video moved on disk keeps its item and watch history.

use std::path::Path;

use serde_json::json;
use sqlx::SqlitePool;

use super::{FileRow, MatchError, MatchReport, in_any_folder};
use crate::events;

/// What a video is called: its file name, without the extension.
fn title_of(relative_path: &str) -> String {
    Path::new(relative_path).file_stem().map_or_else(
        || relative_path.to_string(),
        |stem| stem.to_string_lossy().into_owned(),
    )
}

fn runtime_of(duration: Option<f64>) -> Option<i64> {
    duration.map(|d| d.round() as i64)
}

/// Gives each of `files` (present, without a link) an item, then brings the
/// items of the root's other files up to date with their files.
pub(super) async fn link_videos(
    pool: &SqlitePool,
    root_id: i64,
    files: Vec<FileRow>,
    ignored_folders: &[String],
    progress: impl Fn(usize, usize),
) -> Result<MatchReport, MatchError> {
    let mut report = MatchReport::default();
    let total = files.len();
    progress(0, total);
    for (done, file) in files.into_iter().enumerate() {
        progress(done + 1, total);
        if in_any_folder(&file.relative_path, ignored_folders) {
            sqlx::query("UPDATE media_file SET status = 'ignored' WHERE id = ?")
                .bind(file.id)
                .execute(pool)
                .await?;
            report.ignored += 1;
            continue;
        }
        add_video(pool, root_id, &file).await?;
        report.identified += 1;
        report.linked += 1;
    }
    sync_with_files(pool, root_id).await?;
    Ok(report)
}

async fn add_video(pool: &SqlitePool, root_id: i64, file: &FileRow) -> Result<(), sqlx::Error> {
    let mut tx = crate::db::begin_write(pool).await?;
    let item: i64 = sqlx::query_scalar(
        "INSERT INTO item (library_id, kind, title, runtime_seconds)
         SELECT library_id, 'movie', ?, ? FROM library_root WHERE id = ? RETURNING id",
    )
    .bind(title_of(&file.relative_path))
    .bind(runtime_of(file.duration_seconds))
    .bind(root_id)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO file_link (file_id, item_id, library_id, confidence, source)
         SELECT ?1, ?2, library_id, 1.0, 'parser' FROM media_file WHERE id = ?1",
    )
    .bind(file.id)
    .bind(item)
    .execute(&mut *tx)
    .await?;
    events::emit(&mut tx, events::ITEM_ADDED, json!({ "item_id": item })).await?;
    events::emit(
        &mut tx,
        events::LINK_CREATED,
        json!({ "file_id": file.id, "item_id": item, "source": "parser", "confidence": 1.0 }),
    )
    .await?;
    tx.commit().await
}

/// A file renamed or replaced on disk renames its item and updates its runtime,
/// unless someone locked those fields.
async fn sync_with_files(pool: &SqlitePool, root_id: i64) -> Result<(), sqlx::Error> {
    type Row = (i64, String, Option<i64>, String, String, Option<f64>);
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT i.id, i.title, i.runtime_seconds, i.metadata_locked, f.relative_path,
                f.duration_seconds
         FROM file_link l
         JOIN media_file f ON f.id = l.file_id
         JOIN item i ON i.id = l.item_id
         WHERE f.root_id = ? AND f.status = 'present'
           AND (SELECT count(*) FROM file_link WHERE item_id = i.id) = 1",
    )
    .bind(root_id)
    .fetch_all(pool)
    .await?;
    for (item, title, runtime, locked, path, duration) in rows {
        let locked: Vec<String> = serde_json::from_str(&locked).unwrap_or_default();
        let is_locked = |field: &str| locked.iter().any(|l| l == field);
        let new_title = if is_locked("title") {
            title.clone()
        } else {
            title_of(&path)
        };
        let new_runtime = if is_locked("runtime_seconds") {
            runtime
        } else {
            runtime_of(duration).or(runtime)
        };
        if new_title != title || new_runtime != runtime {
            sqlx::query(
                "UPDATE item SET title = ?, runtime_seconds = ?, updated_at = unixepoch() WHERE id = ?",
            )
            .bind(new_title)
            .bind(new_runtime)
            .bind(item)
            .execute(pool)
            .await?;
        }
    }
    Ok(())
}
