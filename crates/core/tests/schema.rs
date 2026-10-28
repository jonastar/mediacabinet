//! Checks that the schema enforces the invariants from the spec.

use sqlx::SqlitePool;
use tempfile::TempDir;

async fn db() -> (TempDir, SqlitePool) {
    let dir = tempfile::tempdir().unwrap();
    let pool = cabinet_core::db::open(&dir.path().join("cabinet.db"))
        .await
        .unwrap();
    (dir, pool)
}

async fn exec(pool: &SqlitePool, sql: &'static str) -> Result<(), sqlx::Error> {
    sqlx::raw_sql(sql).execute(pool).await.map(|_| ())
}

/// A TV library with one root, and a series with two episodes.
async fn seed(pool: &SqlitePool) {
    exec(
        pool,
        "INSERT INTO library (id, name, kind) VALUES (1, 'TV', 'tv');
         INSERT INTO library_root (id, library_id, path, layout_template)
             VALUES (1, 1, '/media/tv', '{series}/Season {season:00}/{filename}');
         INSERT INTO item (id, kind, title, primary_provider, library_id) VALUES (1, 'series', 'Show', 'tmdb', 1);
         INSERT INTO item (id, parent_id, kind, title) VALUES (2, 1, 'episode', 'Pilot');
         INSERT INTO item (id, parent_id, kind, title) VALUES (3, 1, 'episode', 'Second');",
    )
    .await
    .unwrap();
}

/// Asserts that `sql` fails on a constraint (SQLITE_CONSTRAINT, primary code 19),
/// not on a typo or any other error.
async fn assert_rejected(pool: &SqlitePool, sql: &'static str) {
    let err = match exec(pool, sql).await {
        Ok(()) => {
            panic!("expected rejection: {sql}");
        }
        Err(err) => err,
    };
    let code = err
        .as_database_error()
        .and_then(|e| e.code())
        .and_then(|c| c.parse::<i32>().ok());
    assert!(
        code.is_some_and(|c| c & 0xff == 19),
        "expected a constraint violation, got {err}: {sql}"
    );
}

#[tokio::test]
async fn foreign_keys_are_consistent_after_migration() {
    let (_dir, pool) = db().await;
    seed(&pool).await;
    let violations = sqlx::query("PRAGMA foreign_key_check")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert!(violations.is_empty());
}

#[tokio::test]
async fn episodes_need_a_series_parent() {
    let (_dir, pool) = db().await;
    seed(&pool).await;
    assert_rejected(
        &pool,
        "INSERT INTO item (kind, title) VALUES ('episode', 'Orphan')",
    )
    .await;
    assert_rejected(
        &pool,
        "INSERT INTO item (parent_id, kind, title) VALUES (2, 'episode', 'Child of episode')",
    )
    .await;
    assert_rejected(
        &pool,
        "INSERT INTO item (parent_id, kind, title) VALUES (1, 'movie', 'Movie with parent')",
    )
    .await;
}

#[tokio::test]
async fn one_default_ordering_per_series() {
    let (_dir, pool) = db().await;
    seed(&pool).await;
    exec(
        &pool,
        "INSERT INTO ordering (series_id, name, kind, source, is_default)
             VALUES (1, 'Aired', 'aired', 'tmdb', 1);
         INSERT INTO ordering (series_id, name, kind, source) VALUES (1, 'DVD', 'dvd', 'tmdb');",
    )
    .await
    .unwrap();
    assert_rejected(
        &pool,
        "INSERT INTO ordering (series_id, name, kind, source, is_default)
             VALUES (1, 'Absolute', 'absolute', 'tmdb', 1)",
    )
    .await;
    assert_rejected(
        &pool,
        "INSERT INTO ordering (series_id, name, kind, source) VALUES (2, 'On episode', 'aired', 'tmdb')",
    )
    .await;
}

#[tokio::test]
async fn ordering_entries_allow_split_and_merged_episodes() {
    let (_dir, pool) = db().await;
    seed(&pool).await;
    exec(
        &pool,
        "INSERT INTO ordering (id, series_id, name, kind, source, is_default)
             VALUES (1, 1, 'DVD', 'dvd', 'tvdb', 1);
         INSERT INTO ordering (id, series_id, name, kind, source) VALUES (2, 1, 'Other', 'custom', 'user');
         INSERT INTO ordering_season (id, ordering_id, number) VALUES (1, 1, 1);
         INSERT INTO ordering_season (id, ordering_id, number) VALUES (2, 2, 1);
         -- Item 2 is a double pilot: two entries in DVD order.
         INSERT INTO ordering_entry (ordering_id, position, item_id, ordering_season_id, episode_number)
             VALUES (1, 1, 2, 1, 1), (1, 2, 2, 1, 2), (1, 3, 3, 1, 3);",
    )
    .await
    .unwrap();
    // A season from a different ordering is rejected.
    assert_rejected(
        &pool,
        "INSERT INTO ordering_entry (ordering_id, position, item_id, ordering_season_id, episode_number)
             VALUES (1, 4, 3, 2, 4)",
    )
    .await;
}

#[tokio::test]
async fn only_primary_external_ids_are_unique() {
    let (_dir, pool) = db().await;
    seed(&pool).await;
    exec(
        &pool,
        "INSERT INTO external_id (item_id, provider, value, is_primary) VALUES (2, 'tmdb', '100', 1);
         -- A secondary ID may cover two items (a merged double episode).
         INSERT INTO external_id (item_id, provider, value) VALUES (2, 'tvdb', '900'), (3, 'tvdb', '900');",
    )
    .await
    .unwrap();
    assert_rejected(
        &pool,
        "INSERT INTO external_id (item_id, provider, value, is_primary) VALUES (3, 'tmdb', '100', 1)",
    )
    .await;
    // Unique per library: the same show in another library has the same ID.
    exec(
        &pool,
        "INSERT INTO library (id, name, kind) VALUES (2, 'Kids', 'tv');
         INSERT INTO item (id, kind, title, library_id) VALUES (10, 'series', 'Show', 2);
         INSERT INTO item (id, parent_id, kind, title) VALUES (11, 10, 'episode', 'Pilot');
         INSERT INTO external_id (item_id, provider, value, is_primary) VALUES (11, 'tmdb', '100', 1);",
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn items_and_files_stay_in_one_library() {
    let (_dir, pool) = db().await;
    seed(&pool).await;
    exec(
        &pool,
        "INSERT INTO library (id, name, kind) VALUES (2, 'Kids', 'tv');
         INSERT INTO library_root (id, library_id, path, layout_template)
             VALUES (2, 2, '/media/kids', '{series}/{filename}');
         INSERT INTO media_file (id, root_id, relative_path, size, mtime) VALUES (1, 2, 'a.mkv', 1, 1);",
    )
    .await
    .unwrap();
    assert_rejected(
        &pool,
        "INSERT INTO series_location (series_item_id, root_id, path) VALUES (1, 2, 'Show')",
    )
    .await;
    assert_rejected(
        &pool,
        "INSERT INTO file_link (file_id, item_id, confidence, source) VALUES (1, 2, 1, 'user')",
    )
    .await;
}

#[tokio::test]
async fn only_present_files_hold_their_path() {
    let (_dir, pool) = db().await;
    seed(&pool).await;
    let insert = "INSERT INTO media_file (root_id, relative_path, size, mtime)
                  VALUES (1, 'Show/Season 01/e01.mkv', 1, 1)";
    exec(&pool, insert).await.unwrap();
    assert_rejected(&pool, insert).await;
    exec(&pool, "UPDATE media_file SET status = 'missing'")
        .await
        .unwrap();
    exec(&pool, insert).await.unwrap();
    exec(
        &pool,
        "UPDATE media_file SET status = 'removed' WHERE status = 'present'",
    )
    .await
    .unwrap();
    exec(&pool, insert).await.unwrap();
}

#[tokio::test]
async fn linked_files_and_items_cannot_be_deleted() {
    let (_dir, pool) = db().await;
    seed(&pool).await;
    exec(
        &pool,
        "INSERT INTO media_file (id, root_id, relative_path, size, mtime) VALUES (1, 1, 'a.mkv', 1, 1);
         INSERT INTO file_link (file_id, item_id, confidence, source) VALUES (1, 2, 0.95, 'parser');",
    )
    .await
    .unwrap();
    assert_rejected(&pool, "DELETE FROM media_file WHERE id = 1").await;
    assert_rejected(&pool, "DELETE FROM item WHERE id = 2").await;
}

#[tokio::test]
async fn review_candidates_have_one_subject_and_one_target() {
    let (_dir, pool) = db().await;
    seed(&pool).await;
    exec(
        &pool,
        "INSERT INTO media_file (id, root_id, relative_path, size, mtime) VALUES (1, 1, 'a.mkv', 1, 1);
         INSERT INTO match_candidate (file_id, item_id, confidence, source, reason)
             VALUES (1, 2, 0.6, 'parser', 'S01E01 parsed, runtime off by 9 min');
         INSERT INTO match_candidate (root_id, folder_path, provider, provider_value, provider_title,
                                      confidence, source, reason)
             VALUES (1, 'Some Show', 'tmdb', '1399', 'Some Show', 0.7, 'search', 'folder name search');",
    )
    .await
    .unwrap();
    assert_rejected(
        &pool,
        "INSERT INTO match_candidate (file_id, root_id, folder_path, item_id, confidence, source, reason)
             VALUES (1, 1, 'x', 2, 0.5, 'parser', 'both subjects')",
    )
    .await;
    assert_rejected(
        &pool,
        "INSERT INTO match_candidate (file_id, item_id, provider, provider_value, confidence, source, reason)
             VALUES (1, 2, 'tmdb', '1', 0.5, 'parser', 'both targets')",
    )
    .await;
}

#[tokio::test]
async fn series_pref_is_ordering_or_playlist() {
    let (_dir, pool) = db().await;
    seed(&pool).await;
    exec(
        &pool,
        "INSERT INTO app_user (id, name) VALUES (1, 'jonas');
         INSERT INTO ordering (id, series_id, name, kind, source, is_default)
             VALUES (1, 1, 'Aired', 'aired', 'tmdb', 1);
         INSERT INTO playlist (id, name, kind, base_ordering_id, rules)
             VALUES (1, 'No filler', 'smart', 1, '{\"exclude_tags\": [\"filler\"]}');",
    )
    .await
    .unwrap();
    assert_rejected(
        &pool,
        "INSERT INTO user_series_pref (user_id, series_id) VALUES (1, 1)",
    )
    .await;
    assert_rejected(
        &pool,
        "INSERT INTO user_series_pref (user_id, series_id, ordering_id, playlist_id) VALUES (1, 1, 1, 1)",
    )
    .await;
    exec(
        &pool,
        "INSERT INTO user_series_pref (user_id, series_id, playlist_id) VALUES (1, 1, 1)",
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn event_ids_are_never_reused() {
    let (_dir, pool) = db().await;
    exec(
        &pool,
        "INSERT INTO event (type, payload) VALUES ('item.added', '{}'), ('item.added', '{}');
         DELETE FROM event;
         INSERT INTO event (type, payload) VALUES ('item.added', '{}');",
    )
    .await
    .unwrap();
    let id: i64 = sqlx::query_scalar("SELECT id FROM event")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(id, 3);
}
