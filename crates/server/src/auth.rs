use crate::error::AppError;
use crate::state::AppState;
use axum::body::Body;
use axum::extract::State;
use axum::http::Request;
use axum::middleware::Next;
use axum::response::Response;
use sha2::{Digest, Sha256};
use tracing::debug;

/// Paths that skip authentication entirely.
fn is_public_path(path: &str) -> bool {
    matches!(
        path,
        "/health" | "/ready" | "/health/idle"
    ) || path.starts_with("/auth/")
      || path == "/api-keys/bootstrap"
}

/// Extract the API key from the request headers.
fn extract_api_key(req: &Request<Body>) -> Option<String> {
    // Check Authorization: Bearer <key>
    if let Some(auth) = req.headers().get("authorization") {
        if let Ok(value) = auth.to_str() {
            if let Some(key) = value.strip_prefix("Bearer ") {
                return Some(key.to_string());
            }
        }
    }

    // Check X-API-Key header
    if let Some(api_key) = req.headers().get("x-api-key") {
        if let Ok(value) = api_key.to_str() {
            return Some(value.to_string());
        }
    }

    None
}

/// Validate a key against env-configured keys (plaintext comparison) and
/// DB-issued keys (SHA-256 hash comparison).
async fn validate_key(state: &AppState, key: &str) -> bool {
    // Check against env-configured keys (plaintext)
    if state.config.api_keys.iter().any(|k| k == key) {
        return true;
    }

    // Check against DB-issued keys (hashed)
    let key_hash = hash_api_key(key);
    let result = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM api_keys WHERE key_hash = $1"
    )
    .bind(&key_hash)
    .fetch_one(&state.db)
    .await;

    matches!(result, Ok(count) if count > 0)
}

/// Hash an API key with SHA-256 for storage/comparison.
pub fn hash_api_key(key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    hex::encode(hasher.finalize())
}

/// Auth middleware for axum.
pub async fn auth_middleware(
    State(state): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, AppError> {
    let path = req.uri().path().to_string();

    if is_public_path(&path) {
        debug!(path = %path, "Skipping auth for public path");
        return Ok(next.run(req).await);
    }

    let key = extract_api_key(&req).ok_or(AppError::Unauthorized)?;

    if !validate_key(&state, &key).await {
        return Err(AppError::Unauthorized);
    }

    Ok(next.run(req).await)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_public_path() {
        assert!(is_public_path("/health"));
        assert!(is_public_path("/ready"));
        assert!(is_public_path("/health/idle"));
        assert!(is_public_path("/auth/github"));
        assert!(is_public_path("/auth/github/callback"));
        assert!(is_public_path("/auth/gitlab"));
        assert!(is_public_path("/api-keys/bootstrap"));
        assert!(!is_public_path("/sessions"));
        assert!(!is_public_path("/workers"));
        assert!(!is_public_path("/api-keys"));
    }

    #[test]
    fn test_hash_api_key() {
        let hash = hash_api_key("test-key");
        assert_eq!(hash.len(), 64); // SHA-256 hex output is 64 chars
        // Same input produces same output
        assert_eq!(hash, hash_api_key("test-key"));
        // Different input produces different output
        assert_ne!(hash, hash_api_key("other-key"));
    }
}
