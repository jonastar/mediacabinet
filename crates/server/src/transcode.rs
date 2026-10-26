//! HLS sessions: one ffmpeg per playback that isn't direct, writing segments
//! into its own folder under the data directory.
//!
//! ffmpeg runs ahead of the player but not too far: it's paused (SIGSTOP) once
//! it's three minutes ahead of the last segment asked for, and continued when
//! the player gets within a minute and a half. A session nobody has asked
//! anything of for a minute is stopped and its folder removed.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cabinet_core::playback::{self, Decision, HwAccel, Version};
use tokio::process::{Child, Command};

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

fn segments_written(dir: &Path) -> usize {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| e.file_name().to_str().and_then(segment_number).is_some())
                .count()
        })
        .unwrap_or(0)
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
    /// sessions of the same item. Returns the session ID.
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
                .filter(|(_, s)| s.user == user && s.item == item)
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
        let child = Command::new(&self.ffmpeg)
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(log)
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("can't run ffmpeg: {e}"))?;
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
            },
        );

        // Ready once the playlist lists a segment, or ffmpeg finished a short file.
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let playlist = tokio::fs::read_to_string(dir.join("index.m3u8"))
                .await
                .unwrap_or_default();
            if playlist.contains("#EXTINF") {
                return Ok(id);
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
    #[test]
    fn segment_names() {
        assert_eq!(super::segment_number("seg00012.m4s"), Some(12));
        assert_eq!(super::segment_number("seg../x.m4s"), None);
        assert_eq!(super::segment_number("index.m3u8"), None);
    }
}
