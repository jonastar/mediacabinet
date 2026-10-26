//! HTTP API, mounted under `/api`. Every handler is registered with utoipa so it appears in the OpenAPI spec.

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;
use utoipa::{OpenApi, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::AppState;

mod auth;
mod catalog;
mod error;
mod events;
mod integration;
mod items;
mod library;
mod review;
mod wanted;

pub use error::ApiError;

#[derive(OpenApi)]
#[openapi(info(title = "Media Cabinet API"))]
struct ApiDoc;

fn routes() -> OpenApiRouter<AppState> {
    let api = OpenApiRouter::new()
        .routes(routes!(health))
        .routes(routes!(catalog::home))
        .routes(routes!(catalog::list_series))
        .routes(routes!(catalog::series_detail))
        .routes(routes!(catalog::series_episodes))
        .routes(routes!(catalog::set_series_view))
        .routes(routes!(catalog::set_item_monitor))
        .routes(routes!(library::list_libraries, library::create_library))
        .routes(routes!(library::rename_library, library::delete_library))
        .routes(routes!(library::add_root))
        .routes(routes!(library::update_root, library::remove_root))
        .routes(routes!(library::scan_root))
        .routes(routes!(library::propose_template))
        .routes(routes!(library::check_template))
        .routes(routes!(library::list_folders))
        .routes(routes!(library::placement_example))
        .routes(routes!(review::queue))
        .routes(routes!(review::accept))
        .routes(routes!(review::reject))
        .routes(routes!(review::link_file))
        .routes(routes!(review::ignore_file))
        .routes(routes!(review::identify_folder))
        .routes(routes!(review::search))
        .routes(routes!(items::list_movies))
        .routes(routes!(items::movie_detail))
        .routes(routes!(items::search))
        .routes(routes!(items::artwork))
        .routes(routes!(items::edit_item))
        .routes(routes!(items::set_locks))
        .routes(routes!(items::set_tags))
        .routes(routes!(items::refresh_item))
        .routes(routes!(items::set_match_ordering))
        .routes(routes!(items::rematch_series))
        .routes(routes!(items::rescan_series))
        .routes(routes!(wanted::list))
        .routes(routes!(wanted::calendar))
        .routes(routes!(wanted::set_status, wanted::clear_status))
        .routes(routes!(wanted::set_monitoring))
        .routes(routes!(wanted::quality_profiles))
        .routes(routes!(integration::item))
        .routes(routes!(integration::gaps))
        .routes(routes!(integration::add_series))
        .routes(routes!(integration::add_movie))
        .routes(routes!(integration::list_keys, integration::create_key))
        .routes(routes!(integration::revoke_key))
        .routes(routes!(integration::sonarr_list))
        .routes(routes!(integration::radarr_list))
        .routes(routes!(integration::arr_webhook))
        .routes(routes!(events::events))
        .routes(routes!(events::list_webhooks, events::create_webhook))
        .routes(routes!(events::update_webhook, events::delete_webhook))
        .routes(routes!(events::test_webhook));
    OpenApiRouter::with_openapi(ApiDoc::openapi()).nest("/api", api)
}

pub fn router(state: AppState) -> Router {
    let (api, spec) = routes().with_state(state).split_for_parts();
    api.route("/api/openapi.json", get(move || async move { Json(spec) }))
}

/// The OpenAPI document, for generating clients without running the server.
pub fn spec() -> utoipa::openapi::OpenApi {
    routes().split_for_parts().1
}

#[derive(Serialize, ToSchema)]
struct Health {
    status: &'static str,
    version: &'static str,
}

/// Liveness check that also confirms the database answers.
#[utoipa::path(get, path = "/health", responses((status = 200, body = Health)))]
async fn health(State(state): State<AppState>) -> Json<Health> {
    let ok = sqlx::query("SELECT 1").execute(&state.db).await.is_ok();
    Json(Health {
        status: if ok {
            "ok"
        } else {
            "degraded"
        },
        version: env!("CARGO_PKG_VERSION"),
    })
}
