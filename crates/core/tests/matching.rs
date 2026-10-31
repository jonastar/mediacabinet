//! Identification and matching against the offline catalog, on a library laid
//! out like `fixtures/media`.

use std::fs;
use std::path::Path;
use std::sync::Arc;

use cabinet_core::library::{self, NewRoot};
use cabinet_core::matcher::{self, MatchReport, MatchSettings, review};
use cabinet_core::metadata::Provider;
use cabinet_core::metadata::fake::Catalog;
use sqlx::SqlitePool;
use tempfile::TempDir;

fn provider() -> Provider {
    Provider::Fake(Arc::new(
        Catalog::from_json(include_str!("data/catalog.json")).unwrap(),
    ))
}

fn write(base: &Path, rel: &str, content: &str) {
    let path = base.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

fn video(base: &Path, rel: &str) {
    write(base, rel, &format!("video {rel}").repeat(20));
}

struct Lib {
    _dir: TempDir,
    pool: SqlitePool,
    base: std::path::PathBuf,
}

impl Lib {
    async fn new() -> Lib {
        let dir = tempfile::tempdir().unwrap();
        let pool = cabinet_core::db::open(&dir.path().join("cabinet.db"))
            .await
            .unwrap();
        let base = dir.path().join("media");
        Lib {
            _dir: dir,
            pool,
            base,
        }
    }

    async fn root(&self, library: &str, kind: &str, folder: &str, template: &str) -> i64 {
        let libraries = library::list(&self.pool).await.unwrap();
        let id = match libraries.iter().find(|l| l.name == library) {
            Some(l) => l.id,
            None => library::create(&self.pool, library, kind).await.unwrap().id,
        };
        let path = self.base.join(folder);
        fs::create_dir_all(&path).unwrap();
        let root = library::add_root(
            &self.pool,
            id,
            NewRoot {
                path: path.to_str().unwrap(),
                write_policy: "adaptive",
                layout_template: template,
                is_import_target: false,
            },
        )
        .await
        .unwrap();
        root.id
    }

    async fn scan_and_match(&self, root: i64) -> MatchReport {
        cabinet_core::scan::scan_root(&self.pool, root)
            .await
            .unwrap();
        matcher::match_root(&self.pool, root, &provider(), MatchSettings::default())
            .await
            .unwrap()
    }

    /// file name → linked item titles, for a root.
    async fn links(&self, root: i64) -> Vec<(String, String)> {
        sqlx::query_as(
            "SELECT f.relative_path, group_concat(i.title, ' + ') FROM file_link l
             JOIN media_file f ON f.id = l.file_id JOIN item i ON i.id = l.item_id
             WHERE f.root_id = ? GROUP BY f.id ORDER BY f.relative_path",
        )
        .bind(root)
        .fetch_all(&self.pool)
        .await
        .unwrap()
    }
}

fn names(links: &[(String, String)]) -> Vec<String> {
    links
        .iter()
        .map(|(path, items)| format!("{} = {items}", path.rsplit('/').next().unwrap()))
        .collect()
}

#[tokio::test]
async fn tv_library() {
    let lib = Lib::new().await;
    let tv = lib.base.join("tv");
    write(
        &tv,
        "Night Harbor (2019)/tvshow.nfo",
        r#"<tvshow><uniqueid type="tmdb" default="true">99010</uniqueid></tvshow>"#,
    );
    for f in [
        "Night Harbor (2019)/Season 01/Night Harbor - S01E01 - Pilot.mkv",
        "Night Harbor (2019)/Season 01/Night Harbor - S01E02 - The Tide.mkv",
        "Night Harbor (2019)/Season 01/sample-night.harbor.s01e02.mkv",
        "Night Harbor (2019)/Season 02/Night.Harbor.S02E02E03.1080p.WEB-DL.DDP5.1.H.264-GRP.mkv",
        "Night Harbor (2019)/Specials/Night Harbor - S00E01 - Behind the Scenes.mkv",
        "Glass Orchard {tmdb-99001}/Season 1/Glass Orchard 1x03 - Grafting.mp4",
        "Unmatched Show/Season 01/Unmatched Show S01E01.mkv",
        "loose.mkv",
    ] {
        video(&tv, f);
    }
    let root = lib
        .root("TV", "tv", "tv", "{series}/Season {season:00}/{filename}")
        .await;

    let report = lib.scan_and_match(root).await;

    assert_eq!(
        names(&lib.links(root).await),
        [
            "Glass Orchard 1x03 - Grafting.mp4 = Grafting",
            "Night Harbor - S01E01 - Pilot.mkv = Pilot",
            "Night Harbor - S01E02 - The Tide.mkv = The Tide",
            "Night.Harbor.S02E02E03.1080p.WEB-DL.DDP5.1.H.264-GRP.mkv = Episode 2 + Episode 3",
            "Night Harbor - S00E01 - Behind the Scenes.mkv = Behind the Scenes",
        ]
    );
    assert_eq!(report.identified, 2);
    assert_eq!((report.ignored, report.unsorted), (1, 1));
    let locations: Vec<String> =
        sqlx::query_scalar("SELECT path FROM series_location ORDER BY path")
            .fetch_all(&lib.pool)
            .await
            .unwrap();
    assert_eq!(
        locations,
        ["Glass Orchard {tmdb-99001}", "Night Harbor (2019)"]
    );

    // Matching again changes nothing.
    let again = lib.scan_and_match(root).await;
    assert_eq!(
        (again.linked, again.files_queued, again.folders_queued),
        (0, 0, 0)
    );
}

#[tokio::test]
async fn grouped_and_anime_roots() {
    let lib = Lib::new().await;
    for f in [
        "tv-grouped/2021/Drama/Lantern Street/Season 01/Lantern.Street.S01E01.720p.mkv",
        "tv-grouped/2008/Comedy/Paper Moons (2008)/Season 03/Paper Moons - S03E10.mkv",
        "anime/Starfall Brigade/[Kumo] Starfall Brigade - 01 [1080p][A1B2C3D4].mkv",
        "anime/Starfall Brigade/[Kumo] Starfall Brigade - 14 [1080p][D4E5F6A7].mkv",
        "anime/Starfall Brigade/[Other-Group] Starfall Brigade - 15v2 (BD 1080p HEVC).mkv",
    ] {
        video(&lib.base, f);
    }
    let grouped = lib
        .root(
            "TV",
            "tv",
            "tv-grouped",
            "{year}/{genre}/{series}/Season {season:00}/{filename}",
        )
        .await;
    let anime = lib.root("TV", "tv", "anime", "{series}/{filename}").await;

    lib.scan_and_match(grouped).await;
    lib.scan_and_match(anime).await;

    assert_eq!(
        names(&lib.links(grouped).await),
        [
            "Paper Moons - S03E10.mkv = Episode 10",
            "Lantern.Street.S01E01.720p.mkv = Episode 1"
        ]
    );
    // Absolute numbers through the derived absolute order: 14 and 15 are S02E02 and S02E03.
    let anime_links = lib.links(anime).await;
    assert_eq!(anime_links.len(), 3);
    let s2: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT os.number, oe.episode_number FROM file_link l
         JOIN media_file f ON f.id = l.file_id
         JOIN ordering_entry oe ON oe.item_id = l.item_id
         JOIN ordering o ON o.id = oe.ordering_id AND o.is_default
         JOIN ordering_season os ON os.id = oe.ordering_season_id
         WHERE f.root_id = ? ORDER BY os.number, oe.episode_number",
    )
    .bind(anime)
    .fetch_all(&lib.pool)
    .await
    .unwrap();
    assert_eq!(s2, [(1, 1), (2, 2), (2, 3)]);
}

#[tokio::test]
async fn movie_library() {
    let lib = Lib::new().await;
    let m = lib.base.join("movies");
    write(
        &m,
        "The Quiet Meridian (2014)/The Quiet Meridian (2014).nfo",
        "<movie><imdbid>tt9902000</imdbid></movie>",
    );
    for f in [
        "The Quiet Meridian (2014)/The Quiet Meridian (2014).mkv",
        "The Quiet Meridian (2014)/The Quiet Meridian (2014) - Director's Cut.mkv",
        "The Quiet Meridian (2014)/The Quiet Meridian (2014)-trailer.mp4",
        "Halfway Coast (2001)/Halfway Coast (2001) - 2160p HDR10.mkv",
        "Halfway Coast (2001)/Halfway Coast (2001) - 1080p.mkv",
        "Long Winter Road (1987)/Long Winter Road (1987) - cd1.mkv",
        "Long Winter Road (1987)/Long Winter Road (1987) - cd2.mkv",
        "some.random.movie.2010.720p.BluRay.x264-NOGRP.mkv",
    ] {
        video(&m, f);
    }
    let root = lib
        .root("Movies", "movies", "movies", "{movie} ({year})/{filename}")
        .await;

    let report = lib.scan_and_match(root).await;

    assert_eq!(report.ignored, 1, "the trailer");
    assert_eq!(report.identified, 4);
    let links: Vec<(String, String, Option<String>, Option<i64>)> = sqlx::query_as(
        "SELECT f.relative_path, i.title, l.edition, l.part FROM file_link l
         JOIN media_file f ON f.id = l.file_id JOIN item i ON i.id = l.item_id
         ORDER BY f.relative_path",
    )
    .fetch_all(&lib.pool)
    .await
    .unwrap();
    let summary: Vec<String> = links
        .iter()
        .map(|(p, t, e, part)| {
            format!("{} → {t} {:?} {:?}", p.rsplit('/').next().unwrap(), e, part)
        })
        .collect();
    assert_eq!(
        summary,
        [
            "Halfway Coast (2001) - 1080p.mkv → Halfway Coast None None",
            "Halfway Coast (2001) - 2160p HDR10.mkv → Halfway Coast None None",
            "Long Winter Road (1987) - cd1.mkv → Long Winter Road None Some(1)",
            "Long Winter Road (1987) - cd2.mkv → Long Winter Road None Some(2)",
            "The Quiet Meridian (2014) - Director's Cut.mkv → The Quiet Meridian Some(\"Director's Cut\") None",
            "The Quiet Meridian (2014).mkv → The Quiet Meridian None None",
            "some.random.movie.2010.720p.BluRay.x264-NOGRP.mkv → Some Random Movie None None",
        ]
    );
    let year: i64 = sqlx::query_scalar("SELECT year FROM item WHERE title = 'Some Random Movie'")
        .fetch_one(&lib.pool)
        .await
        .unwrap();
    assert_eq!(
        year, 2010,
        "the year in the name picks the right one of two"
    );
}

#[tokio::test]
async fn episodes_split_over_files() {
    let lib = Lib::new().await;
    let tv = lib.base.join("tv");
    write(
        &tv,
        "Night Harbor (2019)/tvshow.nfo",
        r#"<tvshow><uniqueid type="tmdb" default="true">99010</uniqueid></tvshow>"#,
    );
    for f in [
        // Numbered as two episodes where the series has one; S01E07 is past the season.
        "Night Harbor - S01E06 - Fog (1) WEBDL-1080p.mkv",
        "Night Harbor - S01E07 - Fog (2) WEBDL-1080p.mkv",
        // Two parts under one number.
        "Night Harbor - S01E03 - Lighthouse Part 1.mkv",
        "Night Harbor - S01E03 - Lighthouse Part 2.mkv",
    ] {
        video(&tv, &format!("Night Harbor (2019)/Season 01/{f}"));
    }
    let root = lib
        .root("TV", "tv", "tv", "{series}/Season {season:00}/{filename}")
        .await;

    let report = lib.scan_and_match(root).await;
    assert_eq!((report.linked, report.files_queued), (4, 0));
    let parts: Vec<(String, String, Option<i64>)> = sqlx::query_as(
        "SELECT f.relative_path, i.title, l.part FROM file_link l
         JOIN media_file f ON f.id = l.file_id JOIN item i ON i.id = l.item_id
         ORDER BY f.relative_path",
    )
    .fetch_all(&lib.pool)
    .await
    .unwrap();
    let parts: Vec<(&str, &str, Option<i64>)> = parts
        .iter()
        .map(|(p, t, n)| (p.rsplit('/').next().unwrap(), t.as_str(), *n))
        .collect();
    assert_eq!(
        parts,
        [
            (
                "Night Harbor - S01E03 - Lighthouse Part 1.mkv",
                "Lighthouse",
                Some(1)
            ),
            (
                "Night Harbor - S01E03 - Lighthouse Part 2.mkv",
                "Lighthouse",
                Some(2)
            ),
            (
                "Night Harbor - S01E06 - Fog (1) WEBDL-1080p.mkv",
                "Fog",
                Some(1)
            ),
            (
                "Night Harbor - S01E07 - Fog (2) WEBDL-1080p.mkv",
                "Fog",
                Some(2)
            ),
        ]
    );
}

#[tokio::test]
async fn uncertain_matches_go_to_review() {
    let lib = Lib::new().await;
    let tv = lib.base.join("tv");
    // No year and no ID: two series are called "Night Harbor" (2019 and 2004).
    video(&tv, "Night Harbor/Season 01/Night Harbor - S01E01.mkv");
    video(
        &tv,
        "Night Harbor/Season 01/Night Harbor - S01E04 - Something Else.mkv",
    );
    let root = lib
        .root("TV", "tv", "tv", "{series}/Season {season:00}/{filename}")
        .await;

    let report = lib.scan_and_match(root).await;
    assert_eq!((report.identified, report.folders_queued), (0, 1));
    assert!(
        lib.links(root).await.is_empty(),
        "nothing is imported on a guess"
    );

    let queue = review::queue(&lib.pool).await.unwrap();
    assert_eq!(queue.folders.len(), 1);
    let folder = &queue.folders[0];
    assert_eq!(folder.folder_path, "Night Harbor");
    let titles: Vec<&str> = folder.candidates.iter().map(|c| c.title.as_str()).collect();
    assert_eq!(titles, ["Night Harbor (2019)", "Night Harbor (2004)"]);
    assert!(folder.candidates[0].reason.contains("nearly as close"));

    // Accepting identifies the folder and matches its files.
    review::accept(
        &lib.pool,
        folder.candidates[0].id,
        false,
        &provider(),
        MatchSettings::default(),
    )
    .await
    .unwrap();
    assert_eq!(
        names(&lib.links(root).await),
        ["Night Harbor - S01E01.mkv = Pilot"]
    );

    // The other file's episode title doesn't fit S01E04 ("Breakwater"): review it.
    let queue = review::queue(&lib.pool).await.unwrap();
    assert!(queue.folders.is_empty());
    assert_eq!(queue.files.len(), 1);
    let file = &queue.files[0];
    assert_eq!(file.candidates[0].title, "S01E04 · Breakwater");
    assert!(
        file.candidates[0].reason.contains("differs"),
        "{}",
        file.candidates[0].reason
    );

    review::accept(
        &lib.pool,
        file.candidates[0].id,
        true,
        &provider(),
        MatchSettings::default(),
    )
    .await
    .unwrap();
    let (source, pinned): (String, bool) =
        sqlx::query_as("SELECT source, pinned FROM file_link WHERE file_id = ?")
            .bind(file.file_id)
            .fetch_one(&lib.pool)
            .await
            .unwrap();
    assert_eq!((source.as_str(), pinned), ("user", true));
    assert!(review::queue(&lib.pool).await.unwrap().files.is_empty());
}

#[tokio::test]
async fn unlinked_files_stay_unlinked() {
    let lib = Lib::new().await;
    let tv = lib.base.join("tv");
    write(
        &tv,
        "Night Harbor (2019)/tvshow.nfo",
        r#"<tvshow><uniqueid type="tmdb" default="true">99010</uniqueid></tvshow>"#,
    );
    video(
        &tv,
        "Night Harbor (2019)/Season 01/Night Harbor - S01E01 - Pilot.mkv",
    );
    let root = lib
        .root("TV", "tv", "tv", "{series}/Season {season:00}/{filename}")
        .await;
    lib.scan_and_match(root).await;
    let (file, item): (i64, i64) = sqlx::query_as("SELECT file_id, item_id FROM file_link")
        .fetch_one(&lib.pool)
        .await
        .unwrap();

    review::set_edition(&lib.pool, file, item, Some(" Extended "))
        .await
        .unwrap();
    let edition: Option<String> = sqlx::query_scalar("SELECT edition FROM file_link")
        .fetch_one(&lib.pool)
        .await
        .unwrap();
    assert_eq!(edition.as_deref(), Some("Extended"));

    review::unlink(&lib.pool, file, item).await.unwrap();
    assert!(matches!(
        review::unlink(&lib.pool, file, item).await,
        Err(review::ReviewError::NotFound)
    ));
    // Scans don't link it again, and it waits in review.
    let again = lib.scan_and_match(root).await;
    assert_eq!((again.files_queued, lib.links(root).await.len()), (0, 0));
    assert_eq!(review::queue(&lib.pool).await.unwrap().unmatched.len(), 1);

    // The episode's history has the link, the edition and the unlink.
    let history: Vec<String> = cabinet_core::events::for_item(&lib.pool, item, 50)
        .await
        .unwrap()
        .into_iter()
        .map(|e| e.kind)
        .collect();
    assert_eq!(
        history[..3],
        ["link.removed", "link.changed", "link.created"]
    );

    // A person can link it again.
    review::link_file(&lib.pool, file, &[item], false)
        .await
        .unwrap();
    assert_eq!(lib.links(root).await.len(), 1);
    let rejections: i64 = sqlx::query_scalar("SELECT count(*) FROM link_rejection")
        .fetch_one(&lib.pool)
        .await
        .unwrap();
    assert_eq!(rejections, 0);
}
