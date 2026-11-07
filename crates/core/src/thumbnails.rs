//! Thumbnails: a frame from each video, for movies, folder videos and episodes
//! with no provider art. Runs as the `thumbnails` task over a library. Design:
//! `docs/tasks.md`.
//!
//! The work list is computed each run: what has no thumbnail, or whose file has
//! changed since. Frames go to the data directory, never next to the media, under
//! a temporary name until they're complete.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::json;
use sqlx::SqlitePool;

use crate::tasks::{Ended, Finished, Handle, Trigger};

/// Failures kept in a task's outcome; the rest are only counted.
const FAILURES_KEPT: usize = 200;
/// How long one attempt at a frame may take.
const ATTEMPT_TIMEOUT: Duration = Duration::from_secs(120);
/// How far into the video to look, in order, until a frame isn't dark.
const POSITIONS: [f64; 4] = [0.25, 0.4, 0.6, 0.1];
/// Average brightness (0–255, video black is 16) below which a frame counts as dark.
const DARK: f64 = 28.0;
const WIDTH: u32 = 640;

#[derive(Debug, Clone)]
pub struct Thumbnailer {
    pub ffmpeg: PathBuf,
    /// Where thumbnails are kept: `thumbnails` in the data directory.
    pub dir: PathBuf,
}

impl Thumbnailer {
    pub fn path(&self, file_id: i64) -> PathBuf {
        self.dir.join(format!("{file_id}.webp"))
    }
}

/// Files in library `?1` due a thumbnail: readable video, linked to a movie or
/// episode with no provider art, without a thumbnail of the file as it is now. `?2` tries failed
/// ones again.
const DUE: &str = "FROM media_file f
     JOIN library_root r ON r.id = f.root_id
     LEFT JOIN file_thumbnail t ON t.file_id = f.id
     WHERE f.library_id = ?1 AND f.status = 'present' AND r.offline_since IS NULL
       AND EXISTS (SELECT 1 FROM media_stream s WHERE s.file_id = f.id AND s.kind = 'video')
       AND EXISTS (
           SELECT 1 FROM file_link l JOIN item i ON i.id = l.item_id
           WHERE l.file_id = f.id AND i.kind IN ('movie', 'episode')
             AND NOT EXISTS (SELECT 1 FROM item_artwork a WHERE a.item_id = i.id))
       AND (t.file_id IS NULL OR t.size <> f.size OR t.mtime <> f.mtime
            OR (t.error IS NOT NULL AND ?2))";

/// Whether a run started by a scan would find anything to do.
pub async fn has_work(pool: &SqlitePool, library: i64) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT EXISTS (SELECT 1 {DUE})"
    )))
    .bind(library)
    .bind(false)
    .fetch_one(pool)
    .await
}

/// The file whose thumbnail stands for `item`: the first part or version with one.
pub async fn for_item(pool: &SqlitePool, item: i64) -> Result<Option<i64>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT t.file_id FROM file_link l
         JOIN file_thumbnail t ON t.file_id = l.file_id
         WHERE l.item_id = ? AND t.error IS NULL
         ORDER BY l.part, t.file_id LIMIT 1",
    )
    .bind(item)
    .fetch_optional(pool)
    .await
}

struct Due {
    file_id: i64,
    path: PathBuf,
    size: i64,
    mtime: i64,
    duration: Option<f64>,
    hdr: bool,
}

#[derive(Debug, thiserror::Error)]
enum ExtractError {
    /// ffmpeg itself couldn't be started. Affects every file, so the run stops.
    #[error("can't run {program}: {source}")]
    Unavailable {
        program: PathBuf,
        source: std::io::Error,
    },
    #[error("{0}")]
    Failed(String),
}

/// The filters for one frame: square pixels, tone-mapped if HDR, at most `WIDTH`
/// wide, and printing the frame's brightness to stdout.
fn filters(hdr: bool) -> String {
    let tonemap = if hdr {
        format!("{},", crate::playback::TONEMAP)
    } else {
        String::new()
    };
    format!(
        "{tonemap}scale=trunc(iw*sar/2)*2:ih,setsar=1,scale='min({WIDTH},iw)':-2,format=yuv420p,\
         signalstats,metadata=print:key=lavfi.signalstats.YAVG:file=-"
    )
}

/// The brightness `metadata=print` wrote.
fn brightness(stdout: &str) -> Option<f64> {
    stdout
        .lines()
        .find_map(|line| line.strip_prefix("lavfi.signalstats.YAVG="))
        .and_then(|value| value.trim().parse().ok())
}

/// Writes the frame at `at` seconds to `out`, returning how bright it is.
async fn frame(
    thumbnailer: &Thumbnailer,
    due: &Due,
    at: f64,
    out: &Path,
) -> Result<f64, ExtractError> {
    let mut command = tokio::process::Command::new(&thumbnailer.ffmpeg);
    command
        .args(["-nostdin", "-hide_banner", "-loglevel", "error", "-y"])
        .args(["-ss", &format!("{at:.3}")])
        .arg("-i")
        .arg(&due.path)
        // Capital V: never a cover picture stored as a video stream.
        .args(["-map", "0:V:0", "-map_metadata", "-1", "-frames:v", "1"])
        .args(["-vf", &filters(due.hdr)])
        .args(["-c:v", "libwebp", "-quality", "75", "-f", "webp"])
        .arg(out)
        .kill_on_drop(true);
    let output = tokio::time::timeout(ATTEMPT_TIMEOUT, command.output())
        .await
        .map_err(|_| ExtractError::Failed(format!("ffmpeg timed out after {ATTEMPT_TIMEOUT:?}")))?
        .map_err(|source| ExtractError::Unavailable {
            program: thumbnailer.ffmpeg.clone(),
            source,
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let message = stderr.lines().last().unwrap_or("no output").trim();
        return Err(ExtractError::Failed(format!("ffmpeg failed: {message}")));
    }
    // ffmpeg succeeds with no output when there's no frame to be had at that point.
    let written = tokio::fs::metadata(out).await.is_ok_and(|m| m.len() > 0);
    if !written {
        return Err(ExtractError::Failed(format!("no frame at {at:.0}s")));
    }
    // A brightness that can't be read is no reason to give up on the frame.
    Ok(brightness(&String::from_utf8_lossy(&output.stdout)).unwrap_or(f64::MAX))
}

/// Tries positions until a frame isn't dark, and keeps the brightest.
async fn try_positions(
    thumbnailer: &Thumbnailer,
    due: &Due,
    attempt: &Path,
    best: &Path,
) -> Result<(), ExtractError> {
    let positions: Vec<f64> = match due.duration {
        Some(duration) if duration > 0.0 => POSITIONS.iter().map(|p| p * duration).collect(),
        _ => vec![0.0],
    };
    let mut brightest: Option<f64> = None;
    let mut problem = None;
    for at in positions {
        match frame(thumbnailer, due, at, attempt).await {
            Ok(bright) => {
                if brightest.is_none_or(|b| bright > b) {
                    tokio::fs::rename(attempt, best)
                        .await
                        .map_err(|e| ExtractError::Failed(format!("can't save it: {e}")))?;
                    brightest = Some(bright);
                }
                if bright >= DARK {
                    break;
                }
            }
            Err(e @ ExtractError::Unavailable { .. }) => {
                return Err(e);
            }
            // A damaged stretch of the file: another position may still work.
            Err(e) => problem = Some(e),
        }
    }
    if brightest.is_none() {
        return Err(problem.unwrap_or_else(|| ExtractError::Failed("no frame".into())));
    }
    tokio::fs::rename(best, thumbnailer.path(due.file_id))
        .await
        .map_err(|e| ExtractError::Failed(format!("can't save it: {e}")))
}

async fn extract(thumbnailer: &Thumbnailer, due: &Due) -> Result<(), ExtractError> {
    let attempt = thumbnailer.dir.join(format!("{}.attempt", due.file_id));
    let best = thumbnailer.dir.join(format!("{}.best", due.file_id));
    let result = try_positions(thumbnailer, due, &attempt, &best).await;
    let _ = tokio::fs::remove_file(&attempt).await;
    let _ = tokio::fs::remove_file(&best).await;
    result
}

async fn record(pool: &SqlitePool, due: &Due, error: Option<&str>) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO file_thumbnail (file_id, size, mtime, error) VALUES (?, ?, ?, ?)
         ON CONFLICT (file_id) DO UPDATE SET size = excluded.size, mtime = excluded.mtime,
             made_at = unixepoch(), error = excluded.error",
    )
    .bind(due.file_id)
    .bind(due.size)
    .bind(due.mtime)
    .bind(error)
    .execute(pool)
    .await?;
    Ok(())
}

pub(crate) async fn run_task(task: &Handle, library: i64) -> Result<Finished, sqlx::Error> {
    let failed_to_start = |error: String| Finished {
        state: Ended::Failed,
        outcome: None,
        error: Some(error),
    };
    let Some(thumbnailer) = task.services().thumbnailer.as_ref() else {
        return Ok(failed_to_start("Thumbnails aren't set up.".into()));
    };
    if let Err(e) = tokio::fs::create_dir_all(&thumbnailer.dir).await {
        return Ok(failed_to_start(format!(
            "Can't create {}: {e}",
            thumbnailer.dir.display()
        )));
    }
    let pool = task.pool();
    type Row = (i64, String, String, i64, i64, Option<f64>, Option<String>);
    let rows: Vec<Row> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT f.id, r.path, f.relative_path, f.size, f.mtime, f.duration_seconds,
                (SELECT s.hdr_format FROM media_stream s WHERE s.file_id = f.id AND s.kind = 'video'
                 ORDER BY s.stream_index LIMIT 1)
         {DUE} ORDER BY f.id"
    )))
    .bind(library)
    // Asking by hand tries again what failed before.
    .bind(task.trigger() == Trigger::Manual)
    .fetch_all(pool)
    .await?;
    let total = rows.len();
    tracing::info!(total, "thumbnails due");
    task.progress(|p| {
        p.phase = "extracting";
        p.done = 0;
        p.total = total;
    });

    let mut made = 0;
    let mut failed = 0;
    let mut failures = Vec::new();
    let mut problem = None;
    let mut cancelled = false;
    for (done, row) in rows.into_iter().enumerate() {
        let (file_id, root, relative_path, size, mtime, duration, hdr) = row;
        // Playback comes first.
        task.wait_for_playback().await;
        task.progress(|p| p.phase = "extracting");
        if task.cancelled() {
            cancelled = true;
            break;
        }
        let due = Due {
            file_id,
            path: Path::new(&root).join(&relative_path),
            size,
            mtime,
            duration,
            hdr: hdr.is_some(),
        };
        match extract(thumbnailer, &due).await {
            Ok(()) => {
                record(pool, &due, None).await?;
                made += 1;
                tracing::info!(file_id, path = relative_path, "thumbnail made");
            }
            Err(e @ ExtractError::Unavailable { .. }) => {
                problem = Some(e.to_string());
                break;
            }
            Err(ExtractError::Failed(error)) => {
                // A file that can't be reached says nothing about the file, so
                // it isn't marked: a drive may have just gone away.
                if tokio::fs::metadata(&due.path).await.is_ok() {
                    record(pool, &due, Some(&error)).await?;
                }
                failed += 1;
                tracing::warn!(file_id, path = relative_path, error, "no thumbnail");
                if failures.len() < FAILURES_KEPT {
                    failures.push(json!({
                        "file_id": file_id,
                        "path": relative_path,
                        "error": error,
                    }));
                }
            }
        }
        task.progress(|p| p.done = done + 1);
    }

    let error = problem
        .clone()
        .or_else(|| (failed > 0).then(|| format!("{failed} of {total} files got no thumbnail.")));
    Ok(Finished {
        state: if problem.is_some() {
            Ended::Failed
        } else if cancelled {
            Ended::Cancelled
        } else {
            Ended::Ok
        },
        outcome: Some(json!({ "made": made, "failed": failed, "failures": failures })),
        error,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_brightness_ffmpeg_prints() {
        let printed = "frame:0    pts:1001    pts_time:41.7\nlavfi.signalstats.YAVG=103.42\n";
        assert_eq!(brightness(printed), Some(103.42));
        assert_eq!(brightness("frame:0\n"), None);
    }

    #[test]
    fn hdr_is_tone_mapped_before_scaling() {
        assert!(!filters(false).contains("tonemap"));
        let hdr = filters(true);
        assert!(hdr.find("tonemap").unwrap() < hdr.find("scale=trunc").unwrap());
    }
}
