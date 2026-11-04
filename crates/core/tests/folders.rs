//! Folder libraries: scanning real folders with no metadata provider.

use std::fs;
use std::path::Path;
use std::sync::Arc;

use cabinet_core::catalog::{self, Availability, CatalogError, folders, movies};
use cabinet_core::library::{self, NewRoot, RootChanges};
use cabinet_core::matcher::{MatchSettings, match_root};
use cabinet_core::metadata::Provider;
use cabinet_core::metadata::fake::Catalog;
use cabinet_core::scan::scan_root;
use sqlx::SqlitePool;
use tempfile::TempDir;

struct Lib {
    dir: TempDir,
    pool: SqlitePool,
    library: i64,
    root: i64,
    /// Knows nothing, so anything matched by asking it would stay unmatched.
    provider: Provider,
}

fn write(root: &Path, relative: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, relative.as_bytes()).unwrap();
}

impl Lib {
    async fn new(files: &[&str]) -> Lib {
        let dir = tempfile::tempdir().unwrap();
        let pool = cabinet_core::db::open(&dir.path().join("cabinet.db"))
            .await
            .unwrap();
        let videos = dir.path().join("videos");
        fs::create_dir_all(&videos).unwrap();
        for f in files {
            write(&videos, f);
        }
        let library = library::create(&pool, "Clips", "folders").await.unwrap().id;
        let root = library::add_root(
            &pool,
            library,
            NewRoot {
                path: videos.to_str().unwrap(),
                write_policy: "read_only",
                layout_template: "{video}",
                is_import_target: false,
            },
        )
        .await
        .unwrap()
        .id;
        Lib {
            dir,
            pool,
            library,
            root,
            provider: Provider::Fake(Arc::new(Catalog::default())),
        }
    }

    fn videos(&self) -> std::path::PathBuf {
        self.dir.path().join("videos")
    }

    async fn scan(&self) -> cabinet_core::matcher::MatchReport {
        scan_root(&self.pool, self.root).await.unwrap();
        match_root(
            &self.pool,
            self.root,
            &self.provider,
            MatchSettings::default(),
        )
        .await
        .unwrap()
    }

    async fn browse(&self, path: &str) -> folders::FolderPage {
        folders::browse(&self.pool, self.library, path, None)
            .await
            .unwrap()
    }
}

fn titles(page: &folders::FolderPage) -> Vec<&str> {
    let mut titles: Vec<&str> = page
        .videos
        .iter()
        .map(|v| v.summary.title.as_str())
        .collect();
    titles.sort();
    titles
}

#[tokio::test]
async fn every_video_becomes_an_item_named_after_its_file() {
    // Names the matcher would otherwise skip as extras, or read as a real movie.
    let lib = Lib::new(&[
        "Top level.mp4",
        "trailer.mkv",
        "Holiday/2021/Beach day.mkv",
        "Holiday/2021/The.Matrix.1999.1080p.mkv",
        "Holiday/Packing.mp4",
    ])
    .await;
    let report = lib.scan().await;
    assert_eq!(report.linked, 5);
    assert_eq!(report.ignored, 0);
    assert_eq!(report.unsorted, 0);
    assert_eq!(report.provider_error, None);

    let review: i64 = sqlx::query_scalar("SELECT count(*) FROM match_candidate")
        .fetch_one(&lib.pool)
        .await
        .unwrap();
    assert_eq!(review, 0);
    let ids: i64 = sqlx::query_scalar("SELECT count(*) FROM external_id")
        .fetch_one(&lib.pool)
        .await
        .unwrap();
    assert_eq!(ids, 0);

    let top = lib.browse("").await;
    assert_eq!(titles(&top), ["Top level", "trailer"]);
    assert_eq!(top.folders.len(), 1);
    assert_eq!(top.folders[0].name, "Holiday");
    assert_eq!(top.folders[0].videos, 3);

    let holiday = lib.browse("Holiday").await;
    assert_eq!(titles(&holiday), ["Packing"]);
    assert_eq!(holiday.folders[0].path, "Holiday/2021");
    let year = lib.browse("/Holiday/2021/").await;
    assert_eq!(titles(&year), ["Beach day", "The.Matrix.1999.1080p"]);
    assert_eq!(year.videos[0].summary.availability, Availability::Available);
}

#[tokio::test]
async fn a_second_scan_adds_nothing_and_a_new_file_gets_an_item() {
    let lib = Lib::new(&["a.mkv"]).await;
    assert_eq!(lib.scan().await.linked, 1);
    assert_eq!(lib.scan().await.linked, 0);
    write(&lib.videos(), "b.mkv");
    assert_eq!(lib.scan().await.linked, 1);
    assert_eq!(titles(&lib.browse("").await), ["a", "b"]);
}

#[tokio::test]
async fn a_moved_file_keeps_its_item_and_takes_the_new_name() {
    let lib = Lib::new(&["old/name.mkv"]).await;
    lib.scan().await;
    let id = lib.browse("old").await.videos[0].summary.id;

    fs::create_dir_all(lib.videos().join("new")).unwrap();
    fs::rename(
        lib.videos().join("old/name.mkv"),
        lib.videos().join("new/renamed.mkv"),
    )
    .unwrap();
    lib.scan().await;

    let page = lib.browse("new").await;
    assert_eq!(page.videos[0].summary.id, id);
    assert_eq!(page.videos[0].summary.title, "renamed");
    assert_eq!(page.videos[0].file_name, "renamed.mkv");
    assert!(matches!(
        folders::browse(&lib.pool, lib.library, "old", None).await,
        Err(CatalogError::NotFound)
    ));
}

#[tokio::test]
async fn a_deleted_file_is_counted_as_missing_and_comes_back() {
    let lib = Lib::new(&["show/a.mkv", "show/b.mkv"]).await;
    lib.scan().await;
    let gone = lib.videos().join("show/a.mkv");
    let bytes = fs::read(&gone).unwrap();
    fs::remove_file(&gone).unwrap();
    lib.scan().await;

    let top = lib.browse("").await;
    assert_eq!(top.folders[0].videos, 1);
    assert_eq!(top.folders[0].missing, 1);
    let show = lib.browse("show").await;
    let missing = show.videos.iter().find(|v| v.summary.title == "a").unwrap();
    assert_eq!(missing.summary.availability, Availability::Missing);
    let id = missing.summary.id;

    fs::write(&gone, bytes).unwrap();
    lib.scan().await;
    let show = lib.browse("show").await;
    let back = show.videos.iter().find(|v| v.summary.id == id).unwrap();
    assert_eq!(back.summary.availability, Availability::Available);
    assert_eq!(show.videos.len(), 2);
}

#[tokio::test]
async fn a_locked_title_survives_scans() {
    let lib = Lib::new(&["clip.mkv"]).await;
    lib.scan().await;
    let id = lib.browse("").await.videos[0].summary.id;
    cabinet_core::items::edit(
        &lib.pool,
        id,
        cabinet_core::items::ItemEdit {
            title: Some("Graduation".into()),
            lock: true,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    lib.scan().await;
    assert_eq!(titles(&lib.browse("").await), ["Graduation"]);
}

#[tokio::test]
async fn a_folder_prefix_is_matched_exactly() {
    // `_` and `%` mean something to LIKE, and `Holiday` must not take in `Holiday 2`.
    let lib = Lib::new(&["Holiday/a.mkv", "Holiday 2/b.mkv", "100%_real/c.mkv"]).await;
    lib.scan().await;
    assert_eq!(titles(&lib.browse("Holiday").await), ["a"]);
    assert_eq!(titles(&lib.browse("100%_real").await), ["c"]);
    assert!(matches!(
        folders::browse(&lib.pool, lib.library, "100xxreal", None).await,
        Err(CatalogError::NotFound)
    ));
}

#[tokio::test]
async fn folders_count_watched_videos_per_viewer() {
    let lib = Lib::new(&["show/a.mkv", "show/b.mkv"]).await;
    lib.scan().await;
    let viewer = catalog::ensure_default_user(&lib.pool).await.unwrap();
    let item = lib.browse("show").await.videos[0].summary.id;
    cabinet_core::watch::set_played(
        &lib.pool,
        viewer,
        item,
        cabinet_core::watch::Scope::All,
        true,
        0,
    )
    .await
    .unwrap();
    let page = folders::browse(&lib.pool, lib.library, "", Some(viewer))
        .await
        .unwrap();
    assert_eq!(page.folders[0].videos, 2);
    assert_eq!(page.folders[0].watched, 1);
    assert_eq!(lib.browse("").await.folders[0].watched, 0);
}

#[tokio::test]
async fn a_folder_library_is_not_listed_with_movies_or_announced() {
    let lib = Lib::new(&["a.mkv"]).await;
    lib.scan().await;
    assert!(
        movies::list_movies(&lib.pool, None, None)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        movies::list_movies(&lib.pool, None, Some(lib.library))
            .await
            .unwrap()
            .len(),
        1
    );
    // Search still finds it.
    let found = movies::search(&lib.pool, "a", None, 0).await.unwrap();
    assert_eq!(found.movies.len(), 1);

    cabinet_core::activity::library::sync(&lib.pool)
        .await
        .unwrap();
    let entries: i64 = sqlx::query_scalar("SELECT count(*) FROM activity")
        .fetch_one(&lib.pool)
        .await
        .unwrap();
    assert_eq!(entries, 0);
}

#[tokio::test]
async fn only_folder_libraries_can_be_browsed() {
    let lib = Lib::new(&[]).await;
    assert!(lib.browse("").await.videos.is_empty());
    let movies = library::create(&lib.pool, "Films", "movies").await.unwrap();
    assert!(matches!(
        folders::browse(&lib.pool, movies.id, "", None).await,
        Err(CatalogError::NotFound)
    ));
}

#[tokio::test]
async fn folders_stay_read_only_with_no_layout() {
    let lib = Lib::new(&[]).await;
    for changes in [
        RootChanges {
            write_policy: Some("managed"),
            ..Default::default()
        },
        RootChanges {
            layout_template: Some("{video}/{filename}"),
            ..Default::default()
        },
        RootChanges {
            is_import_target: Some(true),
            ..Default::default()
        },
    ] {
        assert!(
            library::update_root(&lib.pool, lib.root, changes)
                .await
                .is_err()
        );
    }
    let other = lib.dir.path().join("other");
    fs::create_dir_all(&other).unwrap();
    let adaptive = NewRoot {
        path: other.to_str().unwrap(),
        write_policy: "adaptive",
        layout_template: "{video}",
        is_import_target: false,
    };
    assert!(
        library::add_root(&lib.pool, lib.library, adaptive)
            .await
            .is_err()
    );
}
