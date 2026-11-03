//! Deciding how a file plays in a client, and the ffmpeg command to make it so.
//!
//! - **Direct**: the client plays the file as it is, over HTTP range requests.
//! - **Remux**: the video is fine but the container (or audio) isn't, so ffmpeg
//!   copies the video into HLS and converts the audio if it must.
//! - **Transcode**: the video is re-encoded to H.264: an unsupported codec, a
//!   lower quality asked for, HDR on an SDR screen (tone-mapped), or picture
//!   subtitles that have to be burned in.
//!
//! Text subtitles are never burned in: they're served as WebVTT for the player
//! to draw. Everything here is pure apart from loading versions; running
//! ffmpeg is the server's job.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use utoipa::ToSchema;

/// Seconds per HLS segment.
pub const SEGMENT_SECONDS: u32 = 4;
/// Seconds per segment for the first `SEGMENT_SECONDS` of an encoded stream,
/// so the first is ready, and downloaded, sooner.
pub const INITIAL_SEGMENT_SECONDS: u32 = 1;

/// What the client says it can play. Codec names are ffprobe's (`h264`, `hevc`,
/// `av1`, `vp9`, `aac`, `mp3`, `opus`, `flac`, `ac3`, `eac3`); containers are
/// `mp4`, `webm`, `mkv`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, ToSchema)]
pub struct ClientCaps {
    #[serde(default)]
    pub containers: Vec<String>,
    #[serde(default)]
    pub video_codecs: Vec<String>,
    #[serde(default)]
    pub audio_codecs: Vec<String>,
    /// The screen shows HDR (PQ/HLG). Otherwise HDR video is tone-mapped.
    #[serde(default)]
    pub hdr: bool,
}

/// The parts of a request that shape the decision.
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
pub struct Choice {
    /// Which file; defaults to the best present one.
    pub file_id: Option<i64>,
    /// An audio stream index; defaults to the file's default audio.
    pub audio_stream: Option<i64>,
    /// `stream:<index>` or `sidecar:<id>`; none for no subtitles.
    pub subtitle: Option<String>,
    /// Transcode down to this height (a quality choice), e.g. 720.
    pub max_height: Option<i64>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AudioTrack {
    pub index: i64,
    pub codec: Option<String>,
    pub language: Option<String>,
    pub channels: Option<i64>,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SubtitleTrack {
    /// `stream:<index>` or `sidecar:<id>`.
    pub id: String,
    pub codec: Option<String>,
    pub language: Option<String>,
    pub forced: bool,
    pub hearing_impaired: bool,
    /// Text subtitles are sent as WebVTT; picture ones (PGS, DVD) are burned in.
    pub text: bool,
    pub external: bool,
}

/// A file of the item, with what's in it.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Version {
    pub file_id: i64,
    pub file_name: String,
    pub size: i64,
    /// `mp4`, `webm`, `mkv`, or ffprobe's own name for anything else.
    pub container: Option<String>,
    pub duration_seconds: Option<f64>,
    pub video_stream: Option<i64>,
    pub video_codec: Option<String>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub hdr_format: Option<String>,
    pub audio: Vec<AudioTrack>,
    pub subtitles: Vec<SubtitleTrack>,
    #[serde(skip)]
    pub path: PathBuf,
}

/// Picture subtitle codecs, which can't become WebVTT.
const PICTURE_SUBTITLES: &[&str] = &["hdmv_pgs_subtitle", "dvd_subtitle", "dvb_subtitle", "xsub"];
/// Codecs HLS (fMP4) can carry as they are.
const HLS_VIDEO: &[&str] = &["h264", "hevc", "av1", "vp9"];
const HLS_AUDIO: &[&str] = &["aac", "mp3", "ac3", "eac3", "opus", "flac"];

pub fn container_name(format: &str) -> String {
    if format.contains("mp4") || format.contains("mov") {
        "mp4".into()
    } else if format.contains("matroska") {
        // ffprobe can't tell WebM from Matroska by name.
        "mkv".into()
    } else {
        format.into()
    }
}

#[derive(sqlx::FromRow)]
struct FileRow {
    id: i64,
    root_path: String,
    relative_path: String,
    size: i64,
    container: Option<String>,
    duration_seconds: Option<f64>,
}

#[derive(sqlx::FromRow)]
struct StreamRow {
    stream_index: i64,
    kind: String,
    codec: Option<String>,
    language: Option<String>,
    channels: Option<i64>,
    width: Option<i64>,
    height: Option<i64>,
    hdr_format: Option<String>,
    is_default: bool,
    is_forced: bool,
}

/// The item's present files, best first (tallest picture, then largest).
pub async fn versions(pool: &SqlitePool, item: i64) -> Result<Vec<Version>, sqlx::Error> {
    let files: Vec<FileRow> = sqlx::query_as(
        "SELECT f.id, r.path AS root_path, f.relative_path, f.size, f.container, f.duration_seconds
         FROM file_link l JOIN media_file f ON f.id = l.file_id JOIN library_root r ON r.id = f.root_id
         WHERE l.item_id = ? AND f.status = 'present' AND r.offline_since IS NULL",
    )
    .bind(item)
    .fetch_all(pool)
    .await?;
    let mut out = Vec::new();
    for file in files {
        out.push(version(pool, file).await?);
    }
    out.sort_by_key(|v| {
        (
            std::cmp::Reverse(v.height.unwrap_or(0)),
            std::cmp::Reverse(v.size),
        )
    });
    Ok(out)
}

/// One present file, if it's linked to something.
pub async fn version_of_file(pool: &SqlitePool, file: i64) -> Result<Option<Version>, sqlx::Error> {
    let row: Option<FileRow> = sqlx::query_as(
        "SELECT f.id, r.path AS root_path, f.relative_path, f.size, f.container, f.duration_seconds
         FROM media_file f JOIN library_root r ON r.id = f.root_id
         WHERE f.id = ? AND f.status = 'present'",
    )
    .bind(file)
    .fetch_optional(pool)
    .await?;
    match row {
        Some(row) => Ok(Some(version(pool, row).await?)),
        None => Ok(None),
    }
}

async fn version(pool: &SqlitePool, file: FileRow) -> Result<Version, sqlx::Error> {
    let streams: Vec<StreamRow> = sqlx::query_as(
        "SELECT stream_index, kind, codec, language, channels, width, height, hdr_format, is_default, is_forced
         FROM media_stream WHERE file_id = ? ORDER BY stream_index",
    )
    .bind(file.id)
    .fetch_all(pool)
    .await?;
    let sidecars: Vec<(i64, String, Option<String>, bool, bool)> = sqlx::query_as(
        "SELECT id, relative_path, language, is_forced, is_hearing_impaired FROM sidecar
         WHERE file_id = ? AND kind = 'subtitle' AND location = 'root' ORDER BY relative_path",
    )
    .bind(file.id)
    .fetch_all(pool)
    .await?;

    let video = streams.iter().find(|s| s.kind == "video");
    let audio = streams
        .iter()
        .filter(|s| s.kind == "audio")
        .map(|s| AudioTrack {
            index: s.stream_index,
            codec: s.codec.clone(),
            language: s.language.clone(),
            channels: s.channels,
            is_default: s.is_default,
        })
        .collect();
    let mut subtitles: Vec<SubtitleTrack> = streams
        .iter()
        .filter(|s| s.kind == "subtitle")
        .map(|s| SubtitleTrack {
            id: format!("stream:{}", s.stream_index),
            codec: s.codec.clone(),
            language: s.language.clone(),
            forced: s.is_forced,
            hearing_impaired: false,
            text: !s
                .codec
                .as_deref()
                .is_some_and(|c| PICTURE_SUBTITLES.contains(&c)),
            external: false,
        })
        .collect();
    subtitles.extend(
        sidecars
            .into_iter()
            .map(|(id, path, language, forced, hi)| {
                let ext = Path::new(&path)
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(str::to_ascii_lowercase);
                SubtitleTrack {
                    id: format!("sidecar:{id}"),
                    text: ext.as_deref() != Some("sub") && ext.as_deref() != Some("sup"),
                    codec: ext,
                    language,
                    forced,
                    hearing_impaired: hi,
                    external: true,
                }
            }),
    );
    let file_name = Path::new(&file.relative_path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok(Version {
        file_id: file.id,
        file_name,
        size: file.size,
        container: file.container.as_deref().map(container_name),
        duration_seconds: file.duration_seconds,
        video_stream: video.map(|v| v.stream_index),
        video_codec: video.and_then(|v| v.codec.clone()),
        width: video.and_then(|v| v.width),
        height: video.and_then(|v| v.height),
        hdr_format: video.and_then(|v| v.hdr_format.clone()),
        audio,
        subtitles,
        path: Path::new(&file.root_path).join(&file.relative_path),
    })
}

/// A sidecar subtitle's file, if it's known and next to its video.
pub async fn sidecar_path(pool: &SqlitePool, id: i64) -> Result<Option<PathBuf>, sqlx::Error> {
    let row: Option<(String, String)> = sqlx::query_as(
        "SELECT r.path, s.relative_path FROM sidecar s JOIN library_root r ON r.id = s.root_id
         WHERE s.id = ? AND s.kind = 'subtitle' AND s.location = 'root'",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(root, relative)| Path::new(&root).join(relative)))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Direct,
    Remux,
    Transcode,
}

/// What ffmpeg does to the video.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VideoPlan {
    Copy,
    Encode {
        /// Scale down to this height.
        height: Option<i64>,
        tonemap: bool,
        /// A picture subtitle stream to draw onto the video.
        burn_subtitle: Option<i64>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    pub mode: Mode,
    pub video: VideoPlan,
    /// The audio stream, and whether it's converted to stereo AAC.
    pub audio: Option<(i64, bool)>,
    /// Why it isn't played directly, for the player to explain.
    pub reasons: Vec<String>,
}

fn upper(codec: &str) -> String {
    match codec {
        "hevc" => "HEVC".into(),
        "h264" => "H.264".into(),
        "eac3" => "E-AC-3".into(),
        "ac3" => "AC-3".into(),
        "truehd" => "TrueHD".into(),
        other => other.to_uppercase(),
    }
}

fn has(list: &[String], value: &str) -> bool {
    list.iter().any(|v| v.eq_ignore_ascii_case(value))
}

/// Decides how `version` plays on a client with `caps`.
pub fn decide(version: &Version, choice: &Choice, caps: &ClientCaps) -> Decision {
    let mut reasons = Vec::new();
    let codec = version.video_codec.as_deref().unwrap_or("unknown");

    // Video.
    let mut copy_video = version.video_stream.is_some();
    let mut tonemap = false;
    if !has(&caps.video_codecs, codec) {
        reasons.push(format!("This browser can't play {} video.", upper(codec)));
        copy_video = false;
    } else if !HLS_VIDEO.contains(&codec)
        && !has(&caps.containers, version.container.as_deref().unwrap_or(""))
    {
        reasons.push(format!(
            "{} video can't be streamed without converting.",
            upper(codec)
        ));
        copy_video = false;
    }
    if version.hdr_format.is_some() && !caps.hdr {
        reasons.push("HDR is tone-mapped for this screen.".into());
        copy_video = false;
        tonemap = true;
    }
    let height = match (choice.max_height, version.height) {
        (Some(max), Some(h)) if h > max => {
            reasons.push(format!("Converted down to {max}p."));
            copy_video = false;
            Some(max)
        }
        _ => None,
    };
    let burn = choice
        .subtitle
        .as_deref()
        .and_then(|s| version.subtitles.iter().find(|t| t.id == s))
        .filter(|t| !t.text);
    let burn_subtitle = match burn {
        Some(track) if !track.external => {
            reasons.push("Picture subtitles are drawn onto the video.".into());
            copy_video = false;
            track
                .id
                .strip_prefix("stream:")
                .and_then(|i| i.parse().ok())
        }
        _ => None,
    };
    if !copy_video && version.hdr_format.is_some() {
        // Encoding to 8-bit H.264 always needs SDR.
        tonemap = true;
    }

    // Audio.
    let chosen = choice
        .audio_stream
        .and_then(|i| version.audio.iter().find(|a| a.index == i))
        .or_else(|| version.audio.iter().find(|a| a.is_default))
        .or(version.audio.first());
    let first_audio = version.audio.first().map(|a| a.index);
    let mut convert_audio = false;
    if let Some(track) = chosen {
        let codec = track.codec.as_deref().unwrap_or("unknown");
        if !has(&caps.audio_codecs, codec) || !HLS_AUDIO.contains(&codec) {
            reasons.push(format!("{} audio is converted to AAC.", upper(codec)));
            convert_audio = true;
        }
    }

    // Container: only for playing directly.
    let container = version.container.as_deref().unwrap_or("unknown");
    let container_ok = has(&caps.containers, container);
    let other_audio = chosen.map(|a| a.index) != first_audio;
    let mode = if !copy_video {
        Mode::Transcode
    } else if !container_ok || convert_audio || other_audio {
        if !container_ok {
            reasons.push(format!(
                "The {container} container is repackaged for this browser."
            ));
        }
        if other_audio && !convert_audio {
            reasons.push("Another audio track is picked, so the file is repackaged.".into());
        }
        Mode::Remux
    } else {
        Mode::Direct
    };
    let video = if copy_video {
        VideoPlan::Copy
    } else {
        VideoPlan::Encode {
            height,
            tonemap,
            burn_subtitle,
        }
    };
    Decision {
        mode,
        video,
        audio: chosen.map(|a| (a.index, convert_audio)),
        reasons,
    }
}

/// Hardware video encoding. Decoding and filters stay in software, so anything
/// ffmpeg reads works; only the H.264 encode moves to the GPU.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HwAccel {
    Software,
    Vaapi { device: String },
    Nvenc,
    Qsv,
}

impl HwAccel {
    /// `none`, `vaapi` (or `vaapi:/dev/dri/renderD129`), `nvenc` or `qsv`.
    pub fn parse(value: &str) -> Option<HwAccel> {
        match value.trim() {
            "" | "none" | "software" => Some(HwAccel::Software),
            "vaapi" => Some(HwAccel::Vaapi {
                device: "/dev/dri/renderD128".into(),
            }),
            "nvenc" => Some(HwAccel::Nvenc),
            "qsv" => Some(HwAccel::Qsv),
            other => other.strip_prefix("vaapi:").map(|device| HwAccel::Vaapi {
                device: device.into(),
            }),
        }
    }

    fn global_args(&self) -> Vec<String> {
        match self {
            HwAccel::Vaapi { device } => vec!["-vaapi_device".into(), device.clone()],
            HwAccel::Qsv => ["-init_hw_device", "qsv=hw", "-filter_hw_device", "hw"]
                .map(String::from)
                .to_vec(),
            _ => Vec::new(),
        }
    }

    /// The end of the filter chain: pixel format, and the upload to the GPU.
    fn upload(&self) -> &'static str {
        match self {
            HwAccel::Software | HwAccel::Nvenc => "format=yuv420p",
            HwAccel::Vaapi { .. } => "format=nv12,hwupload",
            HwAccel::Qsv => "format=nv12,hwupload=extra_hw_frames=64",
        }
    }

    /// ffmpeg's name for the H.264 encoder.
    pub fn encoder(&self) -> &'static str {
        match self {
            HwAccel::Software => "libx264",
            HwAccel::Vaapi { .. } => "h264_vaapi",
            HwAccel::Nvenc => "h264_nvenc",
            HwAccel::Qsv => "h264_qsv",
        }
    }

    fn encoder_args(&self, bitrate: u32) -> Vec<String> {
        let rate = format!("{bitrate}k");
        let buffer = format!("{}k", bitrate * 2);
        let mut args = vec!["-c:v".to_string(), self.encoder().to_string()];
        args.extend(match self {
            // x264 holds back a frame per thread before writing any; its default
            // on a many-core machine (1.5 per core) delays the first segment.
            HwAccel::Software => [
                "-preset",
                "veryfast",
                "-crf",
                "22",
                "-profile:v",
                "high",
                "-threads:v",
                "8",
            ]
            .map(String::from)
            .to_vec(),
            HwAccel::Vaapi { .. } => ["-b:v", &rate].map(String::from).to_vec(),
            HwAccel::Nvenc => ["-preset", "p4", "-cq", "23", "-b:v", "0"]
                .map(String::from)
                .to_vec(),
            HwAccel::Qsv => ["-preset", "veryfast", "-b:v", &rate]
                .map(String::from)
                .to_vec(),
        });
        args.extend(["-maxrate".into(), rate, "-bufsize".into(), buffer]);
        args
    }
}

impl HwAccel {
    /// ffmpeg arguments that encode a moment of test picture, to check the
    /// hardware works before relying on it.
    pub fn test_args(&self) -> Vec<String> {
        let mut args: Vec<String> = ["-nostdin", "-hide_banner", "-loglevel", "error"]
            .map(String::from)
            .to_vec();
        args.extend(self.global_args());
        args.extend(
            [
                "-f",
                "lavfi",
                "-i",
                "testsrc2=duration=0.2:size=320x240:rate=10",
                "-vf",
            ]
            .map(String::from),
        );
        args.push(self.upload().into());
        args.extend(self.encoder_args(1000));
        args.extend(["-f", "null", "-"].map(String::from));
        args
    }
}

/// The bitrate of audio converted to stereo AAC, in kbit/s.
const AUDIO_KBPS: u32 = 192;

/// What an HLS stream is made of, for the player to show beside the source.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct StreamOutput {
    /// The video codec sent: the file's own when copied, otherwise `h264`.
    pub video_codec: Option<String>,
    /// The H.264 encoder (`libx264`, `h264_nvenc`…); none when the video is copied.
    pub video_encoder: Option<String>,
    /// The height the video is encoded at; the file's own when copied.
    pub height: Option<i64>,
    /// The encoder's bitrate ceiling, in kbit/s.
    pub max_bitrate_kbps: Option<u32>,
    pub tonemap: bool,
    /// A picture subtitle drawn onto the video.
    pub burned_subtitle: bool,
    /// The audio codec sent: the track's own when copied, otherwise `aac`.
    pub audio_codec: Option<String>,
    pub audio_channels: Option<i64>,
    /// Set when the audio is converted.
    pub audio_bitrate_kbps: Option<u32>,
    pub segment_seconds: u32,
    /// The length of the first segments, when shorter.
    pub initial_segment_seconds: Option<u32>,
}

/// What `hls_args` makes of `version` under `decision`.
pub fn stream_output(version: &Version, decision: &Decision, hw: &HwAccel) -> StreamOutput {
    let audio = decision.audio.and_then(|(index, convert)| {
        Some((version.audio.iter().find(|a| a.index == index)?, convert))
    });
    let (audio_codec, audio_channels, audio_bitrate_kbps) = match audio {
        Some((_, true)) => (Some("aac".to_string()), Some(2), Some(AUDIO_KBPS)),
        Some((track, false)) => (track.codec.clone(), track.channels, None),
        None => (None, None, None),
    };
    let mut output = StreamOutput {
        video_codec: version.video_codec.clone(),
        video_encoder: None,
        height: version.height,
        max_bitrate_kbps: None,
        tonemap: false,
        burned_subtitle: false,
        audio_codec,
        audio_channels,
        audio_bitrate_kbps,
        segment_seconds: SEGMENT_SECONDS,
        initial_segment_seconds: None,
    };
    if let VideoPlan::Encode {
        height,
        tonemap,
        burn_subtitle,
    } = &decision.video
    {
        let target = height.or(version.height).unwrap_or(1080);
        output.video_codec = Some("h264".into());
        output.video_encoder = Some(hw.encoder().into());
        output.height = Some(version.height.map_or(target, |h| h.min(target)));
        output.max_bitrate_kbps = Some(bitrate_for(target));
        output.tonemap = *tonemap;
        output.burned_subtitle = burn_subtitle.is_some();
        output.initial_segment_seconds = Some(INITIAL_SEGMENT_SECONDS);
    }
    output
}

/// A ceiling for the H.264 bitrate, in kbit/s, by output height.
fn bitrate_for(height: i64) -> u32 {
    match height {
        h if h > 1440 => 20_000,
        h if h > 1080 => 12_000,
        h if h > 720 => 8_000,
        h if h > 480 => 4_000,
        _ => 2_000,
    }
}

/// Software tone mapping from PQ or HLG to SDR BT.709.
const TONEMAP: &str = "zscale=t=linear:npl=100,format=gbrpf32le,zscale=p=bt709,tonemap=tonemap=hable:desat=0,zscale=t=bt709:m=bt709:r=tv";

/// The ffmpeg arguments (without the program) that write an HLS stream into
/// `out_dir`, starting `start` seconds in.
pub fn hls_args(
    version: &Version,
    decision: &Decision,
    start: f64,
    out_dir: &Path,
    hw: &HwAccel,
) -> Vec<String> {
    let mut args: Vec<String> = ["-nostdin", "-hide_banner", "-loglevel", "error"]
        .map(String::from)
        .to_vec();
    let encoding = matches!(decision.video, VideoPlan::Encode { .. });
    if encoding {
        args.extend(hw.global_args());
    }
    if start > 0.0 {
        args.extend(["-ss".into(), format!("{start:.3}")]);
    }
    args.extend(["-i".into(), version.path.to_string_lossy().into_owned()]);

    match (&decision.video, version.video_stream) {
        (VideoPlan::Copy, Some(v)) => {
            args.extend([
                "-map".into(),
                format!("0:{v}"),
                "-c:v".into(),
                "copy".into(),
            ]);
            if version.video_codec.as_deref() == Some("hevc") {
                // Safari only plays HEVC in fMP4 tagged hvc1.
                args.extend(["-tag:v".into(), "hvc1".into()]);
            }
        }
        (
            VideoPlan::Encode {
                height,
                tonemap,
                burn_subtitle,
            },
            Some(v),
        ) => {
            let mut graph = match burn_subtitle {
                Some(s) => format!("[0:{v}][0:{s}]overlay"),
                None => format!("[0:{v}]null"),
            };
            // Scaled down before tone mapping, the slowest filter by far: 4K to
            // 1080p first makes it a quarter of the work.
            let target = height.or(version.height).unwrap_or(1080);
            graph.push_str(&format!(",scale=-2:'min(ih,{target})'"));
            if *tonemap {
                graph.push(',');
                graph.push_str(TONEMAP);
            }
            graph.push(',');
            graph.push_str(hw.upload());
            graph.push_str("[v]");
            args.extend(["-filter_complex".into(), graph, "-map".into(), "[v]".into()]);
            args.extend(hw.encoder_args(bitrate_for(target)));
            // A keyframe at every segment boundary, so segments start cleanly:
            // every second at first, then every segment.
            let (init, seg) = (INITIAL_SEGMENT_SECONDS, SEGMENT_SECONDS);
            args.extend([
                "-force_key_frames".into(),
                format!(
                    "expr:if(lt(t,{seg}),gte(t,n_forced*{init}),gte(t,{seg}+(n_forced-{})*{seg}))",
                    seg / init
                ),
                // Only when encoding: copied video would be cut at every source
                // keyframe for the whole stream.
                "-hls_init_time".into(),
                init.to_string(),
            ]);
        }
        _ => {}
    }

    if let Some((index, convert)) = decision.audio {
        args.extend(["-map".into(), format!("0:{index}")]);
        if convert {
            args.extend(["-c:a", "aac", "-ac", "2", "-b:a"].map(String::from));
            args.push(format!("{AUDIO_KBPS}k"));
        } else {
            args.extend(["-c:a".into(), "copy".into()]);
        }
    }

    let dir = out_dir.to_string_lossy();
    args.extend(
        [
            "-sn",
            "-dn",
            "-map_metadata",
            "-1",
            "-map_chapters",
            "-1",
            "-max_muxing_queue_size",
            "2048",
            "-f",
            "hls",
            "-hls_time",
            &SEGMENT_SECONDS.to_string(),
            "-hls_segment_type",
            "fmp4",
            "-hls_playlist_type",
            "event",
            "-hls_flags",
            "independent_segments+temp_file",
            "-hls_fmp4_init_filename",
            "init.mp4",
            "-hls_segment_filename",
            &format!("{dir}/seg%05d.m4s"),
            &format!("{dir}/index.m3u8"),
        ]
        .map(String::from),
    );
    args
}

/// The ffmpeg arguments (without the program) that write one subtitle stream of
/// `input` as WebVTT to stdout, timed from `start` seconds in.
pub fn webvtt_args(input: &Path, stream: Option<i64>, start: f64) -> Vec<String> {
    let mut args: Vec<String> = ["-nostdin", "-hide_banner", "-loglevel", "error"]
        .map(String::from)
        .to_vec();
    if start > 0.0 {
        args.extend(["-ss".into(), format!("{start:.3}")]);
    }
    args.extend(["-i".into(), input.to_string_lossy().into_owned()]);
    let map = match stream {
        Some(index) => format!("0:{index}"),
        None => "0:s:0".into(),
    };
    args.extend(["-map".into(), map]);
    args.extend(["-c:s", "webvtt", "-f", "webvtt", "pipe:1"].map(String::from));
    args
}

fn parse_timestamp(s: &str) -> Option<f64> {
    let parts: Vec<&str> = s.trim().split(':').collect();
    let (h, m, sec) = match parts.as_slice() {
        [h, m, s] => (
            h.parse::<f64>().ok()?,
            m.parse::<f64>().ok()?,
            s.parse::<f64>().ok()?,
        ),
        [m, s] => (0.0, m.parse::<f64>().ok()?, s.parse::<f64>().ok()?),
        _ => return None,
    };
    Some(h * 3600.0 + m * 60.0 + sec)
}

fn format_timestamp(t: f64) -> String {
    let millis = (t * 1000.0).round() as i64;
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        millis / 3_600_000,
        millis / 60_000 % 60,
        millis / 1000 % 60,
        millis % 1000
    )
}

/// Moves WebVTT cues `offset` seconds earlier, for a stream that starts that far
/// in. Cues that end before the start are dropped; ones that straddle it are cut.
pub fn shift_vtt(vtt: &str, offset: f64) -> String {
    if offset <= 0.0 {
        return vtt.to_string();
    }
    let mut out = String::with_capacity(vtt.len());
    // Blocks are separated by blank lines; the first is the header.
    for (i, block) in vtt.replace("\r\n", "\n").split("\n\n").enumerate() {
        if i == 0 {
            out.push_str(block);
            out.push_str("\n\n");
            continue;
        }
        let mut lines = block.lines();
        let mut kept = Vec::new();
        let mut keep = true;
        for line in lines.by_ref() {
            match line.split_once("-->") {
                Some((from, rest)) => {
                    let (to, settings) = rest
                        .trim_start()
                        .split_once(' ')
                        .unwrap_or((rest.trim(), ""));
                    match (parse_timestamp(from), parse_timestamp(to)) {
                        (Some(from), Some(to)) if to > offset => {
                            let from = (from - offset).max(0.0);
                            let mut timing = format!(
                                "{} --> {}",
                                format_timestamp(from),
                                format_timestamp(to - offset)
                            );
                            if !settings.is_empty() {
                                timing.push(' ');
                                timing.push_str(settings);
                            }
                            kept.push(timing);
                        }
                        _ => keep = false,
                    }
                    break;
                }
                None => kept.push(line.to_string()),
            }
        }
        if keep {
            kept.extend(lines.map(str::to_string));
            if kept.iter().any(|l| !l.trim().is_empty()) {
                out.push_str(&kept.join("\n"));
                out.push_str("\n\n");
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chrome() -> ClientCaps {
        ClientCaps {
            containers: vec!["mp4".into(), "webm".into()],
            video_codecs: vec!["h264".into(), "vp9".into(), "av1".into()],
            audio_codecs: vec!["aac".into(), "mp3".into(), "opus".into(), "flac".into()],
            hdr: false,
        }
    }

    fn version(container: &str, video: &str, audio: &[&str]) -> Version {
        Version {
            file_id: 1,
            file_name: "f".into(),
            size: 1,
            container: Some(container.into()),
            duration_seconds: Some(60.0),
            video_stream: Some(0),
            video_codec: Some(video.into()),
            width: Some(1920),
            height: Some(1080),
            hdr_format: None,
            audio: audio
                .iter()
                .enumerate()
                .map(|(i, codec)| AudioTrack {
                    index: i as i64 + 1,
                    codec: Some((*codec).into()),
                    language: None,
                    channels: Some(2),
                    is_default: i == 0,
                })
                .collect(),
            subtitles: Vec::new(),
            path: "/media/f.mkv".into(),
        }
    }

    #[test]
    fn plays_directly_when_everything_fits() {
        let d = decide(
            &version("mp4", "h264", &["aac"]),
            &Choice::default(),
            &chrome(),
        );
        assert_eq!(
            (d.mode, d.video, d.audio),
            (Mode::Direct, VideoPlan::Copy, Some((1, false)))
        );
        assert!(d.reasons.is_empty());
    }

    #[test]
    fn remuxes_for_the_container_or_the_audio() {
        let d = decide(
            &version("mkv", "h264", &["aac"]),
            &Choice::default(),
            &chrome(),
        );
        assert_eq!(d.mode, Mode::Remux);
        let d = decide(
            &version("mp4", "h264", &["eac3"]),
            &Choice::default(),
            &chrome(),
        );
        assert_eq!((d.mode, d.audio), (Mode::Remux, Some((1, true))));
        assert_eq!(d.reasons, ["E-AC-3 audio is converted to AAC."]);
        // A second audio track can't be picked in a plain <video>.
        let choice = Choice {
            audio_stream: Some(2),
            ..Default::default()
        };
        let d = decide(&version("mp4", "h264", &["aac", "aac"]), &choice, &chrome());
        assert_eq!((d.mode, d.audio), (Mode::Remux, Some((2, false))));
    }

    #[test]
    fn describes_the_stream_it_sends() {
        let v = version("mkv", "hevc", &["eac3"]);
        let d = decide(&v, &Choice::default(), &chrome());
        let out = stream_output(&v, &d, &HwAccel::Nvenc);
        assert_eq!(
            (out.video_codec.as_deref(), out.video_encoder.as_deref()),
            (Some("h264"), Some("h264_nvenc"))
        );
        assert_eq!(
            (out.height, out.max_bitrate_kbps),
            (Some(1080), Some(8_000))
        );
        assert_eq!(
            (
                out.audio_codec.as_deref(),
                out.audio_channels,
                out.audio_bitrate_kbps
            ),
            (Some("aac"), Some(2), Some(192))
        );

        let v = version("mkv", "h264", &["aac"]);
        let d = decide(&v, &Choice::default(), &chrome());
        let out = stream_output(&v, &d, &HwAccel::Software);
        assert_eq!(
            (
                out.video_codec.as_deref(),
                out.video_encoder,
                out.audio_bitrate_kbps
            ),
            (Some("h264"), None, None)
        );
    }

    #[test]
    fn transcodes_for_codec_quality_hdr_and_picture_subtitles() {
        let d = decide(
            &version("mkv", "hevc", &["aac"]),
            &Choice::default(),
            &chrome(),
        );
        assert_eq!(d.mode, Mode::Transcode);
        assert_eq!(d.reasons[0], "This browser can't play HEVC video.");

        let mut hdr = version("mp4", "hevc", &["aac"]);
        hdr.hdr_format = Some("hdr10".into());
        let mut caps = chrome();
        caps.video_codecs.push("hevc".into());
        let d = decide(&hdr, &Choice::default(), &caps);
        assert_eq!(
            d.video,
            VideoPlan::Encode {
                height: None,
                tonemap: true,
                burn_subtitle: None
            }
        );
        caps.hdr = true;
        assert_eq!(decide(&hdr, &Choice::default(), &caps).mode, Mode::Direct);
        // Any re-encode of HDR tone-maps, even on an HDR screen.
        let lower = Choice {
            max_height: Some(720),
            ..Default::default()
        };
        assert_eq!(
            decide(&hdr, &lower, &caps).video,
            VideoPlan::Encode {
                height: Some(720),
                tonemap: true,
                burn_subtitle: None
            }
        );

        let mut pgs = version("mkv", "h264", &["aac"]);
        pgs.subtitles.push(SubtitleTrack {
            id: "stream:5".into(),
            codec: Some("hdmv_pgs_subtitle".into()),
            language: None,
            forced: false,
            hearing_impaired: false,
            text: false,
            external: false,
        });
        let choice = Choice {
            subtitle: Some("stream:5".into()),
            ..Default::default()
        };
        assert_eq!(
            decide(&pgs, &choice, &chrome()).video,
            VideoPlan::Encode {
                height: None,
                tonemap: false,
                burn_subtitle: Some(5)
            }
        );
    }

    #[test]
    fn hls_commands() {
        let v = version("mkv", "hevc", &["eac3"]);
        let d = decide(&v, &Choice::default(), &chrome());
        let args = hls_args(&v, &d, 90.0, Path::new("/tmp/s"), &HwAccel::Software).join(" ");
        assert!(args.contains("-ss 90.000 -i /media/f.mkv"), "{args}");
        assert!(
            args.contains("[0:0]null,scale=-2:'min(ih,1080)',format=yuv420p[v]"),
            "{args}"
        );
        assert!(args.contains("-c:v libx264"), "{args}");
        assert!(args.contains("-threads:v 8"), "{args}");
        assert!(
            args.contains(
                "-force_key_frames expr:if(lt(t,4),gte(t,n_forced*1),gte(t,4+(n_forced-4)*4)) -hls_init_time 1"
            ),
            "{args}"
        );
        assert!(args.contains("-map 0:1 -c:a aac -ac 2"), "{args}");
        assert!(
            args.ends_with("/tmp/s/seg%05d.m4s /tmp/s/index.m3u8"),
            "{args}"
        );

        let mut hdr = version("mkv", "hevc", &["aac"]);
        (hdr.height, hdr.hdr_format) = (Some(2160), Some("hdr10".into()));
        let d = decide(&hdr, &Choice::default(), &chrome());
        let args = hls_args(&hdr, &d, 0.0, Path::new("/tmp/s"), &HwAccel::Software).join(" ");
        assert!(
            args.contains("[0:0]null,scale=-2:'min(ih,2160)',zscale=t=linear"),
            "{args}"
        );

        let vaapi = HwAccel::parse("vaapi").unwrap();
        let args = hls_args(&v, &d, 0.0, Path::new("/tmp/s"), &vaapi).join(" ");
        assert!(args.starts_with(
            "-nostdin -hide_banner -loglevel error -vaapi_device /dev/dri/renderD128 -i"
        ));
        assert!(
            args.contains("format=nv12,hwupload[v]") && args.contains("-c:v h264_vaapi"),
            "{args}"
        );

        let mut caps = chrome();
        caps.video_codecs.push("hevc".into());
        let d = decide(&v, &Choice::default(), &caps);
        let args = hls_args(&v, &d, 0.0, Path::new("/tmp/s"), &HwAccel::Software).join(" ");
        assert!(args.contains("-map 0:0 -c:v copy -tag:v hvc1"), "{args}");
        assert!(!args.contains("-hls_init_time"), "{args}");
        assert_eq!(HwAccel::parse("bogus"), None);
    }

    #[test]
    fn shifting_subtitles() {
        let vtt = "WEBVTT\n\n1\n00:00:05.000 --> 00:00:07.000\nGone\n\n2\n00:01:39.500 --> 00:01:42.250 align:start\nCut\n\n00:01:45.000 --> 00:01:46.000\nLater\n";
        assert_eq!(
            shift_vtt(vtt, 100.0),
            "WEBVTT\n\n2\n00:00:00.000 --> 00:00:02.250 align:start\nCut\n\n00:00:05.000 --> 00:00:06.000\nLater\n\n"
        );
        assert_eq!(shift_vtt(vtt, 0.0), vtt);
    }
}
