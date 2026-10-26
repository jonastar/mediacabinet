//! Imports, organising managed roots, and moving series, on real folders.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use cabinet_core::library::{self, NewRoot};
use cabinet_core::metadata::Provider;
use cabinet_core::metadata::fake::Catalog;
use cabinet_core::organize::{self, TransferMode};
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
        let series = cabinet_core::items::add(&pool, &provider, "series", "99010", true, None)
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

    async fn import(
        &self,
        source: &Path,
        items: &[i64],
        mode: TransferMode,
        replace: bool,
    ) -> organize::ImportResult {
        organize::import_file(&self.pool, &prober(), source, items, mode, replace, false)
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
        TransferMode::Copy,
        false,
        false,
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
        TransferMode::Move,
        true,
        true,
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
    sqlx::query("INSERT INTO series_location (series_item_id, root_id, path, is_primary) VALUES (?, ?, 'Night Harbor', 1)")
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
