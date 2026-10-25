//! The catalog against the demo data: the behaviour the design mockups show.

use cabinet_core::catalog::{self, Availability, EpisodeList, NumberStyle, ViewRef};
use sqlx::SqlitePool;
use tempfile::TempDir;

async fn demo() -> (TempDir, SqlitePool, i64) {
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
    (dir, pool, kitsune)
}

async fn view_id(pool: &SqlitePool, series: i64, name: &str) -> ViewRef {
    let detail = catalog::series_detail(pool, series, Some(1), catalog::now())
        .await
        .unwrap();
    let view = detail.views.iter().find(|v| v.name == name).unwrap();
    ViewRef::parse(&view.id).unwrap()
}

async fn episodes(pool: &SqlitePool, series: i64, view: ViewRef) -> EpisodeList {
    catalog::episodes(pool, series, Some(view), Some(1), catalog::now())
        .await
        .unwrap()
}

fn titles(list: &EpisodeList) -> Vec<&str> {
    list.entries
        .iter()
        .map(|e| e.episode.title.as_str())
        .collect()
}

#[tokio::test]
async fn aired_order_matches_the_mockup() {
    let (_dir, pool, kitsune) = demo().await;
    let aired = view_id(&pool, kitsune, "Aired").await;
    let list = episodes(&pool, kitsune, aired).await;

    assert_eq!(list.entries.len(), 12);
    let third = &list.entries[2];
    assert_eq!(third.episode.title, "Market Day");
    let number = third.episode.number.as_ref().unwrap();
    assert_eq!(
        (number.style, number.season, number.number),
        (NumberStyle::Season, Some(1), Some(3))
    );
    assert!(third.is_filler && third.is_gap);
    assert_eq!(list.entries[4].availability, Availability::Missing);
    assert_eq!(list.entries[10].availability, Availability::Upcoming);
    assert_eq!(list.entries[11].availability, Availability::Unknown);
    assert_eq!(list.entries[7].files.len(), 2, "two versions of Foxfire");
    assert_eq!(list.entries[9].statuses[0].status, "downloading");

    // Up next is Bathhouse Detour (a filler), the first available unwatched episode.
    assert_eq!(list.up_next, Some(5));
    assert_eq!(
        (
            list.counts.available,
            list.counts.missing,
            list.counts.not_acquired,
            list.counts.wanted
        ),
        (7, 1, 2, 3)
    );
    assert_eq!(list.gaps, 2);
}

#[tokio::test]
async fn no_filler_playlist_drops_filler_and_moves_up_next() {
    let (_dir, pool, kitsune) = demo().await;
    let no_filler = view_id(&pool, kitsune, "No filler").await;
    let list = episodes(&pool, kitsune, no_filler).await;

    assert_eq!(list.entries.len(), 10);
    assert!(!titles(&list).contains(&"Market Day"));
    assert!(!titles(&list).contains(&"Bathhouse Detour"));
    let up_next = &list.entries[list.up_next.unwrap()];
    assert_eq!(up_next.episode.title, "Two Tails");
    let number = up_next.episode.number.as_ref().unwrap();
    assert_eq!(
        (number.style, number.number),
        (NumberStyle::Playlist, Some(7))
    );
    assert_eq!(list.gaps, 1, "Market Day's gap is gone");
}

#[tokio::test]
async fn absolute_order_numbers_straight_through() {
    let (_dir, pool, kitsune) = demo().await;
    let absolute = view_id(&pool, kitsune, "Absolute").await;
    let list = episodes(&pool, kitsune, absolute).await;
    let number = list.entries[8].episode.number.as_ref().unwrap();
    assert_eq!(
        (number.style, number.season, number.number),
        (NumberStyle::Absolute, None, Some(9))
    );
}

#[tokio::test]
async fn user_view_drives_the_default_listing() {
    let (_dir, pool, kitsune) = demo().await;
    // The demo user follows the no-filler playlist.
    let list = catalog::episodes(&pool, kitsune, None, Some(1), catalog::now())
        .await
        .unwrap();
    assert_eq!(list.entries.len(), 10);

    let aired = view_id(&pool, kitsune, "Aired").await;
    catalog::set_view(&pool, kitsune, 1, aired).await.unwrap();
    let list = catalog::episodes(&pool, kitsune, None, Some(1), catalog::now())
        .await
        .unwrap();
    assert_eq!(list.entries.len(), 12);

    let other = ViewRef::Ordering(9999);
    assert!(catalog::set_view(&pool, kitsune, 1, other).await.is_err());
}

#[tokio::test]
async fn unmonitoring_an_episode_lowers_wanted_and_emits_an_event() {
    let (_dir, pool, kitsune) = demo().await;
    let aired = view_id(&pool, kitsune, "Aired").await;
    let mountain_road = episodes(&pool, kitsune, aired).await.entries[4]
        .episode
        .item_id;

    catalog::set_monitored(&pool, mountain_road, Some(false))
        .await
        .unwrap();

    let list = episodes(&pool, kitsune, aired).await;
    assert_eq!(list.counts.wanted, 2);
    assert!(!list.entries[4].monitored);
    let events: Vec<(String, String)> = sqlx::query_as("SELECT type, payload FROM event")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].0, "item.unwanted");
    assert!(events[0].1.contains(&mountain_road.to_string()));
}

#[tokio::test]
async fn home_matches_the_mockup() {
    let (_dir, pool, _) = demo().await;
    let home = catalog::home(&pool, Some(1), catalog::now()).await.unwrap();

    let hero = home.hero.unwrap();
    assert_eq!(
        (hero.series_title.as_str(), hero.episode.title.as_str()),
        ("Kitsune Road", "Two Tails")
    );
    assert_eq!(hero.view_name.as_deref(), Some("No filler"));
    let keep: Vec<&str> = home
        .keep_watching
        .iter()
        .map(|c| c.series_title.as_str())
        .collect();
    assert_eq!(
        keep,
        [
            "Kitsune Road",
            "Harbor Lights",
            "Paper Saints",
            "The Long Orbit"
        ]
    );
    let coming: Vec<&str> = home
        .coming_up
        .iter()
        .map(|c| c.series_title.as_str())
        .collect();
    assert_eq!(
        coming,
        ["Harbor Lights", "Lowland", "Kitsune Road", "The Quiet Year"]
    );
    assert_eq!(home.shows.len(), 8);

    let a = home.attention;
    assert_eq!((a.missing_files, a.wanted, a.in_progress), (7, 20, 1));
    assert_eq!(a.offline_roots.len(), 1);
    assert_eq!(a.status_sources, ["sonarr"]);
}

#[tokio::test]
async fn edits_locks_tags_and_matching_ordering() {
    use cabinet_core::items::{self, ItemEdit};
    let (_dir, pool, kitsune) = demo().await;

    items::edit(
        &pool,
        kitsune,
        ItemEdit {
            title: Some("Kitsune Road (TV)".into()),
            lock: true,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let detail = catalog::series_detail(&pool, kitsune, Some(1), catalog::now())
        .await
        .unwrap();
    assert_eq!(detail.summary.title, "Kitsune Road (TV)");
    assert_eq!(detail.locked_fields, ["title"]);
    assert!(
        items::edit(
            &pool,
            kitsune,
            ItemEdit {
                title: Some("  ".into()),
                ..Default::default()
            }
        )
        .await
        .is_err()
    );

    // An external tool marks the finale as filler.
    let aired = view_id(&pool, kitsune, "Aired").await;
    let foxfire = episodes(&pool, kitsune, aired).await.entries[7]
        .episode
        .item_id;
    items::set_tags(&pool, foxfire, "animefillerlist", &["filler".into()])
        .await
        .unwrap();
    assert!(episodes(&pool, kitsune, aired).await.entries[7].is_filler);
    let no_filler = view_id(&pool, kitsune, "No filler").await;
    assert_eq!(episodes(&pool, kitsune, no_filler).await.entries.len(), 9);

    let absolute = match view_id(&pool, kitsune, "Absolute").await {
        ViewRef::Ordering(id) => id,
        _ => unreachable!(),
    };
    items::set_match_ordering(&pool, kitsune, Some(absolute))
        .await
        .unwrap();
    let detail = catalog::series_detail(&pool, kitsune, Some(1), catalog::now())
        .await
        .unwrap();
    assert_eq!(detail.match_view, Some(format!("ordering:{absolute}")));
    let other_series: i64 =
        sqlx::query_scalar("SELECT id FROM ordering WHERE series_id <> ? LIMIT 1")
            .bind(kitsune)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        items::set_match_ordering(&pool, kitsune, Some(other_series))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn search_finds_series_and_episodes() {
    let (_dir, pool, _) = demo().await;
    let results = catalog::movies::search(&pool, "kitsune", Some(1), catalog::now())
        .await
        .unwrap();
    assert_eq!(results.series.len(), 1);
    let results = catalog::movies::search(&pool, "two tails", Some(1), catalog::now())
        .await
        .unwrap();
    assert_eq!(results.episodes.len(), 1);
    assert_eq!(results.episodes[0].series_title, "Kitsune Road");
    let results = catalog::movies::search(&pool, "100%_", Some(1), catalog::now())
        .await
        .unwrap();
    assert!(
        results.series.is_empty() && results.episodes.is_empty(),
        "wildcards are literal"
    );
}
