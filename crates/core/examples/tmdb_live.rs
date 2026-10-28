//! Exercises the TMDB client against the live API, importing into a temporary
//! database. Needs CABINET_TMDB_KEY.
//!
//!   cargo run -p cabinet-core --example tmdb_live -- "Series title" "Movie title"

use cabinet_core::metadata::import::{import_movie, import_series};
use cabinet_core::metadata::tmdb::Tmdb;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let key = std::env::var("CABINET_TMDB_KEY").map_err(|_| "set CABINET_TMDB_KEY")?;
    let mut args = std::env::args().skip(1);
    let series_query = args.next().unwrap_or_else(|| "Doctor Who".into());
    let movie_query = args.next().unwrap_or_else(|| "Blade Runner 2049".into());
    let tmdb = Tmdb::new(key);
    let dir = tempfile::tempdir()?;
    let pool = cabinet_core::db::open(&dir.path().join("cabinet.db")).await?;
    sqlx::raw_sql(
        "INSERT INTO library (id, name, kind) VALUES (1, 'TV', 'tv'), (2, 'Movies', 'movies')",
    )
    .execute(&pool)
    .await?;

    let results = tmdb.search_series(&series_query, None).await?;
    println!("search \"{series_query}\": {} results", results.len());
    for r in results.iter().take(3) {
        println!("  {} ({:?}) tmdb:{}", r.title, r.year, r.id);
    }
    let first = results.first().ok_or("no series found")?;
    let t = std::time::Instant::now();
    let data = tmdb.series(&first.id).await?;
    println!(
        "series {} ({:?}) {:?}: {} seasons, {} episodes, genres {:?}, ids {:?}, fetched in {:.1}s",
        data.title,
        data.year,
        data.status,
        data.seasons.len(),
        data.episodes.len(),
        data.genres,
        data.external_ids,
        t.elapsed().as_secs_f64()
    );
    for o in &data.orderings {
        println!(
            "  ordering {} ({}): {} entries",
            o.name,
            o.kind,
            o.entries.len()
        );
    }
    let report = import_series(&pool, 1, &data).await?;
    println!("  imported: {report:?}");
    let again = import_series(&pool, 1, &data).await?;
    println!(
        "  re-imported: added {}, updated {}",
        again.episodes_added, again.episodes_updated
    );

    let movies = tmdb.search_movies(&movie_query, None).await?;
    let m = movies.first().ok_or("no movie found")?;
    let movie = tmdb.movie(&m.id).await?;
    println!(
        "movie {} ({:?}) {} min, ids {:?}, poster {:?}",
        movie.title,
        movie.year,
        movie.runtime_minutes.unwrap_or(0),
        movie.external_ids,
        movie.poster
    );
    import_movie(&pool, 2, &movie).await?;

    if let Some((_, tvdb)) = data.external_ids.iter().find(|(p, _)| p == "tvdb") {
        println!("find tvdb:{tvdb} -> {:?}", tmdb.find("tvdb", tvdb).await?);
    }
    Ok(())
}
