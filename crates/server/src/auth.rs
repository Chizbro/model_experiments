//! Bearer / `X-API-Key` validation for protected routes.

use crate::key_material::hash_api_key_secret;
use crate::AppState;
use api_types::{StandardErrorBody, StandardErrorResponse};
use axum::{
    extract::{Request, State},
    http::{header::AUTHORIZATION, HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};

/// Reject requests without a valid API key (env or non-revoked DB row).
pub async fn require_api_key(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, AuthError> {
    let headers = request.headers();
    let Some(presented) = extract_presented_key(headers) else {
        return Err(AuthError::new(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "Missing API key; send Authorization: Bearer <key> or X-API-Key",
        ));
    };

    if !key_is_valid(&state, presented).await {
        return Err(AuthError::new(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "Invalid or revoked API key",
        ));
    }

    Ok(next.run(request).await)
}

fn extract_presented_key(headers: &HeaderMap) -> Option<&str> {
    if let Some(v) = headers.get(AUTHORIZATION) {
        let s = v.to_str().ok()?;
        let prefix = "Bearer ";
        if s.len() > prefix.len() && s[..prefix.len()].eq_ignore_ascii_case(prefix) {
            let token = s[prefix.len()..].trim();
            if !token.is_empty() {
                return Some(token);
            }
        }
    }
    if let Some(v) = headers.get("x-api-key") {
        let s = v.to_str().ok()?.trim();
        if !s.is_empty() {
            return Some(s);
        }
    }
    None
}

async fn key_is_valid(state: &AppState, presented: &str) -> bool {
    let hash = hash_api_key_secret(presented);
    if state.config.api_key_hashes_env.contains(&hash) {
        return true;
    }
    let Some(pool) = &state.db else {
        return false;
    };

    sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM api_keys
            WHERE key_hash = $1 AND revoked_at IS NULL
        )
        "#,
    )
    .bind(&hash)
    .fetch_one(pool)
    .await
    .unwrap_or(false)
}

pub struct AuthError {
    status: StatusCode,
    body: StandardErrorResponse,
}

impl AuthError {
    pub fn new(status: StatusCode, code: &str, message: &str) -> Self {
        Self {
            status,
            body: StandardErrorResponse {
                error: StandardErrorBody {
                    code: code.to_string(),
                    message: message.to_string(),
                    details: None,
                },
            },
        }
    }
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        (self.status, Json(self.body)).into_response()
    }
}
