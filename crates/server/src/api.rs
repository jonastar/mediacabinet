//! HTTP API, mounted under `/api`. Every handler is registered with utoipa so it appears in the OpenAPI spec.

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;
use utoipa::{OpenApi, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::AppState;

#[derive(OpenApi)]
#[openapi(info(title = "Media Cabinet API"))]
struct ApiDoc;

fn routes() -> OpenApiRouter<AppState> {
    let api = OpenApiRouter::new().routes(routes!(health));
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
        status: if ok { "ok" } else { "degraded" },
        version: env!("CARGO_PKG_VERSION"),
    })
}
