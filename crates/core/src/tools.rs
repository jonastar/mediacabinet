//! Finding ffmpeg and ffprobe.

use std::path::{Path, PathBuf};

/// The program to run for `name` (`ffmpeg` or `ffprobe`): the path in the
/// environment variable `env` if it's set, else the copy next to the server's
/// own executable (release archives and the image ship one), else `name` from
/// the PATH.
pub fn program(env: &str, name: &str) -> PathBuf {
    if let Ok(path) = std::env::var(env) {
        return path.into();
    }
    let exe = std::env::current_exe().ok();
    bundled(exe.as_deref().and_then(Path::parent), name).unwrap_or_else(|| name.into())
}

fn bundled(dir: Option<&Path>, name: &str) -> Option<PathBuf> {
    let path = dir?.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    path.is_file().then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_a_copy_next_to_the_executable() {
        let dir = std::env::temp_dir().join(format!("cabinet-tools-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(bundled(Some(&dir), "ffmpeg"), None);

        let path = dir.join(format!("ffmpeg{}", std::env::consts::EXE_SUFFIX));
        std::fs::write(&path, "").unwrap();
        assert_eq!(bundled(Some(&dir), "ffmpeg"), Some(path));
        assert_eq!(bundled(None, "ffmpeg"), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
