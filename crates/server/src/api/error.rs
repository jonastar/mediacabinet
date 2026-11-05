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
    /// Signed in, but not allowed, for this reason.
    ForbiddenBecause(String),
    Conflict(String),
    TooManyRequests(String),
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
            ApiError::ForbiddenBecause(message) => (StatusCode::FORBIDDEN, message),
            ApiError::Conflict(message) => (StatusCode::CONFLICT, message),
            ApiError::TooManyRequests(message) => (StatusCode::TOO_MANY_REQUESTS, message),
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
            AccountError::Disabled => ApiError::Unauthorized(err.to_string()),
            AccountError::Db(e) => ApiError::Internal(e.to_string()),
        }
    }
}

impl From<cabinet_core::pairing::PairError> for ApiError {
    fn from(err: cabinet_core::pairing::PairError) -> Self {
        use cabinet_core::pairing::PairError;
        match err {
            PairError::BadName | PairError::UnknownCode => ApiError::BadRequest(err.to_string()),
            PairError::TooManyStarts | PairError::TooManyMisses => {
                ApiError::TooManyRequests(err.to_string())
            }
        }
    }
}

impl From<cabinet_core::sign_in::SignInError> for ApiError {
    fn from(err: cabinet_core::sign_in::SignInError) -> Self {
        use cabinet_core::sign_in::SignInError;
        match err {
            SignInError::NotFound => ApiError::NotFound,
            SignInError::Invalid(m) => ApiError::BadRequest(m),
            SignInError::Refused(m) => ApiError::ForbiddenBecause(m),
            SignInError::Oidc(e) => ApiError::BadRequest(e.to_string()),
            SignInError::Account(e) => e.into(),
            SignInError::Db(e) => ApiError::Internal(e.to_string()),
        }
    }
}
