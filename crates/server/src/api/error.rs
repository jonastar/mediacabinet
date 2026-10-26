use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use cabinet_core::catalog::CatalogError;
use cabinet_core::library::LibraryError;
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
    Unauthorized(String),
    /// Signed in, but not allowed: admin only.
    Forbidden,
    Conflict(String),
    /// A dependency such as the metadata provider is unavailable.
    Unavailable(String),
    Internal(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, error) = match self {
            ApiError::NotFound => (StatusCode::NOT_FOUND, "not found".to_string()),
            ApiError::BadRequest(message) => (StatusCode::BAD_REQUEST, message),
            ApiError::Unauthorized(message) => (StatusCode::UNAUTHORIZED, message),
            ApiError::Forbidden => (
                StatusCode::FORBIDDEN,
                "Only an admin can do that.".to_string(),
            ),
            ApiError::Conflict(message) => (StatusCode::CONFLICT, message),
            ApiError::Unavailable(message) => (StatusCode::SERVICE_UNAVAILABLE, message),
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

impl From<LibraryError> for ApiError {
    fn from(err: LibraryError) -> Self {
        match err {
            LibraryError::NotFound => ApiError::NotFound,
            LibraryError::Invalid(m) => ApiError::BadRequest(m),
            LibraryError::Conflict(m) => ApiError::Conflict(m),
            LibraryError::Db(e) => ApiError::Internal(e.to_string()),
        }
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(err: sqlx::Error) -> Self {
        ApiError::Internal(err.to_string())
    }
}

impl From<cabinet_core::accounts::AccountError> for ApiError {
    fn from(err: cabinet_core::accounts::AccountError) -> Self {
        use cabinet_core::accounts::AccountError;
        match err {
            AccountError::NotFound => ApiError::NotFound,
            AccountError::Invalid(m) => ApiError::BadRequest(m),
            AccountError::BadLogin => ApiError::Unauthorized(err.to_string()),
            AccountError::Db(e) => ApiError::Internal(e.to_string()),
        }
    }
}
