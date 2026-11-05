//! What a task logs ends up in its log.
//!
//! The only test in this binary, and it has to stay that way: it installs its
//! subscriber for one thread, while tracing caches per callsite, across the process,
//! whether a line is enabled. A test running alongside can leave a line cached as
//! disabled and lose it. The server installs its subscriber before anything runs.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

use cabinet_core::library::{self, NewRoot};
use cabinet_core::scan::ScanService;
use cabinet_core::tasks::{Kind, Scope, Services, Tasks, Trigger};
use cabinet_core::thumbnails::Thumbnailer;
use tracing_subscriber::layer::SubscriberExt;

fn has_ffmpeg() -> bool {
    let ok = |program: &str| {
        std::process::Command::new(program)
            .arg("-version")
            .output()
            .is_ok_and(|o| o.status.success())
    };
    ok("ffmpeg") && ok("ffprobe")
}

#[tokio::test]
async fn what_a_task_logs_ends_up_in_its_log() {
    if !has_ffmpeg() {
        eprintln!("skipping: ffmpeg not installed");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let pool = cabinet_core::db::open(&dir.path().join("cabinet.db"))
        .await
        .unwrap();
    let media = dir.path().join("media");
    fs::create_dir_all(&media).unwrap();
    let status = std::process::Command::new("ffmpeg")
        .args(["-nostdin", "-hide_banner", "-loglevel", "error", "-y"])
        .args([
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=320x180:rate=24:duration=1",
        ])
        .args([
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(media.join("clip.mp4"))
        .status()
        .unwrap();
    assert!(status.success());
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
    let scans = ScanService::new(Tasks::new(pool.clone(), Services::default()));
    scans.request(root, Trigger::Manual).await.unwrap();
    scans.wait_idle(root).await;

    let (layer, lines) = cabinet_core::tasks::log::layer();
    let _subscriber = tracing::subscriber::set_default(tracing_subscriber::registry().with(layer));
    cabinet_core::tasks::log::spawn_writer(pool.clone(), lines);
    tracing::warn!("not in any task, so in no task's log");

    // An ffmpeg that can't read anything, so the file is logged as a failure.
    let ffmpeg = dir.path().join("failing-ffmpeg");
    fs::write(
        &ffmpeg,
        "#!/bin/sh\necho 'moov atom not found' >&2\nexit 1\n",
    )
    .unwrap();
    fs::set_permissions(&ffmpeg, fs::Permissions::from_mode(0o755)).unwrap();
    let tasks = Tasks::new(
        pool.clone(),
        Services {
            thumbnailer: Some(Thumbnailer {
                ffmpeg,
                dir: dir.path().join("thumbnails"),
            }),
            ..Services::default()
        },
    );
    let scope = Scope::Library(library);
    tasks
        .request(Kind::Thumbnails, scope, Trigger::Added)
        .await
        .unwrap();
    tasks.wait_idle(Kind::Thumbnails, scope).await;
    let id: i64 = sqlx::query_scalar("SELECT id FROM task WHERE kind = 'thumbnails'")
        .fetch_one(&pool)
        .await
        .unwrap();

    let mut detail = tasks.detail(id).await.unwrap().unwrap();
    for _ in 0..40 {
        if detail.log.iter().any(|l| l.message == "task finished") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
        detail = tasks.detail(id).await.unwrap().unwrap();
    }
    let warning = detail
        .log
        .iter()
        .find(|l| l.level == "warn" && l.message == "no thumbnail")
        .expect("the failure is in the log");
    assert_eq!(warning.fields.as_ref().unwrap()["path"], "clip.mp4");
    assert!(detail.log.iter().any(|l| l.message == "task started"));
    assert!(!detail.log.iter().any(|l| l.message.contains("in no task")));

    let others: i64 = sqlx::query_scalar("SELECT count(*) FROM task_log WHERE task_id <> ?")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(others, 0);
}
