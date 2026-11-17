//! Imports, organising roots, and moving series, on real folders.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use cabinet_core::library::{self, NewRoot, RootChanges};
use cabinet_core::metadata::Provider;
use cabinet_core::metadata::fake::Catalog;
use cabinet_core::organize::{self, ImportOptions, Naming, OrganizeScope, TransferMode};
use cabinet_core::scan::Prober;
use sqlx::SqlitePool;
use tempfile::TempDir;

struct Lib {
    dir: TempDir,
    pool: SqlitePool,
    adaptive: i64,
    managed: i64,
    read_only: i64,
    series: i64,
}

const MANAGED: &str =
    "{series}/Season {season:00}/{series} - S{season:00}E{episode:00} - {title}.{ext}";

fn options(mode: TransferMode, replace: bool, dry_run: bool) -> ImportOptions {
    ImportOptions {
        mode,
        naming: None,
        replace,
        dry_run,
    }
}

fn prober() -> Prober {
    Prober {
        program: "/nonexistent/ffprobe".into(),
        ..Prober::default()
    }
}

impl Lib {
    async fn new() -> Lib {
        let dir = tempfile::tempdir().unwrap();
        let pool = cabinet_core::db::open(&dir.path().join("cabinet.db"))
            .await
            .unwrap();
        let tv = library::create(&pool, "TV", "tv").await.unwrap();
        let mut roots = Vec::new();
        for (name, policy, template, target) in [
            (
                "adaptive",
                "adaptive",
                "{series}/Season {season:00}/{filename}",
                true,
            ),
            ("managed", "managed", MANAGED, false),
            ("readonly", "read_only", "{series}/{filename}", false),
        ] {
            let path = dir.path().join(name);
            fs::create_dir_all(&path).unwrap();
            let root = library::add_root(
                &pool,
                tv.id,
                NewRoot {
                    path: path.to_str().unwrap(),
                    write_policy: policy,
                    read_only: None,
                    layout_template: template,
                    is_import_target: target,
                },
            )
            .await
            .unwrap();
            roots.push(root.id);
        }
        let provider = Provider::Fake(Arc::new(
            Catalog::from_json(include_str!("data/catalog.json")).unwrap(),
        ));
        let series =
            cabinet_core::items::add(&pool, &provider, "series", "99010", None, true, None)
                .await
                .unwrap()
                .item_id;
        Lib {
            pool,
            adaptive: roots[0],
            managed: roots[1],
            read_only: roots[2],
            series,
            dir,
        }
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.dir.path().join(rel)
    }

    fn download(&self, name: &str) -> PathBuf {
        let p = self.path(&format!("downloads/{name}"));
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, format!("video {name}").repeat(30)).unwrap();
        p
    }

    async fn episode(&self, season: i64, number: i64) -> i64 {
        sqlx::query_scalar(
            "SELECT oe.item_id FROM ordering_entry oe JOIN ordering o ON o.id = oe.ordering_id AND o.is_default
             JOIN ordering_season os ON os.id = oe.ordering_season_id WHERE os.number = ? AND oe.episode_number = ?",
        )
        .bind(season)
        .bind(number)
        .fetch_one(&self.pool)
        .await
        .unwrap()
    }

    /// Gives the adaptive root a layout that names files, as a managed one has.
    async fn name_files_on_adaptive(&self) {
        library::update_root(
            &self.pool,
            self.adaptive,
            RootChanges {
                layout_template: Some(MANAGED),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    }

    async fn import(
        &self,
        source: &Path,
        items: &[i64],
        mode: TransferMode,
        replace: bool,
    ) -> organize::ImportResult {
        organize::import_file(
            &self.pool,
            &prober(),
            source,
            items,
            options(mode, replace, false),
        )
        .await
        .unwrap()
    }
}

#[tokio::test]
async fn imports_hardlink_into_the_import_target_with_sidecars() {
    let lib = Lib::new().await;
    let source = lib.download("Night.Harbor.S01E01.1080p.WEB.mkv");
    fs::write(
        lib.path("downloads/Night.Harbor.S01E01.1080p.WEB.en.srt"),
        "subs",
    )
    .unwrap();
    let pilot = lib.episode(1, 1).await;

    let r = lib
        .import(&source, &[pilot], TransferMode::Hardlink, false)
        .await;

    assert_eq!(r.root_id, lib.adaptive);
    assert_eq!(
        r.path,
        "Night Harbor/Season 01/Night.Harbor.S01E01.1080p.WEB.mkv"
    );
    assert!(lib.path("adaptive").join(&r.path).exists());
    assert!(
        lib.path("adaptive/Night Harbor/Season 01/Night.Harbor.S01E01.1080p.WEB.en.srt")
            .exists()
    );
    assert!(source.exists(), "hard links leave the download for seeding");
    let (source_kind, pinned): (String, bool) =
        sqlx::query_as("SELECT source, pinned FROM file_link WHERE file_id = ?")
            .bind(r.file_id)
            .fetch_one(&lib.pool)
            .await
            .unwrap();
    assert_eq!((source_kind.as_str(), pinned), ("path_id", true));
    let wanted: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM item_wanted WHERE item_id = ?)")
            .bind(pilot)
            .fetch_one(&lib.pool)
            .await
            .unwrap();
    assert!(!wanted);
    let location: String =
        sqlx::query_scalar("SELECT path FROM series_location WHERE series_item_id = ?")
            .bind(lib.series)
            .fetch_one(&lib.pool)
            .await
            .unwrap();
    assert_eq!(location, "Night Harbor", "resolved once and stored");

    // The next episode goes next to its sibling; moving removes the download.
    let second = lib.download("nh.s01e02.mkv");
    let r2 = lib
        .import(
            &second,
            &[lib.episode(1, 2).await],
            TransferMode::Move,
            false,
        )
        .await;
    assert_eq!(r2.path, "Night Harbor/Season 01/nh.s01e02.mkv");
    assert!(!second.exists());

    // Never overwrite.
    let again = lib.download("nh.s01e02.mkv");
    let err = organize::import_file(
        &lib.pool,
        &prober(),
        &again,
        &[lib.episode(1, 2).await],
        options(TransferMode::Copy, false, false),
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("already exists"), "{err}");
}

#[tokio::test]
async fn dry_runs_change_nothing_and_replacing_removes_the_old_version() {
    let lib = Lib::new().await;
    let pilot = lib.episode(1, 1).await;
    let old = lib
        .import(
            &lib.download("pilot.720p.mkv"),
            &[pilot],
            TransferMode::Move,
            false,
        )
        .await;

    let source = lib.download("pilot.1080p.mkv");
    let preview = organize::import_file(
        &lib.pool,
        &prober(),
        &source,
        &[pilot],
        options(TransferMode::Move, true, true),
    )
    .await
    .unwrap();
    assert_eq!(preview.replaced.len(), 1);
    assert!(
        source.exists() && lib.path("adaptive").join(&old.path).exists(),
        "dry run"
    );

    lib.import(&source, &[pilot], TransferMode::Move, true)
        .await;
    assert!(!lib.path("adaptive").join(&old.path).exists());
    let status: String = sqlx::query_scalar("SELECT status FROM media_file WHERE id = ?")
        .bind(old.file_id)
        .fetch_one(&lib.pool)
        .await
        .unwrap();
    assert_eq!(status, "removed", "deleted on purpose: not missing");
    let kinds: Vec<String> = sqlx::query_scalar(
        "SELECT type FROM event WHERE type IN ('file.removed', 'file.upgraded')",
    )
    .fetch_all(&lib.pool)
    .await
    .unwrap();
    assert_eq!(kinds, ["file.removed", "file.upgraded"]);
}

#[tokio::test]
async fn read_only_roots_are_never_written() {
    let lib = Lib::new().await;
    sqlx::query(
        "INSERT INTO series_location (series_item_id, root_id, library_id, path, is_primary)
         SELECT ?1, ?2, library_id, 'Night Harbor', 1 FROM library_root WHERE id = ?2",
    )
    .bind(lib.series)
    .bind(lib.read_only)
    .execute(&lib.pool)
    .await
    .unwrap();
    let r = lib
        .import(
            &lib.download("nh.s01e03.mkv"),
            &[lib.episode(1, 3).await],
            TransferMode::Copy,
            false,
        )
        .await;
    assert_eq!(r.root_id, lib.adaptive);
    assert!(r.note.contains("read-only"), "{}", r.note);
    assert!(fs::read_dir(lib.path("readonly")).unwrap().next().is_none());
    let locations: i64 =
        sqlx::query_scalar("SELECT count(*) FROM series_location WHERE series_item_id = ?")
            .bind(lib.series)
            .fetch_one(&lib.pool)
            .await
            .unwrap();
    assert_eq!(locations, 2, "a second location on the import target");
}

#[tokio::test]
async fn organising_renames_managed_files_by_the_template() {
    let lib = Lib::new().await;
    // A badly named file already on the managed root, linked to S01E03.
    let messy = lib.path("managed/Night Harbor/misc/nh 1x03.mkv");
    fs::create_dir_all(messy.parent().unwrap()).unwrap();
    fs::write(&messy, "video").unwrap();
    fs::write(lib.path("managed/Night Harbor/misc/nh 1x03.en.srt"), "subs").unwrap();
    cabinet_core::scan::scan_root(&lib.pool, lib.managed)
        .await
        .unwrap();
    let file: i64 = sqlx::query_scalar("SELECT id FROM media_file WHERE root_id = ?")
        .bind(lib.managed)
        .fetch_one(&lib.pool)
        .await
        .unwrap();
    cabinet_core::matcher::review::link_file(&lib.pool, file, &[lib.episode(1, 3).await], false)
        .await
        .unwrap();

    let plan = organize::organize(&lib.pool, lib.series, true)
        .await
        .unwrap();
    assert_eq!(plan.moves.len(), 1);
    assert_eq!(
        plan.moves[0].to,
        "Night Harbor/Season 01/Night Harbor - S01E03 - Lighthouse.mkv"
    );
    assert!(messy.exists(), "preview only");

    let plan = organize::organize(&lib.pool, lib.series, false)
        .await
        .unwrap();
    assert!(plan.applied);
    assert!(
        lib.path("managed/Night Harbor/Season 01/Night Harbor - S01E03 - Lighthouse.mkv")
            .exists()
    );
    assert!(
        lib.path("managed/Night Harbor/Season 01/Night Harbor - S01E03 - Lighthouse.en.srt")
            .exists()
    );
    assert!(
        !lib.path("managed/Night Harbor/misc").exists(),
        "empty folder pruned"
    );
    let path: String = sqlx::query_scalar("SELECT relative_path FROM media_file WHERE id = ?")
        .bind(file)
        .fetch_one(&lib.pool)
        .await
        .unwrap();
    assert_eq!(
        path, plan.moves[0].to,
        "same row, so links and history stay"
    );
    assert!(
        organize::organize(&lib.pool, lib.series, true)
            .await
            .unwrap()
            .moves
            .is_empty()
    );
}

#[tokio::test]
async fn moving_a_series_to_another_root() {
    let lib = Lib::new().await;
    let r = lib
        .import(
            &lib.download("nh.s01e01.mkv"),
            &[lib.episode(1, 1).await],
            TransferMode::Move,
            false,
        )
        .await;

    let plan = organize::move_series(&lib.pool, lib.series, lib.managed, false)
        .await
        .unwrap();
    assert_eq!(plan.moves.len(), 1);
    assert!(
        lib.path("managed/Night Harbor/Season 01/nh.s01e01.mkv")
            .exists()
    );
    assert!(!lib.path("adaptive/Night Harbor").exists());
    let (root, primary): (i64, bool) =
        sqlx::query_as("SELECT root_id, is_primary FROM series_location WHERE series_item_id = ?")
            .bind(lib.series)
            .fetch_one(&lib.pool)
            .await
            .unwrap();
    assert_eq!(
        (root, primary),
        (lib.managed, true),
        "old location gone, new one primary"
    );
    let file_root: i64 = sqlx::query_scalar("SELECT root_id FROM media_file WHERE id = ?")
        .bind(r.file_id)
        .fetch_one(&lib.pool)
        .await
        .unwrap();
    assert_eq!(file_root, lib.managed);
    assert!(
        organize::move_series(&lib.pool, lib.series, lib.read_only, true)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn fixes_a_wrong_id_in_a_series_folder_name() {
    let lib = Lib::new().await;
    let source = lib.download("Night.Harbor.S01E01.mkv");
    let pilot = lib.episode(1, 1).await;
    let r = lib
        .import(&source, &[pilot], TransferMode::Hardlink, false)
        .await;
    // The folder had a wrong tag all along.
    fs::rename(
        lib.path("adaptive/Night Harbor"),
        lib.path("adaptive/Night Harbor [tmdb-123123]"),
    )
    .unwrap();
    sqlx::query(
        "UPDATE media_file SET relative_path = replace(relative_path, 'Night Harbor/', 'Night Harbor [tmdb-123123]/');
         UPDATE series_location SET path = 'Night Harbor [tmdb-123123]';",
    )
    .execute(&lib.pool)
    .await
    .unwrap();
    let value: String =
        sqlx::query_scalar("SELECT value FROM external_id WHERE item_id = ? AND provider = 'tmdb'")
            .bind(lib.series)
            .fetch_one(&lib.pool)
            .await
            .unwrap();

    let done = organize::fix_folder_ids(&lib.pool, lib.series, lib.adaptive)
        .await
        .unwrap();
    let fixed = format!("Night Harbor [tmdb-{value}]");
    assert_eq!(done.to, fixed);
    let path: String = sqlx::query_scalar("SELECT relative_path FROM media_file WHERE id = ?")
        .bind(r.file_id)
        .fetch_one(&lib.pool)
        .await
        .unwrap();
    assert_eq!(path, format!("{fixed}/Season 01/Night.Harbor.S01E01.mkv"));
    assert!(lib.path(&format!("adaptive/{path}")).is_file());
    let location: String =
        sqlx::query_scalar("SELECT path FROM series_location WHERE series_item_id = ?")
            .bind(lib.series)
            .fetch_one(&lib.pool)
            .await
            .unwrap();
    assert_eq!(location, fixed);
    assert!(
        organize::fix_folder_ids(&lib.pool, lib.series, lib.adaptive)
            .await
            .is_err(),
        "nothing left to fix"
    );
}

#[tokio::test]
async fn read_only_folders_are_never_renamed() {
    let lib = Lib::new().await;
    let wrong = "Night Harbor {tmdb-1}".to_string();
    fs::create_dir_all(lib.path(&format!("readonly/{wrong}"))).unwrap();
    sqlx::query(
        "INSERT INTO series_location (series_item_id, root_id, library_id, path, is_primary)
         SELECT ?, id, library_id, ?, 1 FROM library_root WHERE id = ?",
    )
    .bind(lib.series)
    .bind(&wrong)
    .bind(lib.read_only)
    .execute(&lib.pool)
    .await
    .unwrap();

    let err = organize::fix_folder_ids(&lib.pool, lib.series, lib.read_only)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("Turn read-only off"), "{err}");
    assert!(lib.path(&format!("readonly/{wrong}")).is_dir());

    // Once it's writable, it can be.
    library::update_root(
        &lib.pool,
        lib.read_only,
        RootChanges {
            read_only: Some(false),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let done = organize::fix_folder_ids(&lib.pool, lib.series, lib.read_only)
        .await
        .unwrap();
    assert!(lib.path(&format!("readonly/{}", done.to)).is_dir());
    assert!(!lib.path(&format!("readonly/{wrong}")).exists());
}

#[tokio::test]
async fn any_writable_root_is_organised() {
    let lib = Lib::new().await;
    let r = lib
        .import(
            &lib.download("nh.s01e01.mkv"),
            &[lib.episode(1, 1).await],
            TransferMode::Move,
            false,
        )
        .await;
    assert_eq!(r.root_id, lib.adaptive);
    assert_eq!(r.path, "Night Harbor/Season 01/nh.s01e01.mkv", "name kept");
    lib.name_files_on_adaptive().await;

    let plan = organize::organize(&lib.pool, lib.series, false)
        .await
        .unwrap();
    assert_eq!(plan.moves.len(), 1, "{:?}", plan.skipped);
    assert_eq!(
        plan.moves[0].to,
        "Night Harbor/Season 01/Night Harbor - S01E01 - Pilot.mkv"
    );
    assert!(lib.path("adaptive").join(&plan.moves[0].to).exists());
    assert!(
        !lib.path("adaptive/Night Harbor/Season 01/nh.s01e01.mkv")
            .exists()
    );
}

#[tokio::test]
async fn naming_is_chosen_per_import() {
    let lib = Lib::new().await;
    lib.name_files_on_adaptive().await;
    let renamed = organize::import_file(
        &lib.pool,
        &prober(),
        &lib.download("nh.s01e01.mkv"),
        &[lib.episode(1, 1).await],
        ImportOptions {
            naming: Some(Naming::Layout),
            ..options(TransferMode::Copy, false, true)
        },
    )
    .await
    .unwrap();
    assert_eq!(renamed.root_id, lib.adaptive);
    assert_eq!(
        renamed.path,
        "Night Harbor/Season 01/Night Harbor - S01E01 - Pilot.mkv"
    );

    let kept = organize::import_file(
        &lib.pool,
        &prober(),
        &lib.download("nh.s01e01.mkv"),
        &[lib.episode(1, 1).await],
        ImportOptions {
            naming: Some(Naming::Keep),
            ..options(TransferMode::Copy, false, true)
        },
    )
    .await
    .unwrap();
    assert_eq!(kept.path, "Night Harbor/Season 01/nh.s01e01.mkv");
}

#[tokio::test]
async fn previews_a_library_and_applies_what_was_chosen() {
    let lib = Lib::new().await;
    // Two badly named episodes on the managed root, and one on the read-only root.
    for (rel, episode) in [
        ("managed/Night Harbor/misc/nh 1x01.mkv", 1),
        ("managed/Night Harbor/misc/nh 1x02.mkv", 2),
        ("readonly/Night Harbor/nh 1x03.mkv", 3),
    ] {
        let p = lib.path(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, format!("video {rel}")).unwrap();
        let root = if rel.starts_with("managed") {
            lib.managed
        } else {
            lib.read_only
        };
        cabinet_core::scan::scan_root(&lib.pool, root)
            .await
            .unwrap();
        let file: i64 = sqlx::query_scalar(
            "SELECT id FROM media_file WHERE root_id = ? AND relative_path LIKE '%' || ?",
        )
        .bind(root)
        .bind(format!("1x0{episode}.mkv"))
        .fetch_one(&lib.pool)
        .await
        .unwrap();
        cabinet_core::matcher::review::link_file(
            &lib.pool,
            file,
            &[lib.episode(1, episode).await],
            false,
        )
        .await
        .unwrap();
    }
    let library: i64 = sqlx::query_scalar("SELECT library_id FROM library_root WHERE id = ?")
        .bind(lib.managed)
        .fetch_one(&lib.pool)
        .await
        .unwrap();

    let preview = organize::preview_organize(&lib.pool, OrganizeScope::Library(library))
        .await
        .unwrap();
    assert_eq!(preview.files, 3);
    let managed = preview
        .groups
        .iter()
        .find(|g| g.root_id == lib.managed)
        .unwrap();
    assert_eq!(managed.title, "Night Harbor");
    assert_eq!(managed.moves.len(), 2);
    let read_only = preview
        .groups
        .iter()
        .find(|g| g.root_id == lib.read_only)
        .unwrap();
    assert!(read_only.moves.is_empty());
    assert_eq!(read_only.skipped.len(), 1);
    assert!(
        lib.path("managed/Night Harbor/misc/nh 1x01.mkv").exists(),
        "preview only"
    );

    // One folder alone; a read-only one can't be.
    let root_only = organize::preview_organize(&lib.pool, OrganizeScope::Root(lib.managed))
        .await
        .unwrap();
    assert_eq!(root_only.groups.len(), 1);
    assert!(
        organize::preview_organize(&lib.pool, OrganizeScope::Root(lib.read_only))
            .await
            .is_err()
    );

    // Apply only the first; a stale or escaping move fails on its own.
    let chosen = managed.moves[0].clone();
    let stale = organize::PlannedMove {
        from: "elsewhere.mkv".into(),
        ..managed.moves[1].clone()
    };
    let escaping = organize::PlannedMove {
        to: "../outside.mkv".into(),
        ..managed.moves[1].clone()
    };
    let done = organize::apply_organize(&lib.pool, &[chosen.clone(), stale, escaping])
        .await
        .unwrap();
    assert_eq!(done.moved.len(), 1);
    assert_eq!(done.failed.len(), 2, "{:?}", done.failed);
    assert!(lib.path("managed").join(&chosen.to).exists());
    assert!(
        lib.path("managed/Night Harbor/misc/nh 1x02.mkv").exists(),
        "not chosen"
    );
    assert!(!lib.dir.path().join("outside.mkv").exists());

    let after = organize::preview_organize(&lib.pool, OrganizeScope::Root(lib.managed))
        .await
        .unwrap();
    assert_eq!((after.files, after.in_place), (2, 1));
}

#[tokio::test]
async fn the_inbox_imports_what_is_finished_and_recognised() {
    use cabinet_core::tasks::{Kind, Scope, Services, Tasks, Trigger};

    let lib = Lib::new().await;
    let library: i64 = sqlx::query_scalar("SELECT library_id FROM library_root WHERE id = ?")
        .bind(lib.adaptive)
        .fetch_one(&lib.pool)
        .await
        .unwrap();
    let inbox = lib.path("inbox");
    fs::create_dir_all(&inbox).unwrap();
    let long_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
    for (name, finished) in [
        ("Night.Harbor.S01E01.1080p.mkv", true),
        ("Some.Other.Show.S01E01.mkv", true),
        ("Night.Harbor.S01E02.mkv", false),
        ("Night.Harbor.S01E03.mkv.part", true),
    ] {
        let p = inbox.join(name);
        fs::write(&p, format!("video {name}").repeat(30)).unwrap();
        if finished {
            fs::File::options()
                .write(true)
                .open(&p)
                .unwrap()
                .set_modified(long_ago)
                .unwrap();
        }
    }

    // An inbox is a folder of its own.
    assert!(
        library::set_inbox(
            &lib.pool,
            library,
            Some(lib.path("adaptive").to_str().unwrap())
        )
        .await
        .is_err()
    );
    let set = library::set_inbox(&lib.pool, library, Some(inbox.to_str().unwrap()))
        .await
        .unwrap();
    assert!(set.inbox_path.is_some());
    assert!(
        cabinet_core::inbox::has_work(&lib.pool, library, 120)
            .await
            .unwrap()
    );

    let tasks = Tasks::new(
        lib.pool.clone(),
        Services {
            prober: prober(),
            ..Services::default()
        },
    );
    let run = || async {
        tasks
            .request(Kind::Inbox, Scope::Library(library), Trigger::Scheduled)
            .await
            .unwrap();
        tasks.wait_idle(Kind::Inbox, Scope::Library(library)).await;
    };
    run().await;

    let pilot = lib.episode(1, 1).await;
    let path: String = sqlx::query_scalar(
        "SELECT f.relative_path FROM file_link l JOIN media_file f ON f.id = l.file_id WHERE l.item_id = ?",
    )
    .bind(pilot)
    .fetch_one(&lib.pool)
    .await
    .unwrap();
    assert!(lib.path("adaptive").join(&path).exists(), "{path}");
    assert!(
        inbox.join("Night.Harbor.S01E01.1080p.mkv").exists(),
        "hard linked: the client keeps seeding"
    );
    let state = cabinet_core::inbox::state(&lib.pool, library)
        .await
        .unwrap();
    let held: Vec<&str> = state
        .attention
        .iter()
        .map(|f| f.relative_path.as_str())
        .collect();
    assert_eq!(held, ["Some.Other.Show.S01E01.mkv"]);
    assert!(state.attention[0].reason.contains("Not recognised"));
    let second: i64 = sqlx::query_scalar("SELECT count(*) FROM file_link WHERE item_id = ?")
        .bind(lib.episode(1, 2).await)
        .fetch_one(&lib.pool)
        .await
        .unwrap();
    assert_eq!(second, 0, "still changing, so left for later");

    // Nothing new: no task is asked for, and running one imports nothing twice.
    assert!(
        !cabinet_core::inbox::has_work(&lib.pool, library, 120)
            .await
            .unwrap()
    );
    run().await;
    let files: i64 = sqlx::query_scalar("SELECT count(*) FROM media_file")
        .fetch_one(&lib.pool)
        .await
        .unwrap();
    assert_eq!(files, 1);
}
