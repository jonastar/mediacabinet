//! Provider artwork over HTTP. Keeping it is `cabinet_core::artwork`'s job.

use axum::http::header;
use axum::response::{IntoResponse, Response};
use cabinet_core::artwork::{ArtworkStore, FetchError};

use crate::api::ApiError;

#[derive(Clone)]
pub struct ArtworkCache(pub ArtworkStore);

impl ArtworkCache {
    pub async fn get(&self, provider: &str, kind: &str, path: &str) -> Result<Response, ApiError> {
        let bytes = self
            .0
            .get(provider, kind, path)
            .await
            .map_err(|e| match e {
                FetchError::NotFound => ApiError::NotFound,
                e @ FetchError::Unreachable(_) => ApiError::Unavailable(e.to_string()),
            })?;
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
