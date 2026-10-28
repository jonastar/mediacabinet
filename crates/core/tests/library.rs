//! Library and root management, and the scan service.

use std::fs;
use std::path::Path;
use std::time::Duration;

use cabinet_core::library::{self, LibraryError, NewRoot, RootChanges};
use cabinet_core::scan::{Prober, ScanService, Trigger};
use sqlx::SqlitePool;
use tempfile::TempDir;

async fn db() -> (TempDir, SqlitePool) {
    let dir = tempfile::tempdir().unwrap();
    let pool = cabinet_core::db::open(&dir.path().join("cabinet.db"))
        .await
        .unwrap();
    (dir, pool)
}

fn folder(base: &Path, name: &str, files: &[&str]) -> String {
    let root = base.join(name);
    for f in files {
        let path = root.join(f);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, format!("video {f}").repeat(50)).unwrap();
    }
    fs::create_dir_all(&root).unwrap();
    root.to_string_lossy().into_owned()
}

fn new_root(path: &str) -> NewRoot<'_> {
    NewRoot {
        path,
        write_policy: "adaptive",
        layout_template: "{series}/Season {season:00}/{filename}",
        is_import_target: false,
    }
}

fn message(err: LibraryError) -> String {
    err.to_string()
}

#[tokio::test]
async fn roots_must_not_overlap() {
    let (dir, pool) = db().await;
    let tv = library::create(&pool, "TV", "tv").await.unwrap();
    let outer = folder(dir.path(), "media", &[]);
    let inner = folder(dir.path(), "media/anime", &[]);
    let other = folder(dir.path(), "other", &[]);

    library::add_root(&pool, tv.id, new_root(&inner))
        .await
        .unwrap();
    let err = library::add_root(&pool, tv.id, new_root(&outer))
        .await
        .unwrap_err();
    assert!(matches!(err, LibraryError::Conflict(_)), "{err}");
    assert!(message(err).contains("overlaps"));
    let err = library::add_root(&pool, tv.id, new_root(&inner))
        .await
        .unwrap_err();
    assert!(message(err).contains("already in a library"));
    let err = library::add_root(&pool, tv.id, new_root("relative/path"))
        .await
        .unwrap_err();
    assert!(message(err).contains("full path"));
    let err = library::add_root(&pool, tv.id, new_root(&format!("{other}/nope")))
        .await
        .unwrap_err();
    assert!(message(err).contains("doesn't exist"));
    library::add_root(&pool, tv.id, new_root(&other))
        .await
        .unwrap();
}

#[tokio::test]
async fn templates_and_policies_are_validated() {
    let (dir, pool) = db().await;
    let tv = library::create(&pool, "TV", "tv").await.unwrap();
    let path = folder(dir.path(), "tv", &[]);

    let err = library::add_root(
        &pool,
        tv.id,
        NewRoot {
            layout_template: "{movie}/{filename}",
            ..new_root(&path)
        },
    )
    .await
    .unwrap_err();
    assert!(message(err).contains("{movie} doesn't belong"));

    let err = library::add_root(
        &pool,
        tv.id,
        NewRoot {
            write_policy: "read_only",
            is_import_target: true,
            ..new_root(&path)
        },
    )
    .await
    .unwrap_err();
    assert!(message(err).contains("read-only"));
}

#[tokio::test]
async fn one_import_target_per_library() {
    let (dir, pool) = db().await;
    let tv = library::create(&pool, "TV", "tv").await.unwrap();
    let a = folder(dir.path(), "a", &[]);
    let b = folder(dir.path(), "b", &[]);
    let a = library::add_root(
        &pool,
        tv.id,
        NewRoot {
            is_import_target: true,
            ..new_root(&a)
        },
    )
    .await
    .unwrap();
    let b = library::add_root(&pool, tv.id, new_root(&b)).await.unwrap();

    library::update_root(
        &pool,
        b.id,
        RootChanges {
            is_import_target: Some(true),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let roots = library::get(&pool, tv.id).await.unwrap().roots;
    let target: Vec<i64> = roots
        .iter()
        .filter(|r| r.is_import_target)
        .map(|r| r.id)
        .collect();
    assert_eq!(target, [b.id]);
    assert!(roots.iter().any(|r| r.id == a.id && !r.is_import_target));
}

#[tokio::test]
async fn removing_a_root_keeps_user_matches_safe() {
    let (dir, pool) = db().await;
    let tv = library::create(&pool, "TV", "tv").await.unwrap();
    let path = folder(dir.path(), "tv", &["Show/Season 01/e01.mkv"]);
    let root = library::add_root(&pool, tv.id, new_root(&path))
        .await
        .unwrap();
    cabinet_core::scan::scan_root(&pool, root.id).await.unwrap();
    sqlx::query(
        "INSERT INTO item (id, kind, title, library_id) VALUES (1, 'series', 'Show', ?1);
         INSERT INTO series (item_id) VALUES (1);
         INSERT INTO item (id, parent_id, library_id, kind, title) VALUES (2, 1, ?1, 'episode', 'Pilot');
         INSERT INTO file_link (file_id, item_id, library_id, confidence, source)
             SELECT id, 2, library_id, 1, 'user' FROM media_file;",
    )
    .bind(tv.id)
    .execute(&pool)
    .await
    .unwrap();

    let err = library::remove_root(&pool, root.id).await.unwrap_err();
    assert!(matches!(err, LibraryError::Conflict(_)), "{err}");
    let err = library::delete(&pool, tv.id).await.unwrap_err();
    assert!(message(err).contains("folders first"));

    sqlx::query("DELETE FROM file_link")
        .execute(&pool)
        .await
        .unwrap();
    library::remove_root(&pool, root.id).await.unwrap();
    let files: i64 = sqlx::query_scalar("SELECT count(*) FROM media_file")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(files, 0);
    let err = library::delete(&pool, tv.id).await.unwrap_err();
    assert!(message(err).contains("still holds 1"));
    sqlx::raw_sql("DELETE FROM item WHERE id = 2; DELETE FROM item WHERE id = 1;")
        .execute(&pool)
        .await
        .unwrap();
    library::delete(&pool, tv.id).await.unwrap();
}

#[tokio::test]
async fn placement_example_follows_the_stored_location() {
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

    let example = library::placement_example(&pool, 1).await.unwrap().unwrap();
    assert_eq!(example.series_title, "Kitsune Road");
    assert_eq!(example.episode_label, "S01E03");
    let paths: Vec<Option<&str>> = example
        .outcomes
        .iter()
        .map(|o| o.placement.path.as_deref())
        .collect();
    assert_eq!(
        paths,
        [
            Some("anime/Kitsune Road/Season 01/Kitsune.Road.S01E03.1080p.WEB-DL.mkv"),
            Some("anime/Kitsune Road/Season 01/Kitsune.Road.S01E03.1080p.WEB-DL.mkv"),
            None
        ]
    );
}

async fn scan_runs(pool: &SqlitePool) -> Vec<(String, String, Option<String>)> {
    sqlx::query_as("SELECT trigger, state, error FROM scan_run ORDER BY id")
        .fetch_all(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn the_service_records_runs_and_offline_roots() {
    let (dir, pool) = db().await;
    let tv = library::create(&pool, "TV", "tv").await.unwrap();
    let path = folder(
        dir.path(),
        "tv",
        &["Show/Season 01/e01.mkv", "Show/Season 01/e02.mkv"],
    );
    let root = library::add_root(&pool, tv.id, new_root(&path))
        .await
        .unwrap();
    let prober = Prober {
        program: "/nonexistent/ffprobe".into(),
        ..Prober::default()
    };
    let scans = ScanService::new(pool.clone(), prober);

    assert!(scans.request(root.id, Trigger::Added));
    // A second request while running is folded into one more scan.
    assert!(!scans.request(root.id, Trigger::Manual));
    assert!(!scans.request(root.id, Trigger::Manual));
    scans.wait_idle(root.id).await;

    let runs = scan_runs(&pool).await;
    assert_eq!(runs.len(), 2);
    assert_eq!((runs[0].0.as_str(), runs[0].1.as_str()), ("added", "ok"));
    assert!(
        runs[0].2.as_deref().unwrap().contains("not probed"),
        "missing ffprobe is noted"
    );
    let root_now = library::root(&pool, root.id).await.unwrap();
    assert_eq!(root_now.files.present, 2);

    // Unmounted: the scan fails, nothing goes missing, and the root is offline.
    fs::rename(&path, format!("{path}-away")).unwrap();
    scans.request(root.id, Trigger::Manual);
    scans.wait_idle(root.id).await;
    let offline = library::root(&pool, root.id).await.unwrap();
    assert!(offline.offline_since.is_some() && !offline.online);
    assert_eq!((offline.files.present, offline.files.missing), (2, 0));
    assert_eq!(offline.last_scan.unwrap().state, "failed");

    fs::rename(format!("{path}-away"), &path).unwrap();
    scans.request(root.id, Trigger::Manual);
    scans.wait_idle(root.id).await;
    assert!(
        library::root(&pool, root.id)
            .await
            .unwrap()
            .offline_since
            .is_none()
    );
}

#[tokio::test]
async fn interrupted_runs_are_marked_failed() {
    let (dir, pool) = db().await;
    let tv = library::create(&pool, "TV", "tv").await.unwrap();
    let path = folder(dir.path(), "tv", &[]);
    let root = library::add_root(&pool, tv.id, new_root(&path))
        .await
        .unwrap();
    sqlx::query("INSERT INTO scan_run (root_id, trigger) VALUES (?, 'manual')")
        .bind(root.id)
        .execute(&pool)
        .await
        .unwrap();

    ScanService::new(pool.clone(), Prober::default())
        .recover()
        .await
        .unwrap();

    let runs = scan_runs(&pool).await;
    assert_eq!(runs[0].1, "failed");
    assert!(runs[0].2.as_deref().unwrap().contains("Interrupted"));
}

#[tokio::test]
async fn the_watcher_rescans_after_changes() {
    let (dir, pool) = db().await;
    let tv = library::create(&pool, "TV", "tv").await.unwrap();
    let path = folder(dir.path(), "tv", &["Show/Season 01/e01.mkv"]);
    let root = library::add_root(&pool, tv.id, new_root(&path))
        .await
        .unwrap();
    let prober = Prober {
        program: "/nonexistent/ffprobe".into(),
        ..Prober::default()
    };
    let scans = ScanService::new(pool.clone(), prober);
    scans.request(root.id, Trigger::Added);
    scans.wait_idle(root.id).await;

    let watcher = scans.spawn_watcher(Duration::from_millis(200));
    tokio::time::sleep(Duration::from_millis(500)).await;
    fs::write(
        Path::new(&path).join("Show/Season 01/e02.mkv"),
        "new video".repeat(50),
    )
    .unwrap();

    let mut present = 0;
    for _ in 0..100 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        scans.wait_idle(root.id).await;
        present = library::root(&pool, root.id).await.unwrap().files.present;
        if present == 2 {
            break;
        }
    }
    watcher.abort();
    assert_eq!(present, 2);
    assert!(scan_runs(&pool).await.iter().any(|r| r.0 == "watch"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_scans_of_several_roots_dont_lock_each_other_out() {
    let (dir, pool) = db().await;
    let tv = library::create(&pool, "TV", "tv").await.unwrap();
    let prober = Prober {
        program: "/nonexistent/ffprobe".into(),
        ..Prober::default()
    };
    let catalog =
        cabinet_core::metadata::fake::Catalog::from_json(include_str!("data/catalog.json"))
            .unwrap();
    let provider = cabinet_core::metadata::Provider::Fake(std::sync::Arc::new(catalog));
    let scans = ScanService::with_matching(pool.clone(), prober, provider, Default::default());
    // Each root holds a different catalog series, so imports run side by side.
    let shows = [
        "Night Harbor (2019)",
        "Glass Orchard (2021)",
        "Lantern Street (2021)",
        "Paper Moons (2008)",
    ];
    let mut roots = Vec::new();
    for (r, show) in shows.iter().enumerate() {
        let files: Vec<String> = (1..=6)
            .map(|e| format!("{show}/Season 01/{show} - S01E{e:02}.mkv"))
            .collect();
        let refs: Vec<&str> = files.iter().map(String::as_str).collect();
        let path = folder(dir.path(), &format!("root{r}"), &refs);
        roots.push(
            library::add_root(&pool, tv.id, new_root(&path))
                .await
                .unwrap()
                .id,
        );
    }

    for id in &roots {
        scans.request(*id, Trigger::Manual);
    }
    for id in &roots {
        scans.wait_idle(*id).await;
    }

    for (trigger, state, error) in scan_runs(&pool).await {
        assert_eq!(state, "ok", "{trigger}: {error:?}");
        // ffprobe is missing on purpose; any other problem is a failure.
        let error = error.unwrap_or_default();
        assert!(error.is_empty() || error.contains("not probed"), "{error}");
    }
    let links: i64 = sqlx::query_scalar("SELECT count(*) FROM file_link")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(links, 24);
}
