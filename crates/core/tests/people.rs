//! People, companies and ratings: stored on import, replaced on refresh, and read
//! back for an item's page and a person's page.

use cabinet_core::catalog::people;
use cabinet_core::metadata::fake::Catalog;
use cabinet_core::metadata::import::{import_movie, import_series};
use cabinet_core::metadata::{
    CompanyData, CreditData, Credits, MovieData, PersonRef, RatingData, SeriesData,
};
use sqlx::SqlitePool;
use tempfile::TempDir;

const TV: i64 = 1;
const MOVIES: i64 = 2;

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

fn credit(id: &str, name: &str, role: &str) -> CreditData {
    CreditData {
        person: PersonRef {
            id: id.into(),
            name: name.into(),
            photo: Some(format!("/{id}.jpg")),
            known_for: None,
        },
        role: Some(role.into()),
        department: None,
        episodes: None,
    }
}

fn night_harbor() -> SeriesData {
    let catalog = Catalog::from_json(include_str!("data/catalog.json")).unwrap();
    let mut s = catalog.series("99010").unwrap().clone();
    s.content_rating = Some("TV-14".into());
    s.networks = vec![CompanyData {
        id: Some("49".into()),
        name: "Harbor Network".into(),
    }];
    s.rating = Some(RatingData {
        value: 7.9,
        scale: 10.0,
        votes: Some(120),
    });
    s.credits = Credits {
        cast: vec![
            CreditData {
                episodes: Some(13),
                ..credit("1", "Ada Lane", "Keeper")
            },
            credit("2", "Bo Reyes", "Ferryman"),
        ],
        guests: vec![],
        crew: vec![credit("3", "Cy Maker", "Creator")],
    };
    s.episodes[0].credits = Credits {
        cast: vec![],
        guests: vec![credit("4", "Di Guest", "Stranger")],
        crew: vec![credit("3", "Cy Maker", "Director")],
    };
    s
}

fn meridian() -> MovieData {
    let catalog = Catalog::from_json(include_str!("data/catalog.json")).unwrap();
    let mut m = catalog.movie("99020").unwrap().clone();
    m.tagline = Some("Hold the line.".into());
    m.credits.cast = vec![credit("1", "Ada Lane", "Navigator")];
    m
}

async fn count(pool: &SqlitePool, sql: &'static str) -> i64 {
    sqlx::query_scalar(sql).fetch_one(pool).await.unwrap()
}

#[tokio::test]
async fn imports_credits_companies_and_ratings() {
    let (_dir, pool) = db().await;
    let series = import_series(&pool, TV, &night_harbor())
        .await
        .unwrap()
        .item_id;

    let about = people::about(&pool, series).await.unwrap();
    assert_eq!(about.content_rating.as_deref(), Some("TV-14"));
    assert_eq!(about.networks, ["Harbor Network"]);
    assert_eq!(about.ratings[0].source, "tmdb");
    let cast: Vec<(&str, Option<&str>, Option<i64>)> = about
        .cast
        .iter()
        .map(|c| (c.name.as_str(), c.role.as_deref(), c.episodes))
        .collect();
    assert_eq!(
        cast,
        [
            ("Ada Lane", Some("Keeper"), Some(13)),
            ("Bo Reyes", Some("Ferryman"), None)
        ]
    );
    assert!(about.cast[0].has_photo);
    assert_eq!(about.crew[0].role.as_deref(), Some("Creator"));

    let pilot: i64 = sqlx::query_scalar(
        "SELECT item_id FROM ordering_entry WHERE position = 1
         AND ordering_id = (SELECT id FROM ordering WHERE series_id = ? AND is_default)",
    )
    .bind(series)
    .fetch_one(&pool)
    .await
    .unwrap();
    let episode = people::about(&pool, pilot).await.unwrap();
    assert_eq!(episode.guests[0].name, "Di Guest");
    assert_eq!(episode.crew[0].role.as_deref(), Some("Director"));

    // One row per person, however many credits.
    assert_eq!(count(&pool, "SELECT count(*) FROM person").await, 4);
}

#[tokio::test]
async fn a_person_page_lists_everything_they_are_in() {
    let (_dir, pool) = db().await;
    import_series(&pool, TV, &night_harbor()).await.unwrap();
    import_movie(&pool, MOVIES, &meridian()).await.unwrap();

    let ada: i64 = sqlx::query_scalar("SELECT id FROM person WHERE name = 'Ada Lane'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let detail = people::person(&pool, ada).await.unwrap();
    let credits: Vec<(&str, &str, Vec<&str>)> = detail
        .credits
        .iter()
        .map(|c| {
            (
                c.kind.as_str(),
                c.title.as_str(),
                c.roles.iter().map(String::as_str).collect(),
            )
        })
        .collect();
    assert_eq!(
        credits,
        [
            ("series", "Night Harbor", vec!["Keeper"]),
            ("movie", "The Quiet Meridian", vec!["Navigator"]),
        ],
        "newest first"
    );
    assert_eq!(detail.credits[0].episode_count, Some(13));

    // The creator directed the pilot: one series credit with the episode inside.
    let cy: i64 = sqlx::query_scalar("SELECT id FROM person WHERE name = 'Cy Maker'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let detail = people::person(&pool, cy).await.unwrap();
    assert_eq!(detail.credits.len(), 1);
    assert_eq!(detail.credits[0].roles, ["Creator", "Director"]);
    let pilot = &detail.credits[0].episodes[0];
    assert_eq!(pilot.role.as_deref(), Some("Director"));
    assert_eq!(pilot.number.as_ref().and_then(|n| n.season), Some(1));
}

#[tokio::test]
async fn refresh_replaces_credits_and_forgets_people_no_longer_credited() {
    let (_dir, pool) = db().await;
    let series = import_series(&pool, TV, &night_harbor())
        .await
        .unwrap()
        .item_id;

    let mut data = night_harbor();
    data.credits.cast.remove(1);
    data.credits.cast[0].role = Some("The Keeper".into());
    data.networks.clear();
    data.rating = None;
    import_series(&pool, TV, &data).await.unwrap();

    let about = people::about(&pool, series).await.unwrap();
    assert_eq!(about.cast.len(), 1);
    assert_eq!(about.cast[0].role.as_deref(), Some("The Keeper"));
    assert!(about.networks.is_empty());
    assert!(about.ratings.is_empty());
    assert_eq!(
        count(&pool, "SELECT count(*) FROM person WHERE name = 'Bo Reyes'").await,
        0
    );

    // Someone credited elsewhere stays.
    import_movie(&pool, MOVIES, &meridian()).await.unwrap();
    data.credits.cast.clear();
    import_series(&pool, TV, &data).await.unwrap();
    assert_eq!(
        count(&pool, "SELECT count(*) FROM person WHERE name = 'Ada Lane'").await,
        1
    );
}
