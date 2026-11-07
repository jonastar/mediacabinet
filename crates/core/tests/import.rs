//! Importing and refreshing provider metadata.

use cabinet_core::catalog::{self, EpisodeList};
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
    sqlx::raw_sql(
        "INSERT INTO library (id, name, kind) VALUES (1, 'TV Shows', 'tv'), (2, 'Movies', 'movies')",
    )
    .execute(&pool)
    .await
    .unwrap();
    (dir, pool)
}

const TV: i64 = 1;
const MOVIES: i64 = 2;

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
    let report = import_series(&pool, TV, &night_harbor()).await.unwrap();
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
    let first = import_series(&pool, TV, &night_harbor()).await.unwrap();
    let ordering: i64 = count(&pool, "SELECT id FROM ordering WHERE kind = 'aired'").await;

    let again = import_series(&pool, TV, &night_harbor()).await.unwrap();
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
    import_series(&pool, TV, &night_harbor()).await.unwrap();
    let pilot = episode_id(&pool, "Pilot").await;

    let mut data = night_harbor();
    for e in &mut data.episodes {
        e.id = format!("new-{}", e.id);
    }
    let report = import_series(&pool, TV, &data).await.unwrap();

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
    let series = import_series(&pool, TV, &night_harbor())
        .await
        .unwrap()
        .item_id;
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
    let series = import_series(&pool, TV, &night_harbor())
        .await
        .unwrap()
        .item_id;
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
    import_series(&pool, TV, &night_harbor()).await.unwrap();
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
        credits: Default::default(),
        rating: None,
        still: None,
    });

    let report = import_series(&pool, TV, &data).await.unwrap();

    assert_eq!(report.episodes_added, 1);
    let events: Vec<String> = sqlx::query_scalar("SELECT type FROM event ORDER BY id DESC LIMIT 2")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(events, ["series.refreshed", "item.added"]);
}

#[tokio::test]
async fn an_episode_lists_its_still_for_as_long_as_the_provider_has_it() {
    let (_dir, pool) = db().await;
    let mut data = night_harbor();
    data.episodes[0].still = Some("/still.jpg".into());
    let series = import_series(&pool, TV, &data).await.unwrap().item_id;

    let artwork = |list: EpisodeList| -> Vec<Vec<String>> {
        list.entries
            .into_iter()
            .map(|e| e.episode.artwork)
            .filter(|kinds| !kinds.is_empty())
            .collect()
    };
    let now = catalog::now();
    let list = catalog::episodes(&pool, series, None, None, now)
        .await
        .unwrap();
    assert_eq!(artwork(list), [["still"]]);

    data.episodes[0].still = None;
    import_series(&pool, TV, &data).await.unwrap();
    let list = catalog::episodes(&pool, series, None, None, now)
        .await
        .unwrap();
    assert!(artwork(list).is_empty());
}

#[tokio::test]
async fn imports_movies() {
    let (_dir, pool) = db().await;
    let data = catalog().movie("99020").unwrap().clone();
    let report = import_movie(&pool, MOVIES, &data).await.unwrap();
    assert!(report.created);
    let again = import_movie(&pool, MOVIES, &data).await.unwrap();
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

#[tokio::test]
async fn refresh_schedule_and_failures() {
    use cabinet_core::metadata::Provider;
    use cabinet_core::metadata::refresh::{due, refresh_item};
    use std::sync::Arc;

    let (_dir, pool) = db().await;
    let series = import_series(&pool, TV, &night_harbor())
        .await
        .unwrap()
        .item_id;
    let ended = import_series(&pool, TV, &catalog().series("99031").unwrap().clone())
        .await
        .unwrap()
        .item_id;
    let now = cabinet_core::catalog::now();

    assert!(
        due(&pool, "tmdb", now).await.unwrap().is_empty(),
        "just imported"
    );
    // A day later the continuing series is due; the ended one waits a week.
    assert_eq!(due(&pool, "tmdb", now + 86400 + 1).await.unwrap(), [series]);
    assert_eq!(due(&pool, "tmdb", now + 8 * 86400).await.unwrap().len(), 2);

    let provider = Provider::Fake(Arc::new(catalog()));
    refresh_item(&pool, &provider, series).await.unwrap();

    // The provider no longer knows it: the error is recorded and the data kept.
    let empty = Provider::Fake(Arc::new(Catalog::default()));
    assert!(refresh_item(&pool, &empty, ended).await.is_err());
    let (title, error): (String, Option<String>) =
        sqlx::query_as("SELECT title, refresh_error FROM item WHERE id = ?")
            .bind(ended)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(title, "Paper Moons");
    assert!(error.is_some());
}

#[tokio::test]
async fn a_refresh_asks_for_the_artwork_it_finds_with_its_own_trigger() {
    use cabinet_core::artwork::ArtworkStore;
    use cabinet_core::metadata::Provider;
    use cabinet_core::tasks::{Kind, Scope, Services, Tasks, Trigger};
    use std::sync::Arc;

    let (dir, pool) = db().await;
    import_series(&pool, TV, &night_harbor()).await.unwrap();
    sqlx::query("UPDATE item SET refreshed_at = 0")
        .execute(&pool)
        .await
        .unwrap();

    let mut found = catalog();
    for series in &mut found.series {
        if series.id == "99010" {
            series.episodes[0].still = Some("/still.jpg".into());
        }
    }
    let tasks = Tasks::new(
        pool.clone(),
        Services {
            provider: Provider::Fake(Arc::new(found)),
            // Nothing's listening, which makes no difference to asking.
            artwork: Some(ArtworkStore::with_images(
                dir.path().join("artwork"),
                "http://127.0.0.1:1".into(),
            )),
            ..Services::default()
        },
    );
    let scope = Scope::Library(TV);
    tasks
        .request(Kind::MetadataRefresh, scope, Trigger::Manual)
        .await
        .unwrap();
    tasks.wait_idle(Kind::MetadataRefresh, scope).await;
    tasks.wait_idle(Kind::Artwork, scope).await;

    let triggers: Vec<String> =
        sqlx::query_scalar("SELECT trigger FROM task WHERE kind = 'artwork' AND library_id = ?")
            .bind(TV)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(triggers, ["manual"]);
}

#[tokio::test]
async fn refreshing_a_library_refreshes_its_items_as_a_task() {
    use cabinet_core::metadata::Provider;
    use cabinet_core::tasks::{Kind, Scope, Services, Tasks, Trigger};
    use std::sync::Arc;

    let (_dir, pool) = db().await;
    let series = import_series(&pool, TV, &night_harbor())
        .await
        .unwrap()
        .item_id;
    let gone = import_series(&pool, TV, &catalog().series("99031").unwrap().clone())
        .await
        .unwrap()
        .item_id;
    // Another library's items are left alone.
    let movie = import_movie(&pool, MOVIES, &catalog().movie("99020").unwrap().clone())
        .await
        .unwrap()
        .item_id;
    sqlx::query("UPDATE item SET refreshed_at = 0")
        .execute(&pool)
        .await
        .unwrap();

    // The provider has lost one of the two series.
    let mut lost = catalog();
    lost.series.retain(|s| s.id != "99031");
    let tasks = Tasks::new(
        pool.clone(),
        Services {
            provider: Provider::Fake(Arc::new(lost)),
            ..Services::default()
        },
    );
    let scope = Scope::Library(TV);
    assert!(
        tasks
            .request(Kind::MetadataRefresh, scope, Trigger::Manual)
            .await
            .unwrap()
    );
    tasks.wait_idle(Kind::MetadataRefresh, scope).await;

    let (state, outcome, error): (String, String, Option<String>) = sqlx::query_as(
        "SELECT state, outcome, error FROM task WHERE kind = 'metadata_refresh' AND library_id = ?",
    )
    .bind(TV)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(state, "ok");
    let outcome: serde_json::Value = serde_json::from_str(&outcome).unwrap();
    assert_eq!(
        (outcome["refreshed"].as_i64(), outcome["failed"].as_i64()),
        (Some(1), Some(1))
    );
    assert_eq!(outcome["failures"][0]["item_id"].as_i64(), Some(gone));
    assert_eq!(error.as_deref(), Some("1 of 2 couldn't be refreshed."));

    let refreshed: Vec<(i64, i64, Option<String>)> = sqlx::query_as(
        "SELECT id, refreshed_at, refresh_error FROM item WHERE kind IN ('series', 'movie') ORDER BY id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let of = |id: i64| refreshed.iter().find(|r| r.0 == id).unwrap();
    assert!(of(series).1 > 0 && of(series).2.is_none());
    assert!(of(gone).2.is_some(), "the failure is recorded on the item");
    assert_eq!(of(movie).1, 0);
}

#[tokio::test]
async fn the_same_series_in_two_libraries_is_two_items() {
    let (_dir, pool) = db().await;
    sqlx::query("INSERT INTO library (id, name, kind) VALUES (3, 'Kids', 'tv')")
        .execute(&pool)
        .await
        .unwrap();
    let a = import_series(&pool, TV, &night_harbor()).await.unwrap();
    let b = import_series(&pool, 3, &night_harbor()).await.unwrap();
    assert!(a.created && b.created);
    assert_ne!(a.item_id, b.item_id);
    // Each has its own episodes, with the provider's IDs.
    let episodes = count(
        &pool,
        "SELECT count(*) FROM external_id x JOIN item e ON e.id = x.item_id
         WHERE e.kind = 'episode' AND x.is_primary",
    )
    .await;
    assert_eq!(episodes as usize, 2 * a.episodes_added);
    // Importing again finds each library's own.
    let again = import_series(&pool, 3, &night_harbor()).await.unwrap();
    assert_eq!((again.created, again.item_id), (false, b.item_id));
}
