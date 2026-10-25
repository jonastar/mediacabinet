//! Reads container and stream information with ffprobe.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct Prober {
    pub program: PathBuf,
    pub timeout: Duration,
    /// Files probed at once.
    pub concurrency: usize,
}

impl Default for Prober {
    fn default() -> Self {
        Prober {
            program: "ffprobe".into(),
            timeout: Duration::from_secs(60),
            concurrency: 4,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Probe {
    /// ffprobe's format name, e.g. `matroska,webm` or `mov,mp4,m4a,3gp,3g2,mj2`.
    pub container: Option<String>,
    pub duration_seconds: Option<f64>,
    pub streams: Vec<Stream>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stream {
    pub index: i64,
    /// `video`, `audio` or `subtitle`.
    pub kind: &'static str,
    pub codec: Option<String>,
    pub language: Option<String>,
    pub channels: Option<i64>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    /// `hdr10`, `hlg` or `dolby_vision`. HDR10+ needs frame data and isn't detected yet.
    pub hdr_format: Option<String>,
    pub bitrate: Option<i64>,
    pub is_default: bool,
    pub is_forced: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum ProbeError {
    /// ffprobe itself couldn't be started. Affects every file, so callers stop.
    #[error("cannot run {program}: {source}")]
    Unavailable {
        program: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("ffprobe timed out after {0:?}")]
    Timeout(Duration),
    #[error("ffprobe failed: {0}")]
    Failed(String),
    #[error("unreadable ffprobe output: {0}")]
    Output(#[from] serde_json::Error),
}

impl Prober {
    pub async fn probe(&self, path: &Path) -> Result<Probe, ProbeError> {
        let mut command = tokio::process::Command::new(&self.program);
        command
            .args([
                "-v",
                "error",
                "-print_format",
                "json",
                "-show_format",
                "-show_streams",
            ])
            .arg(path)
            .kill_on_drop(true);
        let output = tokio::time::timeout(self.timeout, command.output())
            .await
            .map_err(|_| ProbeError::Timeout(self.timeout))?
            .map_err(|source| ProbeError::Unavailable {
                program: self.program.clone(),
                source,
            })?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let message = stderr.lines().last().unwrap_or("no output").trim();
            return Err(ProbeError::Failed(message.to_string()));
        }
        Ok(parse(&output.stdout)?)
    }
}

pub fn parse(json: &[u8]) -> Result<Probe, serde_json::Error> {
    let raw: RawProbe = serde_json::from_slice(json)?;
    let format = raw.format.unwrap_or_default();
    Ok(Probe {
        container: format.format_name,
        duration_seconds: format.duration.and_then(|d| d.parse().ok()),
        streams: raw.streams.into_iter().filter_map(stream).collect(),
    })
}

fn stream(raw: RawStream) -> Option<Stream> {
    let flag = |name: &str| raw.disposition.get(name).copied().unwrap_or(0) == 1;
    let kind = match raw.codec_type.as_deref()? {
        // Cover art embedded in the container is a picture, not a video stream.
        "video" if flag("attached_pic") => {
            return None;
        }
        "video" => "video",
        "audio" => "audio",
        "subtitle" => "subtitle",
        _ => {
            return None;
        }
    };
    let language = raw
        .tags
        .get("language")
        .filter(|l| !l.is_empty() && l.as_str() != "und")
        .cloned();
    // Matroska stores the bitrate as a BPS tag (sometimes BPS-eng).
    let bitrate = raw
        .bit_rate
        .as_deref()
        .or_else(|| {
            raw.tags
                .iter()
                .find(|(k, _)| k.as_str() == "BPS" || k.starts_with("BPS-"))
                .map(|(_, v)| v.as_str())
        })
        .and_then(|b| b.parse().ok());
    let is_dolby_vision = raw.side_data_list.iter().any(|d| {
        d.get("side_data_type")
            .and_then(|t| t.as_str())
            .is_some_and(|t| t.starts_with("DOVI"))
    });
    let hdr_format = if kind != "video" {
        None
    } else if is_dolby_vision {
        Some("dolby_vision")
    } else {
        match raw.color_transfer.as_deref() {
            Some("smpte2084") => Some("hdr10"),
            Some("arib-std-b67") => Some("hlg"),
            _ => None,
        }
    };

    Some(Stream {
        index: raw.index,
        kind,
        codec: raw.codec_name,
        language,
        channels: raw.channels,
        width: raw.width,
        height: raw.height,
        hdr_format: hdr_format.map(String::from),
        bitrate,
        is_default: flag("default"),
        is_forced: flag("forced"),
    })
}

#[derive(Deserialize)]
struct RawProbe {
    #[serde(default)]
    streams: Vec<RawStream>,
    format: Option<RawFormat>,
}

#[derive(Deserialize, Default)]
struct RawFormat {
    format_name: Option<String>,
    duration: Option<String>,
}

#[derive(Deserialize)]
struct RawStream {
    index: i64,
    codec_type: Option<String>,
    codec_name: Option<String>,
    channels: Option<i64>,
    width: Option<i64>,
    height: Option<i64>,
    color_transfer: Option<String>,
    bit_rate: Option<String>,
    #[serde(default)]
    tags: HashMap<String, String>,
    #[serde(default)]
    disposition: HashMap<String, i64>,
    #[serde(default)]
    side_data_list: Vec<HashMap<String, serde_json::Value>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_streams_and_skips_cover_art() {
        let json = br#"{
            "streams": [
                {"index": 0, "codec_type": "video", "codec_name": "hevc", "width": 3840, "height": 2160,
                 "color_transfer": "smpte2084", "tags": {"BPS-eng": "15000000"},
                 "disposition": {"default": 1, "forced": 0}},
                {"index": 1, "codec_type": "audio", "codec_name": "eac3", "channels": 6,
                 "bit_rate": "640000", "tags": {"language": "eng"}, "disposition": {"default": 1}},
                {"index": 2, "codec_type": "subtitle", "codec_name": "subrip",
                 "tags": {"language": "eng"}, "disposition": {"forced": 1}},
                {"index": 3, "codec_type": "video", "codec_name": "mjpeg",
                 "disposition": {"attached_pic": 1}},
                {"index": 4, "codec_type": "attachment", "codec_name": "ttf"}
            ],
            "format": {"format_name": "matroska,webm", "duration": "2712.512000"}
        }"#;
        let probe = parse(json).unwrap();

        assert_eq!(probe.container.as_deref(), Some("matroska,webm"));
        assert_eq!(probe.duration_seconds, Some(2712.512));
        assert_eq!(probe.streams.len(), 3);
        let video = &probe.streams[0];
        assert_eq!(
            (video.width, video.hdr_format.as_deref()),
            (Some(3840), Some("hdr10"))
        );
        assert_eq!(video.bitrate, Some(15_000_000));
        assert_eq!(probe.streams[1].channels, Some(6));
        assert!(probe.streams[2].is_forced && !probe.streams[2].is_default);
    }

    #[test]
    fn detects_dolby_vision_and_hlg() {
        let json = br#"{"streams": [
            {"index": 0, "codec_type": "video", "color_transfer": "smpte2084",
             "side_data_list": [{"side_data_type": "DOVI configuration record", "dv_profile": 8}]},
            {"index": 1, "codec_type": "video", "color_transfer": "arib-std-b67"},
            {"index": 2, "codec_type": "audio", "tags": {"language": "und"}}
        ]}"#;
        let probe = parse(json).unwrap();
        assert_eq!(probe.streams[0].hdr_format.as_deref(), Some("dolby_vision"));
        assert_eq!(probe.streams[1].hdr_format.as_deref(), Some("hlg"));
        assert_eq!(probe.streams[2].language, None);
    }
}
