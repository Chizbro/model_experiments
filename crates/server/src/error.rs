use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

#[derive(Debug)]
pub struct AppError {
    pub status: StatusCode,
    pub code: String,
    pub message: String,
    pub details: Option<serde_json::Value>,
}

impl AppError {
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "bad_request".into(),
            message: message.into(),
            details: None,
        }
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            code: "unauthorized".into(),
            message: message.into(),
            details: None,
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            code: "not_found".into(),
            message: message.into(),
            details: None,
        }
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            code: "conflict".into(),
            message: message.into(),
            details: None,
        }
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            code: "forbidden".into(),
            message: message.into(),
            details: None,
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "internal_error".into(),
            message: message.into(),
            details: None,
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let mut body = json!({
            "error": {
                "code": self.code,
                "message": self.message,
            }
        });

        if let Some(details) = self.details {
            body["error"]["details"] = details;
        }

        (self.status, axum::Json(body)).into_response()
    }
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        tracing::error!(?err, "database error");
        Self::internal("Internal database error")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_constructors() {
        let err = AppError::bad_request("bad");
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
        assert_eq!(err.code, "bad_request");

        let err = AppError::unauthorized("no auth");
        assert_eq!(err.status, StatusCode::UNAUTHORIZED);

        let err = AppError::not_found("gone");
        assert_eq!(err.status, StatusCode::NOT_FOUND);

        let err = AppError::conflict("dup");
        assert_eq!(err.status, StatusCode::CONFLICT);

        let err = AppError::internal("oops");
        assert_eq!(err.status, StatusCode::INTERNAL_SERVER_ERROR);
    }
}
