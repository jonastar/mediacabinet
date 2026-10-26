//! References, *arr lists and adding items, against the demo data.

use cabinet_core::refs::{self, ItemRef, Position};
use sqlx::SqlitePool;
use tempfile::TempDir;

async fn demo() -> (TempDir, SqlitePool) {
    let dir = tempfile::tempdir().unwrap();
    let pool = cabinet_core::db::open(&dir.path().join("cabinet.db"))
        .await
        .unwrap();
    cabinet_core::demo::seed(
        &pool,
        &dir.path().join("media"),
        cabinet_core::catalog::now(),
    )
    .await
    .unwrap();
    (dir, pool)
}

async fn title(pool: &SqlitePool, id: i64) -> String {
    sqlx::query_scalar("SELECT title FROM item WHERE id = ?")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn references_resolve_by_provider_and_position() {
    let (_dir, pool) = demo().await;
    let r = |s: &str| ItemRef::parse(s).unwrap();

    let series = refs::resolve(&pool, &r("tmdb:93405"), None).await.unwrap();
    assert_eq!(title(&pool, series).await, "Kitsune Road");
    // A secondary provider's ID works too.
    assert_eq!(
        refs::resolve(&pool, &r("tvdb:361205"), None).await.unwrap(),
        series
    );

    let aired = Position::parse("aired", "S02E01").unwrap();
    let ep = refs::resolve(&pool, &r("tvdb:361205"), Some(&aired))
        .await
        .unwrap();
    assert_eq!(title(&pool, ep).await, "Return to the Shrine");
    let absolute = Position::parse("absolute", "9").unwrap();
    assert_eq!(
        refs::resolve(&pool, &r("tmdb:93405"), Some(&absolute))
            .await
            .unwrap(),
        ep
    );
    let default = Position::parse("default", "S01E07").unwrap();
    let two_tails = refs::resolve(&pool, &r("tmdb:93405"), Some(&default))
        .await
        .unwrap();
    assert_eq!(title(&pool, two_tails).await, "Two Tails");

    assert!(refs::resolve(&pool, &r("tmdb:1"), None).await.is_err());
    let dvd = Position::parse("dvd", "S01E01").unwrap();
    assert!(
        refs::resolve(&pool, &r("tmdb:93405"), Some(&dvd))
            .await
            .is_err(),
        "no DVD ordering"
    );

    let view = refs::item_view(&pool, ep, cabinet_core::catalog::now())
        .await
        .unwrap();
    assert_eq!(view.parent_title.as_deref(), Some("Kitsune Road"));
    let kinds: Vec<&str> = view.numbers.iter().map(|n| n.kind.as_str()).collect();
    assert_eq!(kinds, ["aired", "absolute"]);
    assert_eq!(view.files.len(), 1);
}

#[tokio::test]
async fn arr_lists_carry_provider_ids() {
    let (_dir, pool) = demo().await;
    let sonarr = cabinet_core::arr::sonarr_list(&pool).await.unwrap();
    let kitsune = sonarr.iter().find(|s| s.title == "Kitsune Road").unwrap();
    assert_eq!(
        (kitsune.tmdb_id, kitsune.tvdb_id),
        (Some(93405), Some(361205))
    );
    let json = serde_json::to_value(kitsune).unwrap();
    assert!(
        json.get("tvdbId").is_some() && json.get("tmdbId").is_some(),
        "Sonarr reads camelCase"
    );
    assert!(
        !sonarr.iter().any(|s| s.title == "Signal & Noise"),
        "nothing wanted there"
    );
    assert!(
        cabinet_core::arr::radarr_list(&pool)
            .await
            .unwrap()
            .is_empty(),
        "the demo has no movies"
    );
}

#[tokio::test]
async fn adding_by_provider_id_imports_and_monitors() {
    use cabinet_core::metadata::Provider;
    use cabinet_core::metadata::fake::Catalog;
    use std::sync::Arc;

    let dir = tempfile::tempdir().unwrap();
    let pool = cabinet_core::db::open(&dir.path().join("cabinet.db"))
        .await
        .unwrap();
    let provider = Provider::Fake(Arc::new(
        Catalog::from_json(include_str!("data/catalog.json")).unwrap(),
    ));

    let report = cabinet_core::items::add(&pool, &provider, "series", "99010", true, None)
        .await
        .unwrap();
    assert!(report.created);
    // Nothing on disk yet: every aired episode is wanted, with an event each,
    // except the special, which isn't monitored by default.
    let wanted: i64 = sqlx::query_scalar("SELECT count(*) FROM item_wanted")
        .fetch_one(&pool)
        .await
        .unwrap();
    let events: i64 = sqlx::query_scalar("SELECT count(*) FROM event WHERE type = 'item.wanted'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!((wanted, events), (12, 12));

    let again = cabinet_core::items::add(&pool, &provider, "series", "99010", false, None)
        .await
        .unwrap();
    assert_eq!((again.created, again.item_id), (false, report.item_id));
    let wanted: i64 = sqlx::query_scalar("SELECT count(*) FROM item_wanted")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(wanted, 0, "unmonitored on the second add");

    assert!(
        cabinet_core::items::add(&pool, &provider, "series", "99010", true, Some(999))
            .await
            .is_err()
    );
    assert!(
        cabinet_core::items::add(&pool, &provider, "podcast", "1", true, None)
            .await
            .is_err()
    );
}
