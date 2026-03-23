use api_types::{ErrorBody, ErrorDetail};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

#[derive(Debug, thiserror::Error)]
#[allow(dead_code)]
pub enum AppError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("unauthorized")]
    Unauthorized,
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("forbidden: {0}")]
    Forbidden(String),
    #[error("internal: {0}")]
    Internal(#[from] anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, message) = match &self {
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, "not_found", msg.clone()),
            AppError::InvalidRequest(msg) => (StatusCode::BAD_REQUEST, "invalid_request", msg.clone()),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized", "unauthorized".to_string()),
            AppError::Conflict(msg) => (StatusCode::CONFLICT, "conflict", msg.clone()),
            AppError::Forbidden(msg) => (StatusCode::FORBIDDEN, "forbidden", msg.clone()),
            AppError::Internal(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal",
                err.to_string(),
            ),
        };

        let body = ErrorBody {
            error: ErrorDetail {
                code: code.to_string(),
                message,
                details: None,
            },
        };

        (status, axum::Json(body)).into_response()
    }
}
