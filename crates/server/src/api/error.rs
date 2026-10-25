use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use cabinet_core::catalog::CatalogError;
use serde::Serialize;
use utoipa::ToSchema;

/// Error body for every non-2xx response.
#[derive(Serialize, ToSchema)]
pub struct ErrorBody {
    pub error: String,
}

#[derive(Debug)]
pub enum ApiError {
    NotFound,
    BadRequest(String),
    Internal(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, error) = match self {
            ApiError::NotFound => (StatusCode::NOT_FOUND, "not found".to_string()),
            ApiError::BadRequest(message) => (StatusCode::BAD_REQUEST, message),
            ApiError::Internal(message) => {
                tracing::error!(error = message, "request failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal error".to_string(),
                )
            }
        };
        (status, Json(ErrorBody { error })).into_response()
    }
}

impl From<CatalogError> for ApiError {
    fn from(err: CatalogError) -> Self {
        match err {
            CatalogError::NotFound => ApiError::NotFound,
            CatalogError::InvalidView(_) => ApiError::BadRequest(err.to_string()),
            CatalogError::Db(e) => ApiError::Internal(e.to_string()),
        }
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(err: sqlx::Error) -> Self {
        ApiError::Internal(err.to_string())
    }
}
