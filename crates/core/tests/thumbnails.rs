//! The thumbnails task and what it needs from the runner: the heavy lane, giving
//! way to playback, and cancelling.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use cabinet_core::library::{self, NewRoot};
use cabinet_core::scan::ScanService;
use cabinet_core::tasks::history::TaskFilter;
use cabinet_core::tasks::{Cancel, Kind, Scope, Services, Tasks, Trigger};
use cabinet_core::thumbnails::{self, Thumbnailer};
use sqlx::SqlitePool;
use tempfile::TempDir;

fn has_ffmpeg() -> bool {
    let ok = |program: &str| {
        std::process::Command::new(program)
            .arg("-version")
            .output()
            .is_ok_and(|o| o.status.success())
    };
    ok("ffmpeg") && ok("ffprobe")
}

/// A two-second clip: black in `black`, a test pattern otherwise.
fn make_clip(path: &Path, black: bool) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let source = if black {
        "color=c=black:size=320x180:rate=24:duration=2"
    } else {
        "testsrc2=size=320x180:rate=24:duration=2"
    };
    let status = std::process::Command::new("ffmpeg")
        .args(["-nostdin", "-hide_banner", "-loglevel", "error", "-y"])
        .args(["-f", "lavfi", "-i", source])
        .args([
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(path)
        .status()
        .unwrap();
    assert!(status.success());
}

/// An ffmpeg that fails for files with `bad` in their name, and otherwise runs
/// the real one, after `delay` seconds.
fn wrapped_ffmpeg(dir: &Path, delay: f64) -> PathBuf {
    let path = dir.join("wrapped-ffmpeg");
    fs::write(
        &path,
        format!(
            r#"#!/bin/sh
case "$*" in
  *bad*) echo "moov atom not found" >&2; exit 1 ;;
esac
sleep {delay}
exec ffmpeg "$@"
"#
        ),
    )
    .unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path
}

struct Fixture {
    dir: TempDir,
    pool: SqlitePool,
    library: i64,
    root: i64,
    media: PathBuf,
}

impl Fixture {
    async fn new() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let pool = cabinet_core::db::open(&dir.path().join("cabinet.db"))
            .await
            .unwrap();
        let media = dir.path().join("media");
        fs::create_dir_all(&media).unwrap();
        let library = library::create(&pool, "Clips", "folders").await.unwrap().id;
        let root = library::add_root(
            &pool,
            library,
            NewRoot {
                path: media.to_str().unwrap(),
                write_policy: "read_only",
                layout_template: "{video}",
                is_import_target: false,
            },
        )
        .await
        .unwrap()
        .id;
        Fixture {
            dir,
            pool,
            library,
            root,
            media,
        }
    }

    fn thumbnailer(&self, ffmpeg: PathBuf) -> Thumbnailer {
        Thumbnailer {
            ffmpeg,
            dir: self.dir.path().join("thumbnails"),
        }
    }

    /// Finds and links the clips, without a thumbnailer so nothing else starts.
    async fn scan(&self) {
        let scans = ScanService::new(Tasks::new(self.pool.clone(), Services::default()));
        scans.request(self.root, Trigger::Manual).await.unwrap();
        scans.wait_idle(self.root).await;
    }

    async fn run(&self, tasks: &Tasks, trigger: Trigger) {
        let scope = Scope::Library(self.library);
        tasks
            .request(Kind::Thumbnails, scope, trigger)
            .await
            .unwrap();
        tasks.wait_idle(Kind::Thumbnails, scope).await;
    }

    /// The latest thumbnails task: state, outcome, error.
    async fn last(&self) -> (String, serde_json::Value, Option<String>) {
        let (state, outcome, error): (String, Option<String>, Option<String>) = sqlx::query_as(
            "SELECT state, outcome, error FROM task WHERE kind = 'thumbnails' ORDER BY id DESC",
        )
        .fetch_one(&self.pool)
        .await
        .unwrap();
        let outcome = outcome.map_or(serde_json::Value::Null, |o| {
            serde_json::from_str(&o).unwrap()
        });
        (state, outcome, error)
    }

    async fn file_id(&self, name: &str) -> i64 {
        sqlx::query_scalar("SELECT id FROM media_file WHERE relative_path = ?")
            .bind(name)
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }

    async fn item_of(&self, file: i64) -> i64 {
        sqlx::query_scalar("SELECT item_id FROM file_link WHERE file_id = ?")
            .bind(file)
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }
}

fn services(thumbnailer: Thumbnailer) -> Services {
    Services {
        thumbnailer: Some(thumbnailer),
        ..Services::default()
    }
}

#[tokio::test]
async fn makes_a_thumbnail_once_and_again_when_the_file_changes() {
    if !has_ffmpeg() {
        eprintln!("skipping: ffmpeg not installed");
        return;
    }
    let f = Fixture::new().await;
    make_clip(&f.media.join("clip.mp4"), false);
    f.scan().await;
    let thumbnailer = f.thumbnailer("ffmpeg".into());
    let tasks = Tasks::new(f.pool.clone(), services(thumbnailer.clone()));

    assert!(thumbnails::has_work(&f.pool, f.library).await.unwrap());
    f.run(&tasks, Trigger::Added).await;
    let (state, outcome, error) = f.last().await;
    assert_eq!((state.as_str(), error), ("ok", None));
    assert_eq!(outcome["made"], 1);

    let file = f.file_id("clip.mp4").await;
    let bytes = fs::read(thumbnailer.path(file)).unwrap();
    assert_eq!(&bytes[8..12], b"WEBP", "a WebP image");
    let names: Vec<_> = fs::read_dir(&thumbnailer.dir).unwrap().collect();
    assert_eq!(names.len(), 1, "no temporary files left behind");
    let item = f.item_of(file).await;
    assert_eq!(
        thumbnails::for_item(&f.pool, item).await.unwrap(),
        Some(file)
    );

    // Running it again finds nothing to do.
    assert!(!thumbnails::has_work(&f.pool, f.library).await.unwrap());
    f.run(&tasks, Trigger::Manual).await;
    assert_eq!(f.last().await.1["made"], 0);

    // A changed file is due again.
    sqlx::query("UPDATE media_file SET mtime = mtime + 1 WHERE id = ?")
        .bind(file)
        .execute(&f.pool)
        .await
        .unwrap();
    assert!(thumbnails::has_work(&f.pool, f.library).await.unwrap());
    f.run(&tasks, Trigger::Added).await;
    assert_eq!(f.last().await.1["made"], 1);
}

#[tokio::test]
async fn a_scan_asks_for_thumbnails_of_what_it_finds() {
    if !has_ffmpeg() {
        eprintln!("skipping: ffmpeg not installed");
        return;
    }
    let f = Fixture::new().await;
    make_clip(&f.media.join("clip.mp4"), false);
    let tasks = Tasks::new(f.pool.clone(), services(f.thumbnailer("ffmpeg".into())));
    let scans = ScanService::new(tasks.clone());

    scans.request(f.root, Trigger::Manual).await.unwrap();
    scans.wait_idle(f.root).await;
    tasks
        .wait_idle(Kind::Thumbnails, Scope::Library(f.library))
        .await;
    assert_eq!(f.last().await.1["made"], 1);

    // Nothing new: the next scan doesn't ask again.
    scans.request(f.root, Trigger::Manual).await.unwrap();
    scans.wait_idle(f.root).await;
    let runs: i64 = sqlx::query_scalar("SELECT count(*) FROM task WHERE kind = 'thumbnails'")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(runs, 1);
}

#[tokio::test]
async fn items_with_provider_art_are_left_alone() {
    if !has_ffmpeg() {
        eprintln!("skipping: ffmpeg not installed");
        return;
    }
    let f = Fixture::new().await;
    make_clip(&f.media.join("clip.mp4"), false);
    f.scan().await;
    let item = f.item_of(f.file_id("clip.mp4").await).await;
    sqlx::query("INSERT INTO item_artwork (item_id, kind, provider, path) VALUES (?, 'poster', 'tmdb', '/p.jpg')")
        .bind(item)
        .execute(&f.pool)
        .await
        .unwrap();
    assert!(!thumbnails::has_work(&f.pool, f.library).await.unwrap());
}

#[tokio::test]
async fn an_episode_gets_a_frame_unless_the_provider_has_a_still() {
    if !has_ffmpeg() {
        eprintln!("skipping: ffmpeg not installed");
        return;
    }
    let f = Fixture::new().await;
    make_clip(&f.media.join("clip.mp4"), false);
    f.scan().await;
    let file = f.file_id("clip.mp4").await;
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "INSERT INTO item (id, kind, title, library_id) VALUES (900, 'series', 'Show', {library});
         INSERT INTO series (item_id) VALUES (900);
         INSERT INTO item (id, parent_id, library_id, kind, title)
             VALUES (901, 900, {library}, 'episode', 'One');
         INSERT INTO ordering (id, series_id, name, kind, source, is_default)
             VALUES (900, 900, 'Aired', 'aired', 'tmdb', 1);
         INSERT INTO ordering_season (id, ordering_id, number) VALUES (900, 900, 1);
         INSERT INTO ordering_entry (ordering_id, position, item_id, ordering_season_id, episode_number)
             VALUES (900, 1, 901, 900, 1);
         UPDATE file_link SET item_id = 901 WHERE file_id = {file};",
        library = f.library,
    )))
    .execute(&f.pool)
    .await
    .unwrap();

    let artwork = || async {
        let list =
            cabinet_core::catalog::episodes(&f.pool, 900, None, None, cabinet_core::catalog::now())
                .await
                .unwrap();
        list.entries[0].episode.artwork.clone()
    };
    let still = "INSERT INTO item_artwork (item_id, kind, provider, path)
                 VALUES (901, 'still', 'tmdb', '/s.jpg')";

    // With a provider still there's nothing to make.
    sqlx::query(still).execute(&f.pool).await.unwrap();
    assert!(!thumbnails::has_work(&f.pool, f.library).await.unwrap());
    assert_eq!(artwork().await, ["still"]);

    // Without one, the frame takes its place under the same name.
    sqlx::query("DELETE FROM item_artwork WHERE item_id = 901")
        .execute(&f.pool)
        .await
        .unwrap();
    assert!(artwork().await.is_empty());
    assert!(thumbnails::has_work(&f.pool, f.library).await.unwrap());
    let tasks = Tasks::new(f.pool.clone(), services(f.thumbnailer("ffmpeg".into())));
    f.run(&tasks, Trigger::Manual).await;
    assert_eq!(
        thumbnails::for_item(&f.pool, 901).await.unwrap(),
        Some(file)
    );
    assert_eq!(artwork().await, ["still"]);

    // Both at once still make one picture to the client.
    sqlx::query(still).execute(&f.pool).await.unwrap();
    assert_eq!(artwork().await, ["still"]);
}

#[tokio::test]
async fn a_dark_video_still_gets_a_thumbnail() {
    if !has_ffmpeg() {
        eprintln!("skipping: ffmpeg not installed");
        return;
    }
    let f = Fixture::new().await;
    make_clip(&f.media.join("dark.mp4"), true);
    f.scan().await;
    let tasks = Tasks::new(f.pool.clone(), services(f.thumbnailer("ffmpeg".into())));
    f.run(&tasks, Trigger::Added).await;
    assert_eq!(f.last().await.1["made"], 1);
}

#[tokio::test]
async fn a_bad_file_is_listed_and_only_retried_by_hand() {
    if !has_ffmpeg() {
        eprintln!("skipping: ffmpeg not installed");
        return;
    }
    let f = Fixture::new().await;
    make_clip(&f.media.join("good.mp4"), false);
    make_clip(&f.media.join("bad.mp4"), false);
    f.scan().await;
    let ffmpeg = wrapped_ffmpeg(f.dir.path(), 0.0);
    let tasks = Tasks::new(f.pool.clone(), services(f.thumbnailer(ffmpeg)));

    f.run(&tasks, Trigger::Added).await;
    let (state, outcome, error) = f.last().await;
    assert_eq!(state, "ok", "one bad file doesn't fail the run");
    assert_eq!(
        (&outcome["made"], &outcome["failed"]),
        (&1.into(), &1.into())
    );
    assert_eq!(outcome["failures"][0]["path"], "bad.mp4");
    assert!(
        outcome["failures"][0]["error"]
            .as_str()
            .unwrap()
            .contains("moov atom"),
        "{outcome}"
    );
    assert_eq!(error.unwrap(), "1 of 2 files got no thumbnail.");

    // A failed file has no thumbnail to show.
    let bad = f.item_of(f.file_id("bad.mp4").await).await;
    assert_eq!(thumbnails::for_item(&f.pool, bad).await.unwrap(), None);

    // Scans don't keep trying, but asking by hand does.
    assert!(!thumbnails::has_work(&f.pool, f.library).await.unwrap());
    f.run(&tasks, Trigger::Added).await;
    assert_eq!(f.last().await.1["failed"], 0);
    f.run(&tasks, Trigger::Manual).await;
    assert_eq!(f.last().await.1["failed"], 1);
}

#[tokio::test]
async fn missing_ffmpeg_stops_the_run_without_marking_files() {
    if !has_ffmpeg() {
        eprintln!("skipping: ffmpeg not installed");
        return;
    }
    let f = Fixture::new().await;
    make_clip(&f.media.join("clip.mp4"), false);
    f.scan().await;
    let tasks = Tasks::new(
        f.pool.clone(),
        services(f.thumbnailer("/nonexistent/ffmpeg".into())),
    );
    f.run(&tasks, Trigger::Added).await;
    let (state, _, error) = f.last().await;
    assert_eq!(state, "failed");
    assert!(error.unwrap().contains("/nonexistent/ffmpeg"));
    assert!(thumbnails::has_work(&f.pool, f.library).await.unwrap());
}

#[tokio::test]
async fn waits_while_anything_is_transcoding() {
    if !has_ffmpeg() {
        eprintln!("skipping: ffmpeg not installed");
        return;
    }
    let f = Fixture::new().await;
    make_clip(&f.media.join("clip.mp4"), false);
    f.scan().await;
    let busy = Arc::new(AtomicBool::new(true));
    let transcoding = busy.clone();
    let tasks = Tasks::new(
        f.pool.clone(),
        Services {
            transcoding: Arc::new(move || transcoding.load(Ordering::Relaxed)),
            ..services(f.thumbnailer("ffmpeg".into()))
        },
    );
    let scope = Scope::Library(f.library);
    tasks
        .request(Kind::Thumbnails, scope, Trigger::Added)
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    let running = tasks.running(Kind::Thumbnails, scope).unwrap();
    assert_eq!(running.progress.phase, "waiting for playback");
    assert_eq!(running.progress.done, 0);

    busy.store(false, Ordering::Relaxed);
    tasks.wait_idle(Kind::Thumbnails, scope).await;
    assert_eq!(f.last().await.1["made"], 1);
}

#[tokio::test]
async fn heavy_tasks_run_one_at_a_time_and_queued_ones_can_be_cancelled() {
    if !has_ffmpeg() {
        eprintln!("skipping: ffmpeg not installed");
        return;
    }
    let f = Fixture::new().await;
    make_clip(&f.media.join("clip.mp4"), false);
    f.scan().await;
    let other = library::create(&f.pool, "Other", "folders")
        .await
        .unwrap()
        .id;
    let busy = Arc::new(AtomicBool::new(true));
    let transcoding = busy.clone();
    let tasks = Tasks::new(
        f.pool.clone(),
        Services {
            transcoding: Arc::new(move || transcoding.load(Ordering::Relaxed)),
            ..services(f.thumbnailer("ffmpeg".into()))
        },
    );
    let first = Scope::Library(f.library);
    let second = Scope::Library(other);
    tasks
        .request(Kind::Thumbnails, first, Trigger::Added)
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    tasks
        .request(Kind::Thumbnails, second, Trigger::Manual)
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;

    let page = tasks
        .page(&TaskFilter {
            limit: 10,
            ..TaskFilter::default()
        })
        .await
        .unwrap();
    let state_of = |library: i64| {
        page.open
            .iter()
            .find(|t| t.library_id == Some(library))
            .unwrap()
            .clone()
    };
    let ahead = state_of(f.library);
    assert_eq!(ahead.state, "running");
    let waiting = state_of(other);
    assert_eq!(waiting.state, "queued", "waits for the heavy lane");

    // Cancelling it doesn't take waiting for the task ahead of it.
    assert_eq!(tasks.cancel(waiting.id).await.unwrap(), Cancel::Stopping);
    tasks.wait_idle(Kind::Thumbnails, second).await;
    let state: String = sqlx::query_scalar("SELECT state FROM task WHERE id = ?")
        .bind(waiting.id)
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(state, "cancelled");
    assert_eq!(tasks.cancel(waiting.id).await.unwrap(), Cancel::NotOpen);

    busy.store(false, Ordering::Relaxed);
    tasks.wait_idle(Kind::Thumbnails, first).await;
    let state: String = sqlx::query_scalar("SELECT state FROM task WHERE id = ?")
        .bind(ahead.id)
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(state, "ok");
}

#[tokio::test]
async fn a_running_task_stops_at_the_next_file_and_carries_on_later() {
    if !has_ffmpeg() {
        eprintln!("skipping: ffmpeg not installed");
        return;
    }
    let f = Fixture::new().await;
    for name in ["a.mp4", "b.mp4", "c.mp4"] {
        make_clip(&f.media.join(name), false);
    }
    f.scan().await;
    let ffmpeg = wrapped_ffmpeg(f.dir.path(), 0.3);
    let tasks = Tasks::new(f.pool.clone(), services(f.thumbnailer(ffmpeg)));
    let scope = Scope::Library(f.library);
    tasks
        .request(Kind::Thumbnails, scope, Trigger::Added)
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    let id = tasks.running(Kind::Thumbnails, scope).unwrap().task_id;
    assert_eq!(tasks.cancel(id).await.unwrap(), Cancel::Stopping);
    assert!(tasks.running(Kind::Thumbnails, scope).unwrap().cancelling);
    tasks.wait_idle(Kind::Thumbnails, scope).await;
    let (state, outcome, _) = f.last().await;
    assert_eq!(state, "cancelled");
    assert_eq!(outcome["made"], 1, "the file in progress finished");

    f.run(&tasks, Trigger::Added).await;
    assert_eq!(f.last().await.1["made"], 2);
}

#[tokio::test]
async fn a_running_scan_cant_be_stopped_but_its_queued_follow_up_can() {
    let f = Fixture::new().await;
    let tasks = Tasks::new(f.pool.clone(), Services::default());
    let scope = Scope::Root(f.root);
    tasks
        .request(Kind::Scan, scope, Trigger::Manual)
        .await
        .unwrap();
    tasks
        .request(Kind::Scan, scope, Trigger::Manual)
        .await
        .unwrap();
    let running = tasks.running(Kind::Scan, scope).unwrap();
    assert_eq!(
        tasks.cancel(running.task_id).await.unwrap(),
        Cancel::CantStop
    );

    let queued: i64 = sqlx::query_scalar("SELECT max(id) FROM task WHERE kind = 'scan'")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(tasks.cancel(queued).await.unwrap(), Cancel::Cancelled);
    tasks.wait_idle(Kind::Scan, scope).await;
    let states: Vec<String> =
        sqlx::query_scalar("SELECT state FROM task WHERE kind = 'scan' ORDER BY id")
            .fetch_all(&f.pool)
            .await
            .unwrap();
    assert_eq!(states.last().unwrap(), "cancelled");
}
