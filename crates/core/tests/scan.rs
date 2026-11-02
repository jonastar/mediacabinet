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
        "INSERT INTO item (id, kind, title, library_id)
             SELECT 1, 'series', 'Show', library_id FROM library_root LIMIT 1;
         INSERT INTO series (item_id) VALUES (1);
         INSERT INTO item (id, parent_id, library_id, kind, title)
             SELECT 2, 1, library_id, 'episode', 'Pilot' FROM item WHERE id = 1;",
    )
    .execute(&lib.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO file_link (file_id, item_id, library_id, confidence, source)
         SELECT id, 2, library_id, 1, 'user' FROM media_file WHERE id = ?",
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

async fn sidecars(lib: &Lib) -> Vec<(String, String, Option<String>, bool)> {
    sqlx::query_as(
        "SELECT s.relative_path, f.relative_path, s.language, s.is_forced
         FROM sidecar s JOIN media_file f ON f.id = s.file_id
         ORDER BY s.relative_path",
    )
    .fetch_all(&lib.pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn sidecars_follow_the_files_on_disk() {
    let lib = Lib::new().await;
    lib.write("S1/e01.mkv", "1");
    lib.write("S1/e01.en.srt", "sub");
    lib.write("S1/e01.fr.forced.srt", "sub");
    lib.write("S1/tvshow.nfo", "nfo");
    assert_eq!(lib.scan().await.sidecars, 2);
    assert_eq!(
        sidecars(&lib).await,
        [
            (
                "S1/e01.en.srt".into(),
                "S1/e01.mkv".into(),
                Some("eng".into()),
                false
            ),
            (
                "S1/e01.fr.forced.srt".into(),
                "S1/e01.mkv".into(),
                Some("fre".into()),
                true
            ),
        ]
    );

    fs::remove_file(lib.path("S1/e01.fr.forced.srt")).unwrap();
    lib.scan().await;
    assert_eq!(sidecars(&lib).await.len(), 1);

    // Renaming the video and its subtitle together keeps them attached.
    fs::rename(lib.path("S1/e01.mkv"), lib.path("S1/Show - S01E01.mkv")).unwrap();
    fs::rename(
        lib.path("S1/e01.en.srt"),
        lib.path("S1/Show - S01E01.en.srt"),
    )
    .unwrap();
    lib.scan().await;
    assert_eq!(
        sidecars(&lib).await,
        [(
            "S1/Show - S01E01.en.srt".into(),
            "S1/Show - S01E01.mkv".into(),
            Some("eng".into()),
            false
        )]
    );
}

#[tokio::test]
async fn stored_probe_lists_streams_in_order_with_sidecars() {
    let lib = Lib::new().await;
    lib.write("S1/e01.mkv", "1");
    lib.write("S1/e01.en.srt", "sub");
    lib.scan().await;
    let id = lib.file_id("S1/e01.mkv").await;

    let probe = cabinet_core::scan::stored_probe(&lib.pool, id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(probe.probed_at, None);
    assert!(probe.streams.is_empty());

    sqlx::query(
        "INSERT INTO media_stream (file_id, stream_index, kind, codec, language, channels, bitrate, is_default)
         VALUES (?, 1, 'audio', 'eac3', 'jpn', 6, 640000, 1), (?, 0, 'video', 'hevc', NULL, NULL, NULL, 1)",
    )
    .bind(id)
    .bind(id)
    .execute(&lib.pool)
    .await
    .unwrap();
    let probe = cabinet_core::scan::stored_probe(&lib.pool, id)
        .await
        .unwrap()
        .unwrap();
    let streams: Vec<_> = probe
        .streams
        .iter()
        .map(|s| (s.index, s.kind.as_str(), s.codec.as_deref(), s.bitrate))
        .collect();
    assert_eq!(
        streams,
        [
            (0, "video", Some("hevc"), None),
            (1, "audio", Some("eac3"), Some(640000)),
        ]
    );
    assert!(probe.streams[1].is_default);
    let sidecars: Vec<_> = probe
        .sidecars
        .iter()
        .map(|s| {
            (
                s.kind.as_str(),
                s.relative_path.as_str(),
                s.language.as_deref(),
            )
        })
        .collect();
    assert_eq!(sidecars, [("subtitle", "S1/e01.en.srt", Some("eng"))]);

    assert!(
        cabinet_core::scan::stored_probe(&lib.pool, 999)
            .await
            .unwrap()
            .is_none()
    );
}

fn has_ffmpeg() -> bool {
    let ok = |program: &str| {
        std::process::Command::new(program)
            .arg("-version")
            .output()
            .is_ok_and(|o| o.status.success())
    };
    ok("ffmpeg") && ok("ffprobe")
}

/// A one-second clip with H.264 video and stereo AAC tagged as Japanese.
fn make_clip(path: &Path, tone: u32) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let status = std::process::Command::new("ffmpeg")
        .args(["-nostdin", "-hide_banner", "-loglevel", "error", "-y"])
        .args([
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=320x180:rate=24:duration=1",
        ])
        .args([
            "-f",
            "lavfi",
            "-i",
            &format!("sine=frequency={tone}:duration=1"),
        ])
        .args([
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-pix_fmt",
            "yuv420p",
        ])
        .args(["-c:a", "aac", "-ac", "2", "-metadata:s:a:0", "language=jpn"])
        .arg(path)
        .status()
        .unwrap();
    assert!(status.success());
}

/// (kind, codec, width or channels, language)
type StreamRow = (String, Option<String>, Option<i64>, Option<String>);

#[tokio::test]
async fn probes_new_and_changed_files() {
    if !has_ffmpeg() {
        eprintln!("skipping: ffmpeg not installed");
        return;
    }
    let lib = Lib::new().await;
    make_clip(&lib.path("e01.mkv"), 300);
    lib.write("broken.mkv", "not a video");
    lib.scan().await;
    let prober = cabinet_core::scan::Prober::default();

    let report = cabinet_core::scan::probe_root(&lib.pool, 1, &prober)
        .await
        .unwrap();
    assert_eq!((report.probed, report.failed), (1, 1));

    let id = lib.file_id("e01.mkv").await;
    let streams: Vec<StreamRow> = sqlx::query_as(
        "SELECT kind, codec, coalesce(width, channels), language
         FROM media_stream WHERE file_id = ? ORDER BY stream_index",
    )
    .bind(id)
    .fetch_all(&lib.pool)
    .await
    .unwrap();
    assert_eq!(
        streams,
        [
            ("video".into(), Some("h264".into()), Some(320), None),
            (
                "audio".into(),
                Some("aac".into()),
                Some(2),
                Some("jpn".into())
            ),
        ]
    );
    let (duration, error): (f64, Option<String>) =
        sqlx::query_as("SELECT duration_seconds, probe_error FROM media_file WHERE id = ?")
            .bind(id)
            .fetch_one(&lib.pool)
            .await
            .unwrap();
    assert!((duration - 1.0).abs() < 0.1, "{duration}");
    assert_eq!(error, None);
    let broken: Option<String> =
        sqlx::query_scalar("SELECT probe_error FROM media_file WHERE relative_path = 'broken.mkv'")
            .fetch_one(&lib.pool)
            .await
            .unwrap();
    assert!(broken.is_some());

    // Nothing pending, so nothing probed again.
    let report = cabinet_core::scan::probe_root(&lib.pool, 1, &prober)
        .await
        .unwrap();
    assert_eq!((report.probed, report.failed), (0, 0));

    // New content at the same path is probed again.
    make_clip(&lib.path("e01.mkv"), 400);
    assert_eq!(lib.scan().await.changed, 1);
    let report = cabinet_core::scan::probe_root(&lib.pool, 1, &prober)
        .await
        .unwrap();
    assert_eq!(report.probed, 1);
}

#[tokio::test]
async fn missing_ffprobe_fails_without_marking_files() {
    let lib = Lib::new().await;
    lib.write("e01.mkv", "1");
    lib.scan().await;
    let prober = cabinet_core::scan::Prober {
        program: "/nonexistent/ffprobe".into(),
        ..Default::default()
    };

    let err = cabinet_core::scan::probe_root(&lib.pool, 1, &prober)
        .await
        .unwrap_err();

    assert!(
        matches!(err, cabinet_core::scan::ProbeRootError::Unavailable(_)),
        "{err}"
    );
    let probed: Option<i64> = sqlx::query_scalar("SELECT probed_at FROM media_file")
        .fetch_one(&lib.pool)
        .await
        .unwrap();
    assert_eq!(probed, None);
}
