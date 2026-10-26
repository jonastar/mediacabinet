//! Playing things: the versions of an item, deciding how to play one, direct
//! streams, HLS sessions and subtitles as WebVTT.

use std::path::{Path as FsPath, PathBuf};

use axum::Json;
use axum::body::Body;
use axum::extract::{Path, Query, Request, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use cabinet_core::catalog::{self, PlayContext};
use cabinet_core::playback::{self, Choice, ClientCaps, Mode, Version};
use cabinet_core::watch::{self, ItemWatch};
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use tower::ServiceExt;
use tower_http::services::ServeFile;
use utoipa::{IntoParams, ToSchema};

use super::ApiError;
use super::auth::Viewer;
use super::error::ErrorBody;
use crate::AppState;

/// The item's playable files, best first.
#[utoipa::path(
    get,
    path = "/items/{id}/versions",
    tag = "playback",
    params(("id" = i64, Path)),
    responses((status = 200, body = Vec<Version>))
)]
pub async fn versions(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Vec<Version>>, ApiError> {
    Ok(Json(playback::versions(&state.db, id).await?))
}

#[derive(Deserialize, ToSchema)]
pub struct PlayRequest {
    #[serde(flatten)]
    choice: Choice,
    #[serde(default)]
    caps: ClientCaps,
    /// Where to start; defaults to the resume position.
    start_seconds: Option<f64>,
}

#[derive(Serialize, ToSchema)]
pub struct SubtitleSource {
    /// `stream:<index>` or `sidecar:<id>`.
    id: String,
    language: Option<String>,
    forced: bool,
    hearing_impaired: bool,
    /// WebVTT, timed to the stream. None when burned into the video.
    url: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct PlayPlan {
    mode: Mode,
    /// Why it isn't played directly.
    reasons: Vec<String>,
    /// The file itself (direct) or an HLS playlist.
    url: String,
    /// The HLS session, to stop when done.
    session_id: Option<String>,
    /// The stream's time 0 is this far into the file. Zero when direct: seek
    /// to `start_seconds` instead.
    offset_seconds: f64,
    start_seconds: f64,
    version: Version,
    audio_stream: Option<i64>,
    subtitle: Option<String>,
    subtitles: Vec<SubtitleSource>,
    /// The video encoder in use when transcoding: `software`, `vaapi`, `nvenc` or `qsv`.
    encoder: String,
    watch: ItemWatch,
    context: PlayContext,
}

/// Decides how to play an episode or movie on this client, and starts an HLS
/// session when it can't play the file directly.
#[utoipa::path(
    post,
    path = "/items/{id}/play",
    tag = "playback",
    params(("id" = i64, Path)),
    request_body = PlayRequest,
    responses(
        (status = 200, body = PlayPlan),
        (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody),
        (status = 503, body = ErrorBody)
    )
)]
pub async fn play(
    State(state): State<AppState>,
    Viewer(user): Viewer,
    Path(id): Path<i64>,
    Json(body): Json<PlayRequest>,
) -> Result<Json<PlayPlan>, ApiError> {
    let versions = playback::versions(&state.db, id).await?;
    let version = match body.choice.file_id {
        Some(file) => versions
            .into_iter()
            .find(|v| v.file_id == file)
            .ok_or(ApiError::NotFound)?,
        None => versions
            .into_iter()
            .next()
            .ok_or_else(|| ApiError::Conflict("There's no file to play.".into()))?,
    };
    let context = catalog::play_context(&state.db, id, Some(user), catalog::now()).await?;
    let watch = watch::get(&state.db, user, id).await?;
    let start = body
        .start_seconds
        .unwrap_or(watch.position_seconds)
        .max(0.0);
    let decision = playback::decide(&version, &body.choice, &body.caps);

    let (url, session_id, offset) = match decision.mode {
        Mode::Direct => (format!("/api/files/{}/stream", version.file_id), None, 0.0),
        Mode::Remux | Mode::Transcode => {
            let session = state
                .transcoder
                .start(user, id, &version, &decision, start)
                .await
                .map_err(ApiError::Unavailable)?;
            (
                format!("/api/play/{session}/index.m3u8"),
                Some(session),
                start,
            )
        }
    };

    let burned = match &decision.video {
        playback::VideoPlan::Encode { burn_subtitle, .. } => {
            burn_subtitle.map(|i| format!("stream:{i}"))
        }
        playback::VideoPlan::Copy => None,
    };
    let subtitles = version
        .subtitles
        .iter()
        .filter(|t| t.text || burned.as_deref() == Some(t.id.as_str()))
        .map(|t| {
            let url = t.text.then(|| {
                let base = match t.id.split_once(':') {
                    Some(("sidecar", sidecar)) => format!("/api/sidecars/{sidecar}/subtitles"),
                    _ => format!(
                        "/api/files/{}/subtitles/{}",
                        version.file_id,
                        t.id.trim_start_matches("stream:")
                    ),
                };
                format!("{base}?offset={offset}")
            });
            SubtitleSource {
                id: t.id.clone(),
                language: t.language.clone(),
                forced: t.forced,
                hearing_impaired: t.hearing_impaired,
                url,
            }
        })
        .collect();
    let encoder = match state.transcoder.hardware() {
        playback::HwAccel::Software => "software",
        playback::HwAccel::Vaapi { .. } => "vaapi",
        playback::HwAccel::Nvenc => "nvenc",
        playback::HwAccel::Qsv => "qsv",
    };
    Ok(Json(PlayPlan {
        mode: decision.mode,
        reasons: decision.reasons,
        url,
        session_id,
        offset_seconds: offset,
        start_seconds: start,
        audio_stream: decision.audio.map(|(i, _)| i),
        subtitle: body.choice.subtitle,
        subtitles,
        version,
        encoder: encoder.into(),
        watch,
        context,
    }))
}

/// A file as it is, with range requests, for playing directly.
#[utoipa::path(
    get,
    path = "/files/{id}/stream",
    tag = "playback",
    params(("id" = i64, Path)),
    responses((status = 200, content_type = "application/octet-stream"), (status = 206), (status = 404, body = ErrorBody))
)]
pub async fn stream(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    request: Request,
) -> Result<Response, ApiError> {
    let version = playback::version_of_file(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let response = ServeFile::new(&version.path)
        .oneshot(request)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(response.map(Body::new))
}

/// A file of an HLS session: the playlist, the init segment or a segment.
#[utoipa::path(
    get,
    path = "/play/{id}/{file}",
    tag = "playback",
    params(("id" = String, Path), ("file" = String, Path)),
    responses((status = 200), (status = 404, body = ErrorBody))
)]
pub async fn session_file(
    State(state): State<AppState>,
    Viewer(user): Viewer,
    Path((id, file)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    let path = state
        .transcoder
        .file(user, &id, &file)
        .ok_or(ApiError::NotFound)?;
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|_| ApiError::NotFound)?;
    let (content_type, cache) = if file.ends_with(".m3u8") {
        ("application/vnd.apple.mpegurl", "no-cache")
    } else {
        ("video/mp4", "private, max-age=3600")
    };
    Ok((
        [
            (header::CONTENT_TYPE, content_type),
            (header::CACHE_CONTROL, cache),
        ],
        bytes,
    )
        .into_response())
}

/// Stops an HLS session.
#[utoipa::path(
    delete,
    path = "/play/{id}",
    tag = "playback",
    params(("id" = String, Path)),
    responses((status = 204), (status = 404, body = ErrorBody))
)]
pub async fn stop(
    State(state): State<AppState>,
    Viewer(user): Viewer,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    if state.transcoder.stop(user, &id).await {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}

#[derive(Deserialize, IntoParams)]
pub struct SubtitleQuery {
    /// The stream starts this far in; cues move earlier by as much.
    #[serde(default)]
    offset: f64,
}

/// Converts subtitles to WebVTT once, caching the result.
async fn webvtt(
    state: &AppState,
    input: &FsPath,
    stream: Option<i64>,
    cache: PathBuf,
) -> Result<String, ApiError> {
    if let Ok(text) = tokio::fs::read_to_string(&cache).await {
        return Ok(text);
    }
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(300),
        Command::new(state.transcoder.ffmpeg())
            .args(playback::webvtt_args(input, stream, 0.0))
            .kill_on_drop(true)
            .output(),
    )
    .await
    .map_err(|_| ApiError::Unavailable("Reading the subtitles took too long.".into()))?
    .map_err(|e| ApiError::Unavailable(format!("can't run ffmpeg: {e}")))?;
    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        return Err(ApiError::Conflict(format!(
            "ffmpeg couldn't read the subtitles: {}",
            error.trim()
        )));
    }
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    if let Some(dir) = cache.parent() {
        let _ = tokio::fs::create_dir_all(dir).await;
        let partial = cache.with_extension("part");
        if tokio::fs::write(&partial, &text).await.is_ok() {
            let _ = tokio::fs::rename(&partial, &cache).await;
        }
    }
    Ok(text)
}

fn vtt_response(text: &str, offset: f64) -> Response {
    (
        [(header::CONTENT_TYPE, "text/vtt; charset=utf-8")],
        playback::shift_vtt(text, offset),
    )
        .into_response()
}

/// An embedded subtitle stream as WebVTT.
#[utoipa::path(
    get,
    path = "/files/{id}/subtitles/{index}",
    tag = "playback",
    params(("id" = i64, Path), ("index" = i64, Path), SubtitleQuery),
    responses((status = 200, content_type = "text/vtt"), (status = 404, body = ErrorBody))
)]
pub async fn file_subtitles(
    State(state): State<AppState>,
    Path((id, index)): Path<(i64, i64)>,
    Query(query): Query<SubtitleQuery>,
) -> Result<Response, ApiError> {
    let version = playback::version_of_file(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let track = format!("stream:{index}");
    if !version.subtitles.iter().any(|t| t.id == track && t.text) {
        return Err(ApiError::NotFound);
    }
    let modified = file_modified(&version.path).await;
    let cache = state
        .cache
        .join("subtitles")
        .join(format!("{id}-{index}-{}-{modified}.vtt", version.size));
    let text = webvtt(&state, &version.path, Some(index), cache).await?;
    Ok(vtt_response(&text, query.offset))
}

/// A subtitle file next to the video, as WebVTT.
#[utoipa::path(
    get,
    path = "/sidecars/{id}/subtitles",
    tag = "playback",
    params(("id" = i64, Path), SubtitleQuery),
    responses((status = 200, content_type = "text/vtt"), (status = 404, body = ErrorBody))
)]
pub async fn sidecar_subtitles(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(query): Query<SubtitleQuery>,
) -> Result<Response, ApiError> {
    let path = playback::sidecar_path(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let modified = file_modified(&path).await;
    let cache = state
        .cache
        .join("subtitles")
        .join(format!("sidecar-{id}-{modified}.vtt"));
    let text = webvtt(&state, &path, None, cache).await?;
    Ok(vtt_response(&text, query.offset))
}

async fn file_modified(path: &FsPath) -> u64 {
    tokio::fs::metadata(path)
        .await
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
