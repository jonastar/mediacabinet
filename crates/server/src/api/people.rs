//! People in series, episodes and movies, and the rest of what providers say
//! about an item: companies, ratings, tagline and age rating.

use axum::Json;
use axum::extract::{Path, State};
use axum::response::Response;
use cabinet_core::catalog;
use cabinet_core::catalog::people::{self, About, PersonDetail};

use super::ApiError;
use super::error::ErrorBody;
use crate::AppState;

/// A series', episode's or movie's cast and crew, networks, studios, ratings,
/// tagline and age rating.
#[utoipa::path(
    get,
    path = "/items/{id}/about",
    tag = "catalog",
    params(("id" = i64, Path)),
    responses((status = 200, body = About), (status = 404, body = ErrorBody))
)]
pub async fn about(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<About>, ApiError> {
    Ok(Json(people::about(&state.db, id).await?))
}

/// Someone credited in the library: their biography (fetched from the provider
/// the first time) and everything in the library they're in.
#[utoipa::path(
    get,
    path = "/people/{id}",
    tag = "catalog",
    params(("id" = i64, Path)),
    responses((status = 200, body = PersonDetail), (status = 404, body = ErrorBody))
)]
pub async fn person(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<PersonDetail>, ApiError> {
    people::fetch_details(&state.db, &state.provider, id, catalog::now()).await?;
    Ok(Json(people::person(&state.db, id).await?))
}

/// Someone's photo, cached locally after first use.
#[utoipa::path(
    get,
    path = "/people/{id}/photo",
    tag = "catalog",
    params(("id" = i64, Path)),
    responses((status = 200, content_type = "image/*"), (status = 404, body = ErrorBody))
)]
pub async fn photo(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Response, ApiError> {
    let row: Option<(String, Option<String>)> =
        sqlx::query_as("SELECT provider, photo FROM person WHERE id = ?")
            .bind(id)
            .fetch_optional(&state.db)
            .await?;
    let Some((provider, Some(path))) = row else {
        return Err(ApiError::NotFound);
    };
    state.artwork.get(&provider, "profile", &path).await
}
