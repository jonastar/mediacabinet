//! Walks a library root and lists the video files in it.

use std::io;
use std::path::Path;
use std::time::UNIX_EPOCH;

use walkdir::{DirEntry, WalkDir};

/// Extensions treated as video files. Anything else (subtitles, nfo, partial
/// downloads such as `.part`) is not a media file.
const VIDEO_EXTENSIONS: &[&str] = &[
    "3gp", "avi", "divx", "flv", "m2ts", "m4v", "mkv", "mov", "mp4", "mpeg", "mpg", "mts", "ogv",
    "ts", "vob", "webm", "wmv",
];

/// Folders created by NAS software, operating systems and recycle bins.
const SKIPPED_DIRS: &[&str] = &[
    "@eaDir",
    "#recycle",
    "#snapshot",
    "$RECYCLE.BIN",
    "lost+found",
    "System Volume Information",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// Relative to the root, `/`-separated.
    pub relative_path: String,
    pub size: i64,
    pub mtime: i64,
    pub device_id: Option<i64>,
    pub inode: Option<i64>,
}

#[derive(Debug, Default)]
pub struct Walk {
    pub files: Vec<Found>,
    /// Subtitles, `.nfo` and artwork, as relative paths.
    pub sidecars: Vec<String>,
    /// Relative paths that could not be read. Files known under them are left
    /// untouched, never marked missing.
    pub unreadable: Vec<String>,
    /// Files whose names are not valid UTF-8, which the database cannot store.
    pub skipped_non_utf8: usize,
}

impl Walk {
    pub fn is_unreadable(&self, relative_path: &str) -> bool {
        self.unreadable.iter().any(|dir| {
            relative_path == dir
                || relative_path
                    .strip_prefix(dir.as_str())
                    .is_some_and(|rest| rest.starts_with('/'))
        })
    }
}

/// Lists the video files under `root`. Fails only if the root itself can't be read.
pub fn walk(root: &Path) -> io::Result<Walk> {
    // Fail early, and distinctly, for an unreachable root.
    std::fs::read_dir(root)?;

    let mut walk = Walk::default();
    let entries = WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| e.depth() == 0 || !is_skipped(e));

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                let path = err.path().and_then(|p| relative(root, p));
                if err.depth() == 0 {
                    return Err(err
                        .into_io_error()
                        .unwrap_or_else(|| io::Error::other("cannot read root")));
                }
                tracing::warn!(error = %err, "unreadable path during scan");
                if let Some(path) = path {
                    walk.unreadable.push(path);
                }
                continue;
            }
        };
        if !entry.file_type().is_file() {
            continue;
        }
        let is_sidecar = super::sidecar::kind_of(entry.path()).is_some();
        if !is_video(entry.path()) && !is_sidecar {
            continue;
        }
        let Some(relative_path) = relative(root, entry.path()) else {
            walk.skipped_non_utf8 += 1;
            tracing::warn!(path = ?entry.path(), "skipping file with a non-UTF-8 name");
            continue;
        };
        if is_sidecar {
            walk.sidecars.push(relative_path);
            continue;
        }
        let metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(err) => {
                tracing::warn!(error = %err, path = relative_path, "cannot read file metadata");
                walk.unreadable.push(relative_path);
                continue;
            }
        };
        // Zero-byte files can't be media.
        if metadata.len() == 0 {
            continue;
        }
        let (device_id, inode) = identity(&metadata);
        walk.files.push(Found {
            relative_path,
            size: metadata.len() as i64,
            mtime: mtime(&metadata),
            device_id,
            inode,
        });
    }
    Ok(walk)
}

/// Identity and size of one file under `root`, as a walk would record it.
pub fn stat(root: &Path, path: &Path) -> io::Result<Found> {
    let metadata = std::fs::metadata(path)?;
    let relative_path = relative(root, path).ok_or_else(|| {
        io::Error::other(format!(
            "{} isn't a UTF-8 path under the root",
            path.display()
        ))
    })?;
    let (device_id, inode) = identity(&metadata);
    Ok(Found {
        relative_path,
        size: metadata.len() as i64,
        mtime: mtime(&metadata),
        device_id,
        inode,
    })
}

pub fn is_sidecar(path: &Path) -> bool {
    super::sidecar::kind_of(path).is_some()
}

fn is_skipped(entry: &DirEntry) -> bool {
    let name = entry.file_name().to_string_lossy();
    name.starts_with('.') || (entry.file_type().is_dir() && SKIPPED_DIRS.contains(&name.as_ref()))
}

pub fn is_video(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| VIDEO_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

fn relative(root: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?;
    let parts: Option<Vec<&str>> = rel.components().map(|c| c.as_os_str().to_str()).collect();
    Some(parts?.join("/"))
}

fn mtime(metadata: &std::fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(unix)]
fn identity(metadata: &std::fs::Metadata) -> (Option<i64>, Option<i64>) {
    use std::os::unix::fs::MetadataExt;
    (Some(metadata.dev() as i64), Some(metadata.ino() as i64))
}

#[cfg(not(unix))]
fn identity(_metadata: &std::fs::Metadata) -> (Option<i64>, Option<i64>) {
    (None, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unreadable_matches_whole_path_segments() {
        let walk = Walk {
            unreadable: vec!["Show/Season 01".into()],
            ..Walk::default()
        };
        assert!(walk.is_unreadable("Show/Season 01"));
        assert!(walk.is_unreadable("Show/Season 01/e01.mkv"));
        assert!(!walk.is_unreadable("Show/Season 010/e01.mkv"));
        assert!(!walk.is_unreadable("Show/e01.mkv"));
    }

    #[test]
    fn video_extensions_ignore_case() {
        assert!(is_video(Path::new("a/B.MKV")));
        assert!(!is_video(Path::new("a/b.mkv.part")));
        assert!(!is_video(Path::new("a/b.srt")));
        assert!(!is_video(Path::new("a/mkv")));
    }
}
