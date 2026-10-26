//! API keys for external tools: `X-Api-Key: <key>` or `?apikey=<key>`.
//!
//! Endpoints made for tools (the *arr lists and webhooks, imports) require a key.
//! Endpoints the web UI uses accept one but don't require it yet; logins come in
//! Phase 7, after which every request needs a session or a key.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;

use super::ApiError;
use crate::AppState;

/// Extractor that rejects requests without a valid API key.
pub struct ApiKeyAuth;

impl FromRequestParts<AppState> for ApiKeyAuth {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get("x-api-key")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let query = parts
            .uri
            .query()
            .and_then(|q| url_param(q, "apikey").or_else(|| url_param(q, "api_key")));
        let Some(key) = header.or(query) else {
            return Err(ApiError::Unauthorized(
                "An API key is required: X-Api-Key header or ?apikey=.".into(),
            ));
        };
        match cabinet_core::apikeys::verify(&state.db, &key).await? {
            Some(_) => Ok(ApiKeyAuth),
            None => Err(ApiError::Unauthorized("That API key isn't valid.".into())),
        }
    }
}

fn url_param(query: &str, name: &str) -> Option<String> {
    query.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        (k == name).then(|| v.to_string())
    })
}
