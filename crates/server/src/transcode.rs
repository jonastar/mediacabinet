//! HLS sessions: one ffmpeg per playback that isn't direct, writing segments
//! into its own folder under the data directory.
//!
//! ffmpeg runs ahead of the player but not too far: it's paused (SIGSTOP) once
//! it's about three minutes ahead of the last segment asked for, and continued when
//! the player gets within a minute and a half. A session nobody has asked
//! anything of for a minute is stopped and its folder removed.
//!
//! ffmpeg reports its progress on stdout (`-progress`), for the player's stats.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cabinet_core::playback::{self, Decision, HwAccel, Version};
use serde::Serialize;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use utoipa::ToSchema;

const IDLE: Duration = Duration::from_secs(60);
const PAUSE_AHEAD: usize = 45;
const RESUME_AHEAD: usize = 22;

struct Session {
    user: i64,
    item: i64,
    child: Child,
    dir: PathBuf,
    last_access: Instant,
    /// The highest segment the player has asked for.
    requested: usize,
    paused: bool,
    /// Has its first segment. Sessions still starting aren't stopped by a newer
    /// start: two can race (the later request can arrive first), and the
    /// player stops the one it doesn't use.
    ready: bool,
    started: Instant,
    /// How long ffmpeg took to write its first segment.
    ready_after: Option<Duration>,
    /// The program and its arguments.
    command: Vec<String>,
    progress: Arc<Mutex<Progress>>,
}

/// What ffmpeg last reported, from its `-progress` blocks.
#[derive(Debug, Default, Clone, PartialEq)]
struct Progress {
    fps: Option<f64>,
    speed: Option<f64>,
    /// How much of the stream is written, from the stream's time 0.
    out_seconds: f64,
    finished: bool,
}

impl Progress {
    /// Takes in one `key=value` line.
    fn read(&mut self, line: &str) {
        let Some((key, value)) = line.trim().split_once('=') else {
            return;
        };
        let value = value.trim();
        match key {
            "fps" => self.fps = value.parse().ok(),
            "speed" => self.speed = value.trim_end_matches('x').parse().ok(),
            "out_time_us" => {
                if let Ok(us) = value.parse::<i64>() {
                    self.out_seconds = us.max(0) as f64 / 1_000_000.0;
                }
            }
            "progress" => self.finished = value == "end",
            _ => {}
        }
    }
}

/// How an HLS session is doing on the server, for the player's stats.
#[derive(Debug, Serialize, ToSchema)]
pub struct StreamStats {
    /// ffmpeg's speed as a multiple of real time; above 1 keeps ahead.
    speed: Option<f64>,
    /// Frames written per second.
    fps: Option<f64>,
    /// The output's average bitrate so far, from the segments written, in kbit/s.
    bitrate_kbps: Option<f64>,
    /// How much of the stream is written, in the stream's time.
    converted_seconds: f64,
    segments_written: usize,
    /// The highest segment the player has asked for.
    segments_requested: usize,
    /// Paused for being far enough ahead of the player.
    throttled: bool,
    /// ffmpeg reached the end of the file.
    finished: bool,
    running_seconds: f64,
    /// How long ffmpeg took to write its first segment.
    first_segment_seconds: Option<f64>,
    /// The ffmpeg command line, for admins.
    command: Option<String>,
    /// The end of ffmpeg's error log, for admins.
    log: Option<String>,
}

/// The command as a shell would take it, an option and its value to a line,
/// and the output last on its own.
fn pretty_command(command: &[String]) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut args = command.iter().map(|a| shell_quote(a)).peekable();
    if let Some(program) = args.next() {
        lines.push(program);
    }
    while let Some(arg) = args.next() {
        let is_option = arg.starts_with('-') && arg.len() > 1;
        let takes_value = args
            .peek()
            .is_some_and(|next| !next.starts_with('-') || next.parse::<f64>().is_ok());
        let line = match args.next_if(|_| is_option && takes_value) {
            Some(value) => format!("  {arg} {value}"),
            None => format!("  {arg}"),
        };
        lines.push(line);
    }
    lines.join(" \\\n")
}

/// Quotes an argument for a POSIX shell when it needs it.
fn shell_quote(arg: &str) -> String {
    let plain = !arg.is_empty()
        && arg
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./:=+,@%".contains(c));
    if plain {
        arg.to_string()
    } else {
        format!("'{}'", arg.replace('\'', "'\\''"))
    }
}

#[derive(Clone)]
pub struct Transcoder {
    dir: PathBuf,
    ffmpeg: PathBuf,
    hw: HwAccel,
    sessions: Arc<Mutex<HashMap<String, Session>>>,
}

fn signal(child: &Child, sig: i32) {
    #[cfg(unix)]
    if let Some(pid) = child.id() {
        // SAFETY: sending a signal to our own child process.
        unsafe {
            libc::kill(pid as i32, sig);
        }
    }
}

fn segment_number(name: &str) -> Option<usize> {
    name.strip_prefix("seg")?.strip_suffix(".m4s")?.parse().ok()
}

/// The segments written so far and their total size.
fn segments_size(dir: &Path) -> (usize, u64) {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| e.file_name().to_str().and_then(segment_number).is_some())
                .fold((0, 0), |(n, bytes), e| {
                    (n + 1, bytes + e.metadata().map(|m| m.len()).unwrap_or(0))
                })
        })
        .unwrap_or((0, 0))
}

/// How long the segments in the playlist add up to; they aren't all the same length.
fn playlist_seconds(dir: &Path) -> f64 {
    let playlist = std::fs::read_to_string(dir.join("index.m3u8")).unwrap_or_default();
    playlist
        .lines()
        .filter_map(|line| line.strip_prefix("#EXTINF:"))
        .filter_map(|rest| rest.split(',').next()?.parse::<f64>().ok())
        .sum()
}

fn segments_written(dir: &Path) -> usize {
    segments_size(dir).0
}

impl Transcoder {
    /// Clears what earlier runs left, and checks the hardware encoder works,
    /// falling back to software if it doesn't.
    pub async fn new(dir: PathBuf, ffmpeg: PathBuf, hw: HwAccel) -> Transcoder {
        let _ = tokio::fs::remove_dir_all(&dir).await;
        let hw = if hw == HwAccel::Software {
            hw
        } else {
            let test = Command::new(&ffmpeg)
                .args(hw.test_args())
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .output()
                .await;
            match test {
                Ok(out) if out.status.success() => {
                    tracing::info!(?hw, "hardware encoding works");
                    hw
                }
                Ok(out) => {
                    let error = String::from_utf8_lossy(&out.stderr);
                    tracing::warn!(?hw, error = %error.trim(), "hardware encoding failed; encoding in software");
                    HwAccel::Software
                }
                Err(e) => {
                    tracing::warn!(error = %e, "can't run ffmpeg; playback will only play files directly");
                    HwAccel::Software
                }
            }
        };
        Transcoder {
            dir,
            ffmpeg,
            hw,
            sessions: Arc::default(),
        }
    }

    /// Starts ffmpeg and waits for the first segment. Stops the user's earlier
    /// sessions of the same item that have started. Returns the session ID.
    pub async fn start(
        &self,
        user: i64,
        item: i64,
        version: &Version,
        decision: &Decision,
        start: f64,
    ) -> Result<String, String> {
        let old: Vec<String> = {
            let sessions = self.sessions.lock().unwrap();
            sessions
                .iter()
                .filter(|(_, s)| s.user == user && s.item == item && s.ready)
                .map(|(id, _)| id.clone())
                .collect()
        };
        for id in old {
            self.stop(user, &id).await;
        }

        let id = cabinet_core::apikeys::random_secret().replacen("mc_", "", 1)[..20].to_string();
        let dir = self.dir.join(&id);
        tokio::fs::create_dir_all(&dir)
            .await
            .map_err(|e| e.to_string())?;
        let log = std::fs::File::create(dir.join("ffmpeg.log")).map_err(|e| e.to_string())?;
        let args = playback::hls_args(version, decision, start, &dir, &self.hw);
        tracing::debug!(session = id, ?args, "starting ffmpeg");
        let mut child = Command::new(&self.ffmpeg)
            .args(["-progress", "pipe:1", "-stats_period", "1"])
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(log)
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("can't run ffmpeg: {e}"))?;
        let progress = Arc::new(Mutex::new(Progress::default()));
        if let Some(stdout) = child.stdout.take() {
            let progress = progress.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    progress.lock().unwrap().read(&line);
                }
            });
        }
        let mut command = vec![self.ffmpeg.to_string_lossy().into_owned()];
        command.extend(args);
        self.sessions.lock().unwrap().insert(
            id.clone(),
            Session {
                user,
                item,
                child,
                dir: dir.clone(),
                last_access: Instant::now(),
                requested: 0,
                paused: false,
                ready: false,
                started: Instant::now(),
                ready_after: None,
                command,
                progress,
            },
        );

        // Ready once the playlist lists a segment, or ffmpeg finished a short file.
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let playlist = tokio::fs::read_to_string(dir.join("index.m3u8"))
                .await
                .unwrap_or_default();
            if playlist.contains("#EXTINF") {
                return match self.sessions.lock().unwrap().get_mut(&id) {
                    Some(s) => {
                        s.ready = true;
                        s.ready_after = Some(s.started.elapsed());
                        Ok(id)
                    }
                    None => Err("The stream was stopped.".into()),
                };
            }
            let exited = {
                let mut sessions = self.sessions.lock().unwrap();
                match sessions.get_mut(&id) {
                    Some(s) => s.child.try_wait().ok().flatten(),
                    None => return Err("The stream was stopped.".into()),
                }
            };
            if let Some(status) = exited {
                let log = tokio::fs::read_to_string(dir.join("ffmpeg.log"))
                    .await
                    .unwrap_or_default();
                self.stop(user, &id).await;
                tracing::warn!(%status, log = log.trim(), "ffmpeg failed to start a stream");
                return Err(format!(
                    "ffmpeg couldn't convert this file: {}",
                    log.trim().lines().last().unwrap_or("")
                ));
            }
            if Instant::now() > deadline {
                self.stop(user, &id).await;
                return Err("ffmpeg took too long to start.".into());
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// A file of the user's session (`index.m3u8`, `init.mp4`, `seg00001.m4s`).
    /// Asking for a segment keeps the session going and may wake ffmpeg.
    pub fn file(&self, user: i64, id: &str, name: &str) -> Option<PathBuf> {
        let allowed = name == "index.m3u8" || name == "init.mp4" || segment_number(name).is_some();
        if !allowed {
            return None;
        }
        let mut sessions = self.sessions.lock().unwrap();
        let session = sessions.get_mut(id).filter(|s| s.user == user)?;
        session.last_access = Instant::now();
        if let Some(n) = segment_number(name) {
            session.requested = session.requested.max(n);
            if session.paused && segments_written(&session.dir) < session.requested + RESUME_AHEAD {
                signal(&session.child, libc::SIGCONT);
                session.paused = false;
            }
        }
        Some(session.dir.join(name))
    }

    /// How the user's session is doing; the command and log only for admins.
    pub async fn stats(&self, user: i64, id: &str, admin: bool) -> Option<StreamStats> {
        let (mut stats, log) = {
            let sessions = self.sessions.lock().unwrap();
            let session = sessions.get(id).filter(|s| s.user == user)?;
            let progress = session.progress.lock().unwrap().clone();
            let command = admin.then(|| pretty_command(&session.command));
            let (written, size) = segments_size(&session.dir);
            let seconds = playlist_seconds(&session.dir);
            let stats = StreamStats {
                speed: progress.speed,
                fps: progress.fps,
                bitrate_kbps: (seconds > 0.0).then(|| size as f64 * 8.0 / 1000.0 / seconds),
                converted_seconds: progress.out_seconds,
                segments_written: written,
                segments_requested: session.requested,
                throttled: session.paused,
                finished: progress.finished,
                running_seconds: session.started.elapsed().as_secs_f64(),
                first_segment_seconds: session.ready_after.map(|d| d.as_secs_f64()),
                command,
                log: None,
            };
            (stats, session.dir.join("ffmpeg.log"))
        };
        if admin {
            let text = tokio::fs::read_to_string(log).await.unwrap_or_default();
            let lines: Vec<&str> = text.trim().lines().collect();
            let tail = lines[lines.len().saturating_sub(20)..].join("\n");
            stats.log = (!tail.is_empty()).then_some(tail);
        }
        Some(stats)
    }

    pub async fn stop(&self, user: i64, id: &str) -> bool {
        let session = {
            let mut sessions = self.sessions.lock().unwrap();
            match sessions.get(id) {
                Some(s) if s.user == user => sessions.remove(id),
                _ => None,
            }
        };
        match session {
            Some(session) => {
                finish(session).await;
                true
            }
            None => false,
        }
    }

    /// Pauses runaway ffmpegs and ends idle sessions, every two seconds.
    pub fn spawn_reaper(&self) {
        let this = self.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(2));
            loop {
                tick.tick().await;
                let idle: Vec<Session> = {
                    let mut sessions = this.sessions.lock().unwrap();
                    let ids: Vec<String> = sessions
                        .iter()
                        .filter(|(_, s)| s.last_access.elapsed() > IDLE)
                        .map(|(id, _)| id.clone())
                        .collect();
                    for session in sessions.values_mut() {
                        let written = segments_written(&session.dir);
                        if !session.paused && written > session.requested + PAUSE_AHEAD {
                            signal(&session.child, libc::SIGSTOP);
                            session.paused = true;
                        } else if session.paused && written < session.requested + RESUME_AHEAD {
                            signal(&session.child, libc::SIGCONT);
                            session.paused = false;
                        }
                    }
                    ids.iter().filter_map(|id| sessions.remove(id)).collect()
                };
                for session in idle {
                    tracing::debug!(dir = %session.dir.display(), "ending an idle stream");
                    finish(session).await;
                }
            }
        });
    }

    /// Whether any stream is being remuxed or transcoded.
    pub fn active(&self) -> bool {
        !self.sessions.lock().unwrap().is_empty()
    }

    pub fn hardware(&self) -> &HwAccel {
        &self.hw
    }

    pub fn ffmpeg(&self) -> &Path {
        &self.ffmpeg
    }
}

async fn finish(mut session: Session) {
    // A stopped process still dies of SIGKILL.
    let _ = session.child.start_kill();
    let _ = session.child.wait().await;
    let _ = tokio::fs::remove_dir_all(&session.dir).await;
}

#[cfg(test)]
mod tests {
    use super::Progress;

    #[test]
    fn reads_progress() {
        let mut p = Progress::default();
        for line in [
            "frame=240",
            "fps=59.94",
            "bitrate=4123.5kbits/s",
            "speed= 2.41x",
            "out_time_us=N/A",
            "out_time_us=10010000",
            "progress=continue",
        ] {
            p.read(line);
        }
        assert_eq!(p.fps, Some(59.94));
        assert_eq!(p.out_seconds, 10.01);
        assert_eq!(p.speed, Some(2.41));
        assert!(!p.finished);
        p.read("speed=N/A");
        p.read("progress=end");
        assert_eq!(p.speed, None);
        assert!(p.finished);
    }

    #[test]
    fn lays_out_a_command() {
        let command = [
            "ffmpeg",
            "-nostdin",
            "-ss",
            "10.000",
            "-i",
            "/a b.mkv",
            "-map_metadata",
            "-1",
            "-c:a",
            "copy",
            "/out/index.m3u8",
        ]
        .map(String::from);
        assert_eq!(
            super::pretty_command(&command),
            "ffmpeg \\\n  -nostdin \\\n  -ss 10.000 \\\n  -i '/a b.mkv' \\\n  -map_metadata -1 \\\n  -c:a copy \\\n  /out/index.m3u8"
        );
    }

    #[test]
    fn quotes_for_a_shell() {
        assert_eq!(super::shell_quote("-c:v"), "-c:v");
        assert_eq!(super::shell_quote("/a b/it's.mkv"), "'/a b/it'\\''s.mkv'");
        assert_eq!(
            super::shell_quote("[0:0]null,scale=-2:'min(ih,1080)'"),
            "'[0:0]null,scale=-2:'\\''min(ih,1080)'\\'''"
        );
    }

    #[test]
    fn segment_names() {
        assert_eq!(super::segment_number("seg00012.m4s"), Some(12));
        assert_eq!(super::segment_number("seg../x.m4s"), None);
        assert_eq!(super::segment_number("index.m3u8"), None);
    }
}
