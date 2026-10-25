//! Scanner behaviour against a real filesystem.

use std::fs;
use std::path::{Path, PathBuf};

use cabinet_core::scan::{ScanError, ScanReport, scan_root};
use sqlx::SqlitePool;
use tempfile::TempDir;

struct Lib {
    _dir: TempDir,
    root: PathBuf,
    pool: SqlitePool,
}

impl Lib {
    async fn new() -> Lib {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("tv");
        fs::create_dir(&root).unwrap();
        let pool = cabinet_core::db::open(&dir.path().join("cabinet.db"))
            .await
            .unwrap();
        sqlx::query("INSERT INTO library (id, name, kind) VALUES (1, 'TV', 'tv')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO library_root (id, library_id, path, layout_template)
             VALUES (1, 1, ?, '{series}/Season {season:00}/{filename}')",
        )
        .bind(root.to_str().unwrap())
        .execute(&pool)
        .await
        .unwrap();
        Lib {
            _dir: dir,
            root,
            pool,
        }
    }

    /// Writes a file whose content is derived from `seed`, so equal seeds mean equal content.
    fn write(&self, rel: &str, seed: &str) {
        let path = self.root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, format!("video:{seed}").repeat(100)).unwrap();
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }

    async fn scan(&self) -> ScanReport {
        scan_root(&self.pool, 1).await.unwrap()
    }

    async fn file_id(&self, rel: &str) -> i64 {
        sqlx::query_scalar("SELECT id FROM media_file WHERE relative_path = ?")
            .bind(rel)
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }

    async fn status(&self, id: i64) -> String {
        sqlx::query_scalar("SELECT status FROM media_file WHERE id = ?")
            .bind(id)
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }

    async fn path_of(&self, id: i64) -> String {
        sqlx::query_scalar("SELECT relative_path FROM media_file WHERE id = ?")
            .bind(id)
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }

    async fn events(&self) -> Vec<String> {
        sqlx::query_scalar("SELECT type FROM event ORDER BY id")
            .fetch_all(&self.pool)
            .await
            .unwrap()
    }
}

#[tokio::test]
async fn discovers_videos_and_skips_everything_else() {
    let lib = Lib::new().await;
    lib.write("Show/Season 01/Show - S01E01.mkv", "1");
    lib.write("Show/Season 01/Show - S01E02.MP4", "2");
    lib.write("Show/Season 01/Show - S01E01.en.srt", "sub");
    lib.write("Show/Season 01/Show - S01E03.mkv.part", "3");
    lib.write("Show/tvshow.nfo", "nfo");
    lib.write("Show/.hidden.mkv", "4");
    lib.write("Show/@eaDir/thumb.mkv", "5");
    lib.write("#recycle/old.mkv", "6");
    fs::write(lib.path("Show/empty.mkv"), "").unwrap();

    let report = lib.scan().await;
    assert_eq!(report.added, 2);

    let paths: Vec<String> = sqlx::query_scalar("SELECT relative_path FROM media_file ORDER BY 1")
        .fetch_all(&lib.pool)
        .await
        .unwrap();
    assert_eq!(
        paths,
        [
            "Show/Season 01/Show - S01E01.mkv",
            "Show/Season 01/Show - S01E02.MP4"
        ]
    );
    assert_eq!(lib.events().await, ["file.added", "file.added"]);
}

#[tokio::test]
async fn rescanning_unchanged_files_changes_nothing() {
    let lib = Lib::new().await;
    lib.write("a.mkv", "a");
    lib.write("b.mkv", "b");
    lib.scan().await;

    let report = lib.scan().await;
    assert_eq!(
        report,
        ScanReport {
            unchanged: 2,
            ..ScanReport::default()
        }
    );
    assert_eq!(lib.events().await.len(), 2);
}

#[tokio::test]
async fn renamed_file_keeps_its_row_and_links() {
    let lib = Lib::new().await;
    lib.write("Show/e01.mkv", "1");
    lib.scan().await;
    let id = lib.file_id("Show/e01.mkv").await;
    sqlx::raw_sql(
        "INSERT INTO item (id, kind, title) VALUES (1, 'series', 'Show');
         INSERT INTO item (id, parent_id, kind, title) VALUES (2, 1, 'episode', 'Pilot');",
    )
    .execute(&lib.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO file_link (file_id, item_id, confidence, source) VALUES (?, 2, 1, 'user')",
    )
    .bind(id)
    .execute(&lib.pool)
    .await
    .unwrap();

    fs::create_dir(lib.path("Show/Season 01")).unwrap();
    fs::rename(
        lib.path("Show/e01.mkv"),
        lib.path("Show/Season 01/Show - S01E01.mkv"),
    )
    .unwrap();
    let report = lib.scan().await;

    assert_eq!(report.moved, 1);
    assert_eq!(report.added + report.missing, 0);
    assert_eq!(lib.path_of(id).await, "Show/Season 01/Show - S01E01.mkv");
    let linked: i64 = sqlx::query_scalar("SELECT item_id FROM file_link WHERE file_id = ?")
        .bind(id)
        .fetch_one(&lib.pool)
        .await
        .unwrap();
    assert_eq!(linked, 2);
    assert_eq!(lib.events().await.last().unwrap(), "file.moved");
}

#[tokio::test]
async fn swapped_names_follow_the_files() {
    let lib = Lib::new().await;
    lib.write("e01.mkv", "first");
    lib.write("e02.mkv", "second");
    lib.scan().await;
    let first = lib.file_id("e01.mkv").await;
    let second = lib.file_id("e02.mkv").await;

    fs::rename(lib.path("e01.mkv"), lib.path("tmp.mkv")).unwrap();
    fs::rename(lib.path("e02.mkv"), lib.path("e01.mkv")).unwrap();
    fs::rename(lib.path("tmp.mkv"), lib.path("e02.mkv")).unwrap();
    let report = lib.scan().await;

    assert_eq!(report.moved, 2);
    assert_eq!(lib.path_of(first).await, "e02.mkv");
    assert_eq!(lib.path_of(second).await, "e01.mkv");
}

#[tokio::test]
async fn copy_then_delete_is_recognised_by_content() {
    let lib = Lib::new().await;
    lib.write("old/e01.mkv", "1");
    lib.scan().await;
    let id = lib.file_id("old/e01.mkv").await;

    // A copy gets a new inode, like a move across devices.
    fs::create_dir(lib.path("new")).unwrap();
    fs::copy(lib.path("old/e01.mkv"), lib.path("new/e01.mkv")).unwrap();
    fs::remove_file(lib.path("old/e01.mkv")).unwrap();
    let report = lib.scan().await;

    assert_eq!(report.moved, 1);
    assert_eq!(lib.path_of(id).await, "new/e01.mkv");
}

#[tokio::test]
async fn deleted_file_goes_missing_and_comes_back() {
    let lib = Lib::new().await;
    lib.write("e01.mkv", "1");
    lib.write("e02.mkv", "2");
    lib.scan().await;
    let id = lib.file_id("e01.mkv").await;
    let backup = fs::read(lib.path("e01.mkv")).unwrap();

    fs::remove_file(lib.path("e01.mkv")).unwrap();
    let report = lib.scan().await;
    assert_eq!(report.missing, 1);
    assert_eq!(lib.status(id).await, "missing");

    // A second scan doesn't report it again.
    assert_eq!(lib.scan().await.missing, 0);

    fs::write(lib.path("e01.mkv"), backup).unwrap();
    let report = lib.scan().await;
    assert_eq!(report.restored, 1);
    assert_eq!(lib.status(id).await, "present");
    let events = lib.events().await;
    assert_eq!(events[2..], ["file.missing", "file.restored"]);
}

#[tokio::test]
async fn file_moved_onto_a_vanished_files_path_takes_it_over() {
    let lib = Lib::new().await;
    lib.write("e01.mkv", "old");
    lib.write("incoming/e01.mkv", "new");
    lib.scan().await;
    let old = lib.file_id("e01.mkv").await;
    let new = lib.file_id("incoming/e01.mkv").await;

    // mv incoming/e01.mkv e01.mkv: replaces the old file.
    fs::rename(lib.path("incoming/e01.mkv"), lib.path("e01.mkv")).unwrap();
    let report = lib.scan().await;

    assert_eq!((report.moved, report.missing), (1, 1));
    assert_eq!(lib.path_of(new).await, "e01.mkv");
    assert_eq!(lib.status(new).await, "present");
    assert_eq!(lib.status(old).await, "missing");
}

#[tokio::test]
async fn content_changed_in_place() {
    let lib = Lib::new().await;
    lib.write("e01.mkv", "1");
    lib.scan().await;
    let id = lib.file_id("e01.mkv").await;
    let before: String = sqlx::query_scalar("SELECT content_hash FROM media_file WHERE id = ?")
        .bind(id)
        .fetch_one(&lib.pool)
        .await
        .unwrap();

    fs::write(lib.path("e01.mkv"), "a different, longer video").unwrap();
    let report = lib.scan().await;

    assert_eq!(report.changed, 1);
    let after: String = sqlx::query_scalar("SELECT content_hash FROM media_file WHERE id = ?")
        .bind(id)
        .fetch_one(&lib.pool)
        .await
        .unwrap();
    assert_ne!(before, after);
}

#[tokio::test]
async fn unreachable_root_changes_nothing() {
    let lib = Lib::new().await;
    lib.write("e01.mkv", "1");
    lib.scan().await;
    let id = lib.file_id("e01.mkv").await;

    fs::rename(&lib.root, lib.root.with_file_name("tv-offline")).unwrap();
    let err = scan_root(&lib.pool, 1).await.unwrap_err();

    assert!(matches!(err, ScanError::RootUnreachable { .. }), "{err}");
    assert_eq!(lib.status(id).await, "present");
}

#[tokio::test]
async fn empty_root_with_known_files_is_treated_as_unmounted() {
    let lib = Lib::new().await;
    lib.write("e01.mkv", "1");
    lib.scan().await;
    let id = lib.file_id("e01.mkv").await;

    fs::remove_file(lib.path("e01.mkv")).unwrap();
    let err = scan_root(&lib.pool, 1).await.unwrap_err();

    assert!(
        matches!(err, ScanError::RootEmpty { known: 1, .. }),
        "{err}"
    );
    assert_eq!(lib.status(id).await, "present");
}

#[tokio::test]
async fn unreadable_folder_marks_nothing_missing() {
    let lib = Lib::new().await;
    lib.write("locked/e01.mkv", "1");
    lib.write("open/e02.mkv", "2");
    lib.scan().await;
    let locked = lib.file_id("locked/e01.mkv").await;

    let _guard = Locked::new(&lib.path("locked"));
    if fs::read_dir(lib.path("locked")).is_ok() {
        eprintln!("skipping: running with permissions that bypass chmod");
        return;
    }
    let report = lib.scan().await;

    assert_eq!(report.unreadable, 1);
    assert_eq!(report.missing, 0);
    assert_eq!(lib.status(locked).await, "present");
}

/// Removes all permissions from a folder, restoring them on drop so cleanup works.
struct Locked(PathBuf);

impl Locked {
    fn new(path: &Path) -> Locked {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o000)).unwrap();
        Locked(path.to_path_buf())
    }
}

impl Drop for Locked {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o755));
    }
}
