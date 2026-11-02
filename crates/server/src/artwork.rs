//! Provider artwork, fetched once and kept in the data folder, so posters keep
//! working offline after they've been seen.

use std::path::PathBuf;
use std::time::Duration;

use axum::http::header;
use axum::response::{IntoResponse, Response};

use crate::api::ApiError;

#[derive(Clone)]
pub struct ArtworkCache {
    dir: PathBuf,
    http: reqwest::Client,
}

/// TMDB image sizes per kind (people's photos are `profile`): big enough for the UI, small enough to load fast.
fn size(kind: &str) -> &'static str {
    match kind {
        "backdrop" => "w1280",
        "still" => "w300",
        "profile" => "w342",
        _ => "w500",
    }
}

impl ArtworkCache {
    pub fn new(dir: PathBuf) -> ArtworkCache {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .build()
            .expect("HTTP client builds");
        ArtworkCache { dir, http }
    }

    pub async fn get(&self, provider: &str, kind: &str, path: &str) -> Result<Response, ApiError> {
        // Provider paths look like "/abc123.jpg"; anything else is refused.
        let safe = path.starts_with('/')
            && !path.contains("..")
            && path[1..]
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
        if provider != "tmdb" || !safe {
            return Err(ApiError::NotFound);
        }
        let size = size(kind);
        let file = self.dir.join(provider).join(size).join(&path[1..]);
        let bytes = match tokio::fs::read(&file).await {
            Ok(bytes) => bytes,
            Err(_) => {
                let url = format!("https://image.tmdb.org/t/p/{size}{path}");
                let response = self.http.get(&url).send().await.map_err(|e| {
                    ApiError::Unavailable(format!("artwork unavailable: {}", e.without_url()))
                })?;
                if !response.status().is_success() {
                    return Err(ApiError::NotFound);
                }
                let bytes = response
                    .bytes()
                    .await
                    .map_err(|e| ApiError::Unavailable(e.without_url().to_string()))?
                    .to_vec();
                if let Some(parent) = file.parent() {
                    let _ = tokio::fs::create_dir_all(parent).await;
                }
                if let Err(e) = tokio::fs::write(&file, &bytes).await {
                    tracing::warn!(error = %e, "can't cache artwork");
                }
                bytes
            }
        };
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        Ok((
            [
                (header::CONTENT_TYPE, mime.as_ref().to_string()),
                (header::CACHE_CONTROL, "public, max-age=86400".to_string()),
            ],
            bytes,
        )
            .into_response())
    }
}
