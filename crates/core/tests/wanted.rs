//! The derived views (availability, monitoring, wanted), wanted events, the
//! wanted list, the calendar and custom status.

use cabinet_core::catalog;
use cabinet_core::wanted::{self, StatusUpdate, WantedFilter};
use sqlx::SqlitePool;
use tempfile::TempDir;

async fn db() -> (TempDir, SqlitePool) {
    let dir = tempfile::tempdir().unwrap();
    let pool = cabinet_core::db::open(&dir.path().join("cabinet.db"))
        .await
        .unwrap();
    (dir, pool)
}

async fn exec(pool: &SqlitePool, sql: &'static str) {
    sqlx::raw_sql(sql).execute(pool).await.unwrap();
}

async fn state(pool: &SqlitePool, item: i64) -> (String, bool, Option<String>) {
    sqlx::query_as(
        "SELECT st.availability, st.monitored, w.reason FROM item_state st
         LEFT JOIN item_wanted w ON w.item_id = st.item_id WHERE st.item_id = ?",
    )
    .bind(item)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// A series (1) with episodes 2–7, one root, and files for some of them.
async fn library(pool: &SqlitePool) {
    exec(
        pool,
        "INSERT INTO library (id, name, kind) VALUES (1, 'TV', 'tv');
         INSERT INTO library_root (id, library_id, path, layout_template) VALUES (1, 1, '/tv', '{series}/{filename}');
         INSERT INTO item (id, kind, title, library_id) VALUES (1, 'series', 'Show', 1);
         INSERT INTO series (item_id) VALUES (1);
         INSERT INTO item (id, parent_id, library_id, kind, title, airs_at) VALUES
             (2, 1, 1, 'episode', 'present', 1000),
             (3, 1, 1, 'episode', 'missing', 1000),
             (4, 1, 1, 'episode', 'never had a file', 1000),
             (5, 1, 1, 'episode', 'airs next year', unixepoch() + 365 * 86400),
             (6, 1, 1, 'episode', 'no date', NULL),
             (7, 1, 1, 'episode', 'removed file', 1000);
         INSERT INTO media_file (id, root_id, library_id, relative_path, size, mtime, status) VALUES
             (1, 1, 1, 'Show/a.mkv', 1, 1, 'present'),
             (2, 1, 1, 'Show/b.mkv', 1, 1, 'missing'),
             (3, 1, 1, 'Show/c.mkv', 1, 1, 'removed');
         INSERT INTO file_link (file_id, item_id, library_id, confidence, source)
             VALUES (1, 2, 1, 1, 'user'), (2, 3, 1, 1, 'user'), (3, 7, 1, 1, 'user');
         INSERT INTO media_stream (file_id, stream_index, kind, height) VALUES (1, 0, 'video', 720);",
    )
    .await;
}

#[tokio::test]
async fn availability_follows_the_spec() {
    let (_dir, pool) = db().await;
    library(&pool).await;
    let expected = [
        (2, "available"),
        (3, "missing"),
        (4, "not_acquired"),
        (5, "upcoming"),
        (6, "unknown"),
        // Deleted on purpose through the app: not acquired, not missing.
        (7, "not_acquired"),
    ];
    for (item, availability) in expected {
        assert_eq!(state(&pool, item).await.0, availability, "item {item}");
    }
}

#[tokio::test]
async fn monitoring_inherits_and_wanted_needs_it() {
    let (_dir, pool) = db().await;
    library(&pool).await;
    assert_eq!(
        state(&pool, 4).await,
        ("not_acquired".into(), false, None),
        "unmonitored by default"
    );

    exec(
        &pool,
        "INSERT INTO monitor (item_id, monitored) VALUES (1, 1), (3, 0)",
    )
    .await;
    assert_eq!(
        state(&pool, 4).await.2.as_deref(),
        Some("not_acquired"),
        "inherits the series'"
    );
    assert_eq!(
        state(&pool, 3).await,
        ("missing".into(), false, None),
        "its own setting wins"
    );
    assert_eq!(state(&pool, 5).await.2, None, "upcoming isn't wanted yet");
    assert_eq!(state(&pool, 6).await.2, None, "nor is an unknown date");
    assert_eq!(
        state(&pool, 2).await.2,
        None,
        "no profile: any file will do"
    );
}

#[tokio::test]
async fn files_below_the_cutoff_are_wanted_as_upgrades() {
    let (_dir, pool) = db().await;
    library(&pool).await;
    exec(
        &pool,
        "INSERT INTO monitor (item_id, monitored, quality_profile_id)
         VALUES (1, 1, (SELECT id FROM quality_profile WHERE name = 'HD 1080p'))",
    )
    .await;
    assert_eq!(
        state(&pool, 2).await.2.as_deref(),
        Some("upgrade"),
        "720p under a 1080p cutoff"
    );

    // A scope film at 1920x800 counts as 1080p.
    exec(
        &pool,
        "UPDATE media_stream SET height = 800 WHERE file_id = 1",
    )
    .await;
    assert_eq!(state(&pool, 2).await.2, None);

    exec(&pool, "UPDATE monitor SET quality_profile_id = (SELECT id FROM quality_profile WHERE name = 'UHD 2160p')").await;
    assert_eq!(state(&pool, 2).await.2.as_deref(), Some("upgrade"));
}

#[tokio::test]
async fn a_wanted_playlist_limits_wanted_episodes() {
    let dir = tempfile::tempdir().unwrap();
    let pool = cabinet_core::db::open(&dir.path().join("cabinet.db"))
        .await
        .unwrap();
    cabinet_core::demo::seed(&pool, &dir.path().join("media"), catalog::now())
        .await
        .unwrap();
    let kitsune: i64 = sqlx::query_scalar("SELECT id FROM item WHERE title = 'Kitsune Road'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let wanted = |pool: SqlitePool| async move {
        let filter = WantedFilter {
            series_id: Some(kitsune),
            ..Default::default()
        };
        let page = wanted::list(&pool, &filter, catalog::now()).await.unwrap();
        page.items.into_iter().map(|i| i.title).collect::<Vec<_>>()
    };
    assert_eq!(
        wanted(pool.clone()).await,
        [
            "Market Day",
            "The Mountain Road",
            "Bathhouse Detour",
            "Paper Charms"
        ]
    );

    let no_filler: i64 = sqlx::query_scalar("SELECT id FROM playlist WHERE name = 'No filler'")
        .fetch_one(&pool)
        .await
        .unwrap();
    wanted::set_monitoring(&pool, kitsune, None, Some(Some(no_filler)))
        .await
        .unwrap();
    assert_eq!(
        wanted(pool.clone()).await,
        ["The Mountain Road", "Paper Charms"],
        "fillers aren't wanted"
    );
    let unwanted: i64 =
        sqlx::query_scalar("SELECT count(*) FROM event WHERE type = 'item.unwanted'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(unwanted, 2);
}

#[tokio::test]
async fn events_follow_changes_including_time() {
    let (_dir, pool) = db().await;
    library(&pool).await;
    exec(
        &pool,
        "INSERT INTO monitor (item_id, monitored) VALUES (1, 1)",
    )
    .await;
    assert_eq!(wanted::sync(&pool).await.unwrap(), (3, 0), "3, 4 and 7");
    assert_eq!(
        wanted::sync(&pool).await.unwrap(),
        (0, 0),
        "nothing changed"
    );

    // The upcoming episode airs.
    exec(&pool, "UPDATE item SET airs_at = 1000 WHERE id = 5").await;
    assert_eq!(wanted::sync(&pool).await.unwrap(), (1, 0));
    // A file arrives for one wanted episode.
    exec(
        &pool,
        "INSERT INTO file_link (file_id, item_id, library_id, confidence, source) VALUES (1, 4, 1, 1, 'user')",
    )
    .await;
    assert_eq!(wanted::sync(&pool).await.unwrap(), (0, 1));
    let kinds: Vec<String> = sqlx::query_scalar("SELECT type FROM event ORDER BY id")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(kinds.iter().filter(|k| *k == "item.wanted").count(), 4);
    assert_eq!(kinds.iter().filter(|k| *k == "item.unwanted").count(), 1);
}

#[tokio::test]
async fn the_wanted_list_filters_and_pages() {
    let (_dir, pool) = db().await;
    library(&pool).await;
    exec(
        &pool,
        "INSERT INTO monitor (item_id, monitored) VALUES (1, 1)",
    )
    .await;
    let now = catalog::now();
    let all = wanted::list(&pool, &WantedFilter::default(), now)
        .await
        .unwrap();
    assert_eq!(all.total, 3);
    let missing = WantedFilter {
        reason: Some("missing".into()),
        ..Default::default()
    };
    assert_eq!(wanted::list(&pool, &missing, now).await.unwrap().total, 1);
    let page = WantedFilter {
        limit: Some(2),
        offset: Some(2),
        ..Default::default()
    };
    let page = wanted::list(&pool, &page, now).await.unwrap();
    assert_eq!((page.items.len(), page.total), (1, 3));
    // Episodes belong to their series' library, file or no file.
    let library = |id| WantedFilter {
        library_id: Some(id),
        ..Default::default()
    };
    assert_eq!(
        wanted::list(&pool, &library(1), now).await.unwrap().total,
        3
    );
    assert_eq!(
        wanted::list(&pool, &library(2), now).await.unwrap().total,
        0
    );
    let bad = WantedFilter {
        reason: Some("bogus".into()),
        ..Default::default()
    };
    assert!(wanted::list(&pool, &bad, now).await.is_err());
}

#[tokio::test]
async fn calendar_and_status() {
    let (_dir, pool) = db().await;
    library(&pool).await;
    let now = catalog::now();
    let year = wanted::calendar(&pool, now, now + 366 * 86400)
        .await
        .unwrap();
    assert_eq!(
        year.iter().map(|e| e.title.as_str()).collect::<Vec<_>>(),
        ["airs next year"]
    );
    assert!(wanted::calendar(&pool, now, now - 1).await.is_err());

    let update = StatusUpdate {
        status: "downloading".into(),
        detail: Some(serde_json::json!({ "progress": 0.4 })),
        expires_in_seconds: Some(60),
    };
    wanted::set_status(&pool, 4, "sonarr", &update)
        .await
        .unwrap();
    exec(
        &pool,
        "INSERT INTO monitor (item_id, monitored) VALUES (1, 1)",
    )
    .await;
    let list = wanted::list(&pool, &WantedFilter::default(), now)
        .await
        .unwrap();
    let item = list.items.iter().find(|i| i.item_id == 4).unwrap();
    assert_eq!(
        (item.statuses[0].status.as_str(), item.statuses[0].stale),
        ("downloading", false)
    );
    let later = wanted::list(&pool, &WantedFilter::default(), now + 120)
        .await
        .unwrap();
    assert!(
        later
            .items
            .iter()
            .find(|i| i.item_id == 4)
            .unwrap()
            .statuses[0]
            .stale
    );

    wanted::clear_status(&pool, 4, "sonarr").await.unwrap();
    assert!(wanted::clear_status(&pool, 4, "sonarr").await.is_err());
    let events: i64 =
        sqlx::query_scalar("SELECT count(*) FROM event WHERE type = 'status.changed'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(events, 2);
}
