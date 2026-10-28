//! Progress reports, played thresholds, play counts and marking played.

use cabinet_core::watch::{self, Scope, WatchError};
use sqlx::SqlitePool;
use tempfile::TempDir;

/// A series (1) with episodes 2–4 in a season of its ordering (episode 4 in
/// season 2), a movie (5) with a 100-second file, and a user (1).
async fn db() -> (TempDir, SqlitePool) {
    let dir = tempfile::tempdir().unwrap();
    let pool = cabinet_core::db::open(&dir.path().join("cabinet.db"))
        .await
        .unwrap();
    sqlx::raw_sql(
        "INSERT INTO app_user (id, name, is_admin) VALUES (1, 'jo', 1);
         INSERT INTO library (id, name, kind) VALUES (1, 'Films', 'movies'), (2, 'TV', 'tv');
         INSERT INTO item (id, kind, title, library_id) VALUES (1, 'series', 'Show', 2), (5, 'movie', 'Film', 1);
         INSERT INTO series (item_id) VALUES (1);
         INSERT INTO item (id, parent_id, library_id, kind, title) VALUES
             (2, 1, 2, 'episode', 'one'), (3, 1, 2, 'episode', 'two'), (4, 1, 2, 'episode', 'three');
         INSERT INTO ordering (id, series_id, name, kind, source, is_default) VALUES (1, 1, 'Aired', 'aired', 'tmdb', 1);
         INSERT INTO ordering_season (id, ordering_id, number) VALUES (1, 1, 1), (2, 1, 2);
         INSERT INTO ordering_entry (ordering_id, position, item_id, ordering_season_id, episode_number)
             VALUES (1, 1, 2, 1, 1), (1, 2, 3, 1, 2), (1, 3, 4, 2, 1);
         INSERT INTO library_root (id, library_id, path, layout_template) VALUES (1, 1, '/m', '{movie}/{filename}');
         INSERT INTO media_file (id, root_id, library_id, relative_path, size, mtime, duration_seconds)
             VALUES (1, 1, 1, 'Film/film.mkv', 1, 1, 100);
         INSERT INTO file_link (file_id, item_id, library_id, confidence, source) VALUES (1, 5, 1, 1, 'user');",
    )
    .execute(&pool)
    .await
    .unwrap();
    (dir, pool)
}

#[tokio::test]
async fn progress_resumes_and_finishes_once() {
    let (_dir, pool) = db().await;
    let w = watch::report_progress(&pool, 1, 5, 10.0, None, 100)
        .await
        .unwrap();
    assert_eq!(
        (w.position_seconds, w.last_played_at),
        (0.0, Some(100)),
        "too early to resume"
    );
    let w = watch::report_progress(&pool, 1, 5, 45.0, None, 101)
        .await
        .unwrap();
    assert_eq!((w.position_seconds, w.played), (45.0, false));

    // 90 of the file's 100 seconds.
    let w = watch::report_progress(&pool, 1, 5, 90.0, None, 102)
        .await
        .unwrap();
    assert_eq!((w.position_seconds, w.played, w.play_count), (0.0, true, 1));
    let w = watch::report_progress(&pool, 1, 5, 98.0, None, 103)
        .await
        .unwrap();
    assert_eq!(w.play_count, 1, "the credits don't count again");

    // Watching again counts again.
    watch::report_progress(&pool, 1, 5, 40.0, None, 200)
        .await
        .unwrap();
    let w = watch::report_progress(&pool, 1, 5, 95.0, None, 201)
        .await
        .unwrap();
    assert_eq!((w.play_count, w.played), (2, true));

    // The player's duration wins; without one, an episode never finishes.
    let w = watch::report_progress(&pool, 1, 2, 500.0, Some(1000.0), 300)
        .await
        .unwrap();
    assert!(!w.played);
    let w = watch::report_progress(&pool, 1, 2, 5000.0, None, 300)
        .await
        .unwrap();
    assert!(!w.played);

    assert!(matches!(
        watch::report_progress(&pool, 1, 1, 50.0, None, 1).await,
        Err(WatchError::Invalid(_))
    ));
    assert!(
        watch::report_progress(&pool, 1, 5, -1.0, None, 1)
            .await
            .is_err()
    );
    assert!(matches!(
        watch::report_progress(&pool, 1, 99, 1.0, None, 1).await,
        Err(WatchError::NotFound)
    ));
}

#[tokio::test]
async fn marking_played() {
    let (_dir, pool) = db().await;
    watch::report_progress(&pool, 1, 2, 300.0, Some(1000.0), 50)
        .await
        .unwrap();
    assert_eq!(
        watch::set_played(&pool, 1, 1, Scope::Season(1), true, 100)
            .await
            .unwrap(),
        2,
        "season 1"
    );
    assert!(
        !watch::get(&pool, 1, 4).await.unwrap().played,
        "season 2 untouched"
    );
    let w = watch::get(&pool, 1, 2).await.unwrap();
    assert_eq!(
        (w.played, w.position_seconds, w.play_count, w.last_played_at),
        (true, 0.0, 1, Some(50))
    );

    assert_eq!(
        watch::set_played(&pool, 1, 1, Scope::All, true, 100)
            .await
            .unwrap(),
        1,
        "only the rest"
    );
    assert_eq!(
        watch::set_played(&pool, 1, 1, Scope::All, false, 100)
            .await
            .unwrap(),
        3
    );
    let w = watch::get(&pool, 1, 3).await.unwrap();
    assert_eq!((w.played, w.play_count), (false, 1), "the count is history");

    assert!(
        watch::set_played(&pool, 1, 1, Scope::Season(99), true, 1)
            .await
            .is_err()
    );
    assert!(
        watch::set_played(&pool, 1, 5, Scope::Season(1), true, 1)
            .await
            .is_err()
    );
    assert_eq!(
        watch::set_played(&pool, 1, 5, Scope::All, true, 1)
            .await
            .unwrap(),
        1
    );
}
