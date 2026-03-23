use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use sha2::{Digest, Sha256};

use crate::state::AppState;

/// Hash a raw API key with SHA-256 and return the hex string.
pub fn hash_api_key(key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Extract API key from request headers or query parameters.
/// Checks `Authorization: Bearer <key>`, `X-API-Key: <key>`, and `?api_key=<key>`.
/// Query parameter fallback is needed for SSE (EventSource doesn't support custom headers).
fn extract_api_key(req: &Request) -> Option<String> {
    // Try Authorization: Bearer <key>
    if let Some(auth) = req.headers().get("authorization") {
        if let Ok(val) = auth.to_str() {
            if let Some(key) = val.strip_prefix("Bearer ") {
                let key = key.trim();
                if !key.is_empty() {
                    return Some(key.to_string());
                }
            }
        }
    }

    // Try X-API-Key header
    if let Some(key_header) = req.headers().get("x-api-key") {
        if let Ok(val) = key_header.to_str() {
            let val = val.trim();
            if !val.is_empty() {
                return Some(val.to_string());
            }
        }
    }

    // Try ?api_key= query parameter (for SSE/EventSource which can't set headers)
    if let Some(query) = req.uri().query() {
        for pair in query.split('&') {
            if let Some(val) = pair.strip_prefix("api_key=") {
                let val = val.trim();
                if !val.is_empty() {
                    return Some(val.to_string());
                }
            }
        }
    }

    None
}

/// Auth middleware that validates API keys against env-based keys and DB-issued keys.
pub async fn require_auth(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, (StatusCode, axum::Json<serde_json::Value>)> {
    let key = extract_api_key(&req).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            axum::Json(serde_json::json!({
                "error": {
                    "code": "unauthorized",
                    "message": "Missing API key"
                }
            })),
        )
    })?;

    // Check env-based keys first (plain text comparison)
    if state.config.api_keys.iter().any(|k| k == &key) {
        return Ok(next.run(req).await);
    }

    // Check DB-issued keys (hash comparison)
    let key_hash = hash_api_key(&key);
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM api_keys WHERE key_hash = $1)",
    )
    .bind(&key_hash)
    .fetch_one(&state.pool)
    .await
    .unwrap_or(false);

    if exists {
        return Ok(next.run(req).await);
    }

    Err((
        StatusCode::UNAUTHORIZED,
        axum::Json(serde_json::json!({
            "error": {
                "code": "unauthorized",
                "message": "Invalid API key"
            }
        })),
    ))
}
