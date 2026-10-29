//! The activity log: audiences, library additions from the outbox, play sessions.

use cabinet_core::activity::{self, ActivityFilter, NewEntry, Viewer, library, sessions};
use cabinet_core::{events, watch};
use serde_json::json;
use sqlx::SqlitePool;
use tempfile::TempDir;

/// An admin (1) and two users (2, 3); a series (1) with episodes 2–4 and a movie
/// (5) with a 6000-second file.
async fn db() -> (TempDir, SqlitePool) {
    let dir = tempfile::tempdir().unwrap();
    let pool = cabinet_core::db::open(&dir.path().join("cabinet.db"))
        .await
        .unwrap();
    sqlx::raw_sql(
        "INSERT INTO app_user (id, name, is_admin) VALUES (1, 'admin', 1), (2, 'kim', 0), (3, 'lee', 0);
         INSERT INTO library (id, name, kind) VALUES (1, 'Films', 'movies'), (2, 'TV', 'tv');
         INSERT INTO item (id, kind, title, library_id) VALUES (1, 'series', 'Show', 2), (5, 'movie', 'Film', 1);
         INSERT INTO series (item_id) VALUES (1);
         INSERT INTO item (id, parent_id, library_id, kind, title) VALUES
             (2, 1, 2, 'episode', 'one'), (3, 1, 2, 'episode', 'two'), (4, 1, 2, 'episode', 'three');
         INSERT INTO library_root (id, library_id, path, layout_template) VALUES (1, 1, '/m', '{movie}/{filename}');
         INSERT INTO media_file (id, root_id, library_id, relative_path, size, mtime, duration_seconds)
             VALUES (1, 1, 1, 'Film/film.mkv', 1, 1, 6000);
         INSERT INTO file_link (file_id, item_id, library_id, confidence, source) VALUES (1, 5, 1, 1, 'user');",
    )
    .execute(&pool)
    .await
    .unwrap();
    (dir, pool)
}

fn viewer(user: i64) -> Viewer {
    Viewer {
        user,
        admin: user == 1,
    }
}

/// The kinds the user sees, newest first.
async fn seen(pool: &SqlitePool, user: i64) -> Vec<String> {
    activity::list(pool, viewer(user), &ActivityFilter::default())
        .await
        .unwrap()
        .entries
        .into_iter()
        .map(|e| e.kind)
        .collect()
}

async fn write(pool: &SqlitePool, entry: NewEntry) {
    let mut tx = pool.begin().await.unwrap();
    activity::record(&mut tx, &entry).await.unwrap();
    tx.commit().await.unwrap();
}

#[tokio::test]
async fn each_viewer_sees_their_audience() {
    let (_dir, pool) = db().await;
    write(
        &pool,
        NewEntry::admins(activity::LOGIN_FAILED, 10, None, json!({})),
    )
    .await;
    let mut about_kim = NewEntry::admins(activity::WATCHED, 20, Some(2), json!({}));
    about_kim.subject = Some(2);
    about_kim.audience = activity::Audience::Subject;
    write(&pool, about_kim.clone()).await;
    about_kim.at = 30;
    about_kim.kind = activity::STOPPED_WATCHING;
    about_kim.audience = activity::Audience::Everyone;
    write(&pool, about_kim).await;

    assert_eq!(
        seen(&pool, 1).await,
        ["stopped_watching", "watched", "login_failed"]
    );
    assert_eq!(seen(&pool, 2).await, ["stopped_watching", "watched"]);
    assert_eq!(seen(&pool, 3).await, ["stopped_watching"]);
}

#[tokio::test]
async fn pages_follow_the_cursor() {
    let (_dir, pool) = db().await;
    for at in [5, 5, 6, 7] {
        write(
            &pool,
            NewEntry::admins(activity::LOGIN, at, None, json!({})),
        )
        .await;
    }
    let mut filter = ActivityFilter {
        limit: Some(3),
        ..Default::default()
    };
    let first = activity::list(&pool, viewer(1), &filter).await.unwrap();
    assert_eq!(first.entries.len(), 3);
    filter.before = first.next.clone();
    let second = activity::list(&pool, viewer(1), &filter).await.unwrap();
    assert_eq!(second.entries.len(), 1);
    assert_eq!(second.next, None);
    let ids: Vec<i64> = first
        .entries
        .iter()
        .chain(&second.entries)
        .map(|e| e.id)
        .collect();
    assert_eq!(ids, [4, 3, 2, 1]);
}

async fn link(pool: &SqlitePool, item: i64, file: i64, at: i64) {
    let mut tx = pool.begin().await.unwrap();
    let id = events::emit(
        &mut tx,
        events::LINK_CREATED,
        json!({ "file_id": file, "item_id": item }),
    )
    .await
    .unwrap();
    sqlx::query("UPDATE event SET created_at = ? WHERE id = ?")
        .bind(at)
        .bind(id)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
}

async fn additions(pool: &SqlitePool) -> Vec<(Option<i64>, serde_json::Value)> {
    activity::list(pool, viewer(3), &ActivityFilter::default())
        .await
        .unwrap()
        .entries
        .into_iter()
        .filter(|e| e.kind == activity::ITEMS_ADDED)
        .map(|e| (e.item.map(|i| i.id), e.payload))
        .collect()
}

#[tokio::test]
async fn additions_are_grouped_per_series_and_only_first_links_count() {
    let (_dir, pool) = db().await;
    link(&pool, 2, 10, 1000).await;
    link(&pool, 3, 11, 1000).await;
    link(&pool, 5, 1, 1000).await;
    library::sync(&pool).await.unwrap();
    assert_eq!(
        additions(&pool).await,
        [
            (Some(5), json!({ "items": [5], "new_series": false })),
            (Some(1), json!({ "items": [2, 3], "new_series": true })),
        ]
    );

    // A second file for an episode isn't news; a new one soon after joins the entry.
    link(&pool, 2, 12, 1100).await;
    link(&pool, 4, 13, 1200).await;
    library::sync(&pool).await.unwrap();
    let after = additions(&pool).await;
    assert_eq!(after.len(), 2);
    assert_eq!(
        after[0],
        (Some(1), json!({ "items": [2, 3, 4], "new_series": true }))
    );

    // Episode filter: the series' entry that includes it.
    let for_episode = activity::list(
        &pool,
        viewer(3),
        &ActivityFilter {
            item_id: Some(4),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(for_episode.entries.len(), 1);
}

#[tokio::test]
async fn later_additions_get_their_own_entry() {
    let (_dir, pool) = db().await;
    link(&pool, 2, 10, 1000).await;
    library::sync(&pool).await.unwrap();
    link(&pool, 3, 11, 1000 + library::GROUP_WINDOW + 1).await;
    library::sync(&pool).await.unwrap();
    assert_eq!(
        additions(&pool).await,
        [
            (Some(1), json!({ "items": [3], "new_series": false })),
            (Some(1), json!({ "items": [2], "new_series": true })),
        ]
    );
}

async fn play(pool: &SqlitePool, user: i64, from: f64, to: f64, start: i64) {
    let mut position = from;
    let mut now = start;
    while position <= to {
        watch::report_progress(pool, user, 5, position, None, now)
            .await
            .unwrap();
        position += 60.0;
        now += 60;
    }
}

#[tokio::test]
async fn short_sessions_leave_no_trace() {
    let (_dir, pool) = db().await;
    play(&pool, 2, 0.0, 120.0, 1000).await;
    sessions::end_quiet(&pool, 1000 + 120 + sessions::QUIET_AFTER + 1)
        .await
        .unwrap();
    assert_eq!(seen(&pool, 1).await, Vec::<String>::new());
}

#[tokio::test]
async fn stopping_partway_and_finishing() {
    let (_dir, pool) = db().await;
    play(&pool, 2, 0.0, 600.0, 1000).await;
    // Not ended yet: still playing as far as anyone knows.
    assert_eq!(seen(&pool, 3).await, Vec::<String>::new());
    sessions::end_quiet(&pool, 1600 + sessions::QUIET_AFTER + 1)
        .await
        .unwrap();
    assert_eq!(seen(&pool, 3).await, ["stopped_watching"]);

    // The next day: picks up, finishes (90% of 6000 s) and watches the credits.
    play(&pool, 2, 600.0, 5700.0, 100_000).await;
    let page = activity::list(&pool, viewer(3), &ActivityFilter::default())
        .await
        .unwrap();
    let kinds: Vec<&str> = page.entries.iter().map(|e| e.kind.as_str()).collect();
    assert_eq!(kinds, ["watched", "stopped_watching"]);
    let watched = &page.entries[0];
    assert_eq!(watched.payload["position_start"], json!(600.0));
    assert_eq!(watched.payload["duration_seconds"], json!(6000.0));
    assert_eq!(watched.actor.as_ref().unwrap().name, "kim");
    assert_eq!(watched.item.as_ref().unwrap().title, "Film");
}

#[tokio::test]
async fn incognito_hides_from_others_but_not_admins_and_sticks_to_the_session() {
    let (_dir, pool) = db().await;
    cabinet_core::accounts::set_incognito(&pool, 2, true)
        .await
        .unwrap();
    play(&pool, 2, 0.0, 600.0, 1000).await;
    // Turning it off before the session ends doesn't expose it.
    cabinet_core::accounts::set_incognito(&pool, 2, false)
        .await
        .unwrap();
    sessions::end_quiet(&pool, 1_000_000).await.unwrap();
    assert_eq!(seen(&pool, 3).await, Vec::<String>::new());
    assert_eq!(seen(&pool, 2).await, ["stopped_watching"]);
    assert_eq!(seen(&pool, 1).await, ["stopped_watching"]);
}

#[tokio::test]
async fn clearing_and_removing_users() {
    let (_dir, pool) = db().await;
    play(&pool, 2, 0.0, 600.0, 1000).await;
    play(&pool, 3, 0.0, 600.0, 1000).await;
    sessions::end_quiet(&pool, 1_000_000).await.unwrap();
    write(
        &pool,
        NewEntry::admins(activity::USER_CREATED, 50, Some(2), json!({})),
    )
    .await;

    assert_eq!(activity::clear_watch(&pool, 2).await.unwrap(), 1);
    assert_eq!(seen(&pool, 1).await, ["stopped_watching", "user_created"]);

    // What's about lee goes with lee; kim acted in what stays.
    sqlx::query("DELETE FROM app_user WHERE id = 3")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM app_user WHERE id = 2")
        .execute(&pool)
        .await
        .unwrap();
    let page = activity::list(&pool, viewer(1), &ActivityFilter::default())
        .await
        .unwrap();
    assert_eq!(page.entries.len(), 1);
    assert!(page.entries[0].actor.is_none());
}

#[tokio::test]
async fn pruning_keeps_additions_longer_than_watching() {
    let (_dir, pool) = db().await;
    let day = 86400;
    let mut watched = NewEntry::admins(activity::WATCHED, 0, None, json!({}));
    watched.subject = Some(2);
    write(&pool, watched).await;
    write(
        &pool,
        NewEntry::admins(activity::CONFIG_CHANGED, 0, None, json!({})),
    )
    .await;
    assert_eq!(activity::prune(&pool, 100 * day).await.unwrap(), 1);
    assert_eq!(activity::prune(&pool, 400 * day).await.unwrap(), 1);
}
