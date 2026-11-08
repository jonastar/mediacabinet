//! Serves the built web UI, and the TV app at `/tv-mode/` for TV browsers, both
//! embedded in release builds and read from disk in debug builds.

use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Redirect, Response};
use rust_embed::{EmbeddedFile, RustEmbed};

#[derive(RustEmbed)]
#[folder = "../../web/dist"]
#[allow_missing = true]
struct Assets;

/// The TV app from `players/web`. Not `/tv`, which is the web UI's shows.
#[derive(RustEmbed)]
#[folder = "../../players/web/dist"]
#[allow_missing = true]
struct TvAssets;

const TV_PATH: &str = "tv-mode";

pub async fn serve(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path.starts_with("api/") {
        return StatusCode::NOT_FOUND.into_response();
    }
    if path == TV_PATH {
        // The app's files are relative to its folder, so it needs the slash.
        return Redirect::permanent(&format!("/{TV_PATH}/")).into_response();
    }
    if let Some(rest) = path.strip_prefix(TV_PATH).and_then(|p| p.strip_prefix('/')) {
        return app(
            rest,
            TvAssets::get,
            "TV app not built. Run `pnpm build` in players/web/.",
        );
    }
    app(
        path,
        Assets::get,
        "Web UI not built. Run `pnpm build` in web/.",
    )
}

/// A file of one app, or its shell for anything else, since its routes are its own.
fn app(path: &str, get: fn(&str) -> Option<EmbeddedFile>, missing: &'static str) -> Response {
    if let Some(file) = get(path).filter(|_| !path.is_empty()) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        let cache = if path.starts_with("assets/") {
            "public, max-age=31536000, immutable"
        } else {
            "no-cache"
        };
        return (
            [
                (header::CONTENT_TYPE, mime.as_ref()),
                (header::CACHE_CONTROL, cache),
            ],
            file.data,
        )
            .into_response();
    }
    match get("index.html") {
        Some(index) => (
            [
                (header::CONTENT_TYPE, "text/html; charset=utf-8"),
                (header::CACHE_CONTROL, "no-cache"),
            ],
            index.data,
        )
            .into_response(),
        None => (StatusCode::NOT_FOUND, missing).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn tv_mode_needs_its_slash_and_leaves_the_shows_alone() {
        let bare = serve(Uri::from_static("/tv-mode")).await;
        assert_eq!(bare.status(), StatusCode::PERMANENT_REDIRECT);
        assert_eq!(bare.headers()[header::LOCATION], "/tv-mode/");

        // `/tv-modes` and `/tv/1` are the web UI's, not the TV app's.
        for path in ["/tv-modes", "/tv/1"] {
            let response = serve(Uri::from_static(path)).await;
            assert_ne!(response.status(), StatusCode::PERMANENT_REDIRECT, "{path}");
        }
    }
}
