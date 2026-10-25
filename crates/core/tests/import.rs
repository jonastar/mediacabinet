//! Importing and refreshing provider metadata.

use cabinet_core::metadata::fake::Catalog;
use cabinet_core::metadata::import::{import_movie, import_series, refresh_series};
use cabinet_core::metadata::{EpisodeData, SeriesData};
use sqlx::SqlitePool;
use tempfile::TempDir;

fn catalog() -> Catalog {
    Catalog::from_json(include_str!("data/catalog.json")).unwrap()
}

fn night_harbor() -> SeriesData {
    catalog().series("99010").unwrap().clone()
}

async fn db() -> (TempDir, SqlitePool) {
    let dir = tempfile::tempdir().unwrap();
    let pool = cabinet_core::db::open(&dir.path().join("cabinet.db"))
        .await
        .unwrap();
    (dir, pool)
}

async fn count(pool: &SqlitePool, sql: &'static str) -> i64 {
    sqlx::query_scalar(sql).fetch_one(pool).await.unwrap()
}

async fn episode_id(pool: &SqlitePool, title: &str) -> i64 {
    sqlx::query_scalar("SELECT id FROM item WHERE kind = 'episode' AND title = ?")
        .bind(title)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn imports_items_orderings_and_ids() {
    let (_dir, pool) = db().await;
    let report = import_series(&pool, &night_harbor()).await.unwrap();
    assert!(report.created);
    assert_eq!(report.episodes_added, 13);

    assert_eq!(
        count(&pool, "SELECT count(*) FROM item WHERE kind = 'episode'").await,
        13
    );
    let orderings: Vec<(String, String, String, bool)> =
        sqlx::query_as("SELECT name, kind, source, is_default FROM ordering ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(
        orderings,
        [
            ("Aired".into(), "aired".into(), "tmdb".into(), true),
            (
                "Absolute".into(),
                "absolute".into(),
                "computed".into(),
                false
            ),
        ]
    );
    // Specials sort after the regular seasons; absolute order leaves them out.
    let last: (i64, String) = sqlx::query_as(
        "SELECT os.number, i.title FROM ordering_entry oe
         JOIN ordering o ON o.id = oe.ordering_id AND o.kind = 'aired'
         JOIN ordering_season os ON os.id = oe.ordering_season_id
         JOIN item i ON i.id = oe.item_id ORDER BY oe.position DESC LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(last, (0, "Behind the Scenes".into()));
    assert_eq!(
        count(&pool, "SELECT count(*) FROM ordering_entry oe JOIN ordering o ON o.id = oe.ordering_id WHERE o.kind = 'absolute'").await,
        12
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM external_id WHERE provider = 'tvdb'"
        )
        .await,
        1
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM item_tag WHERE tag LIKE 'genre:%'"
        )
        .await,
        2
    );
}

#[tokio::test]
async fn reimporting_unchanged_data_changes_nothing() {
    let (_dir, pool) = db().await;
    let first = import_series(&pool, &night_harbor()).await.unwrap();
    let ordering: i64 = count(&pool, "SELECT id FROM ordering WHERE kind = 'aired'").await;

    let again = import_series(&pool, &night_harbor()).await.unwrap();
    assert_eq!(again.item_id, first.item_id);
    assert!(!again.created);
    assert_eq!(
        (
            again.episodes_added,
            again.episodes_updated,
            again.episodes_removed
        ),
        (0, 0, 0)
    );
    assert_eq!(
        count(&pool, "SELECT id FROM ordering WHERE kind = 'aired'").await,
        ordering,
        "ordering IDs are kept"
    );
}

#[tokio::test]
async fn changed_episode_ids_are_reconciled_not_duplicated() {
    let (_dir, pool) = db().await;
    import_series(&pool, &night_harbor()).await.unwrap();
    let pilot = episode_id(&pool, "Pilot").await;

    let mut data = night_harbor();
    for e in &mut data.episodes {
        e.id = format!("new-{}", e.id);
    }
    let report = import_series(&pool, &data).await.unwrap();

    assert_eq!(report.episodes_added, 0);
    assert_eq!(
        count(&pool, "SELECT count(*) FROM item WHERE kind = 'episode'").await,
        13
    );
    assert_eq!(episode_id(&pool, "Pilot").await, pilot);
    let value: String =
        sqlx::query_scalar("SELECT value FROM external_id WHERE item_id = ? AND is_primary")
            .bind(pilot)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        value, "new-990100101",
        "Night Harbor S01E01 under its new ID"
    );
}

#[tokio::test]
async fn dropped_episodes_are_kept_only_with_history() {
    let (_dir, pool) = db().await;
    let series = import_series(&pool, &night_harbor()).await.unwrap().item_id;
    let pilot = episode_id(&pool, "Pilot").await;
    sqlx::query("INSERT INTO app_user (id, name) VALUES (1, 'u')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO user_item_state (user_id, item_id, played) VALUES (1, ?, 1)")
        .bind(pilot)
        .execute(&pool)
        .await
        .unwrap();

    let mut data = night_harbor();
    data.episodes
        .retain(|e| e.title != "Pilot" && e.title != "The Tide");
    let report = refresh_series(&pool, series, &data).await.unwrap();

    assert_eq!((report.episodes_flagged, report.episodes_removed), (1, 1));
    let flagged: Option<i64> =
        sqlx::query_scalar("SELECT removed_upstream_at FROM item WHERE id = ?")
            .bind(pilot)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(flagged.is_some());
    assert_eq!(
        count(&pool, "SELECT count(*) FROM item WHERE title = 'The Tide'").await,
        0
    );

    // It comes back upstream: the flag clears.
    refresh_series(&pool, series, &night_harbor())
        .await
        .unwrap();
    let flagged: Option<i64> =
        sqlx::query_scalar("SELECT removed_upstream_at FROM item WHERE id = ?")
            .bind(pilot)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(flagged, None);
}

#[tokio::test]
async fn locked_fields_survive_refresh() {
    let (_dir, pool) = db().await;
    let series = import_series(&pool, &night_harbor()).await.unwrap().item_id;
    sqlx::query("UPDATE item SET title = 'Night Harbour (my spelling)', metadata_locked = '[\"title\"]' WHERE id = ?")
        .bind(series)
        .execute(&pool)
        .await
        .unwrap();

    let mut data = night_harbor();
    data.overview = Some("A new overview.".into());
    refresh_series(&pool, series, &data).await.unwrap();

    let (title, overview): (String, String) =
        sqlx::query_as("SELECT title, overview FROM item WHERE id = ?")
            .bind(series)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(title, "Night Harbour (my spelling)");
    assert_eq!(overview, "A new overview.");
}

#[tokio::test]
async fn new_episodes_are_added_with_an_event() {
    let (_dir, pool) = db().await;
    import_series(&pool, &night_harbor()).await.unwrap();
    let mut data = night_harbor();
    data.episodes.push(EpisodeData {
        id: "999".into(),
        title: "Return".into(),
        overview: None,
        airs: None,
        airs_tz: None,
        runtime_minutes: Some(45),
        season: 3,
        number: 1,
    });

    let report = import_series(&pool, &data).await.unwrap();

    assert_eq!(report.episodes_added, 1);
    let events: Vec<String> = sqlx::query_scalar("SELECT type FROM event ORDER BY id DESC LIMIT 2")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(events, ["series.refreshed", "item.added"]);
}

#[tokio::test]
async fn imports_movies() {
    let (_dir, pool) = db().await;
    let data = catalog().movie("99020").unwrap().clone();
    let report = import_movie(&pool, &data).await.unwrap();
    assert!(report.created);
    let again = import_movie(&pool, &data).await.unwrap();
    assert_eq!(again.item_id, report.item_id);
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM external_id WHERE provider = 'imdb'"
        )
        .await,
        1
    );
}
