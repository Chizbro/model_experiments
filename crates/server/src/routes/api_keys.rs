use crate::auth::hash_api_key;
use crate::error::AppError;
use crate::state::AppState;
use api_types::{
    ApiKeyListItem, CreateApiKeyRequest, CreateApiKeyResponse, PaginatedResponse, PaginationParams,
};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use chrono::{DateTime, Utc};
use rand::Rng;
use tracing::info;
use uuid::Uuid;

/// Generate a cryptographically random API key (32 bytes, hex-encoded = 64 chars).
fn generate_api_key() -> String {
    let mut rng = rand::thread_rng();
    let mut bytes = [0u8; 32];
    rng.fill(&mut bytes);
    hex::encode(bytes)
}

/// POST /api-keys/bootstrap — Create first key only when no keys exist.
/// No auth required. Returns 403 if any key exists (env or DB).
pub async fn bootstrap(
    State(state): State<AppState>,
    Json(req): Json<CreateApiKeyRequest>,
) -> Result<(StatusCode, Json<CreateApiKeyResponse>), AppError> {
    // Count env keys
    let env_key_count = state.config.api_keys.len();

    // Count DB keys
    let db_key_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM api_keys")
        .fetch_one(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let total_keys = env_key_count as i64 + db_key_count;

    if total_keys > 0 {
        return Err(AppError::Forbidden(
            "cannot bootstrap: keys already exist".to_string(),
        ));
    }

    // Generate and store new key
    let key_id = Uuid::new_v4().to_string();
    let plain_key = generate_api_key();
    let key_hash = hash_api_key(&plain_key);
    let now = Utc::now();

    sqlx::query("INSERT INTO api_keys (id, key_hash, label, created_at) VALUES ($1, $2, $3, $4)")
        .bind(&key_id)
        .bind(&key_hash)
        .bind(&req.label)
        .bind(now)
        .execute(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    info!(key_id = %key_id, "API key bootstrapped (first key)");

    Ok((
        StatusCode::CREATED,
        Json(CreateApiKeyResponse {
            id: key_id,
            key: plain_key,
            label: req.label,
            created_at: now,
        }),
    ))
}

/// POST /api-keys — Create a new API key (authenticated).
pub async fn create_api_key(
    State(state): State<AppState>,
    Json(req): Json<CreateApiKeyRequest>,
) -> Result<(StatusCode, Json<CreateApiKeyResponse>), AppError> {
    let key_id = Uuid::new_v4().to_string();
    let plain_key = generate_api_key();
    let key_hash = hash_api_key(&plain_key);
    let now = Utc::now();

    sqlx::query("INSERT INTO api_keys (id, key_hash, label, created_at) VALUES ($1, $2, $3, $4)")
        .bind(&key_id)
        .bind(&key_hash)
        .bind(&req.label)
        .bind(now)
        .execute(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    info!(key_id = %key_id, label = ?req.label, "API key created");

    Ok((
        StatusCode::CREATED,
        Json(CreateApiKeyResponse {
            id: key_id,
            key: plain_key,
            label: req.label,
            created_at: now,
        }),
    ))
}

/// GET /api-keys — List keys (id, label, created_at). No secrets.
pub async fn list_api_keys(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<PaginatedResponse<ApiKeyListItem>>, AppError> {
    let limit = params.limit.unwrap_or(20).min(100) as i64;
    let fetch_limit = limit + 1;

    let rows: Vec<(String, Option<String>, DateTime<Utc>)> = if let Some(ref cursor) = params.cursor
    {
        let cursor_time: DateTime<Utc> = cursor
            .parse()
            .map_err(|_| AppError::InvalidRequest("invalid cursor".to_string()))?;

        sqlx::query_as(
            "SELECT id, label, created_at FROM api_keys
             WHERE created_at < $1
             ORDER BY created_at DESC
             LIMIT $2",
        )
        .bind(cursor_time)
        .bind(fetch_limit)
        .fetch_all(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?
    } else {
        sqlx::query_as(
            "SELECT id, label, created_at FROM api_keys
             ORDER BY created_at DESC
             LIMIT $1",
        )
        .bind(fetch_limit)
        .fetch_all(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?
    };

    let has_more = rows.len() as i64 > limit;
    let items: Vec<ApiKeyListItem> = rows
        .into_iter()
        .take(limit as usize)
        .map(|(id, label, created_at)| ApiKeyListItem {
            id,
            label,
            created_at,
        })
        .collect();

    let next_cursor = if has_more {
        items.last().map(|item| item.created_at.to_rfc3339())
    } else {
        None
    };

    Ok(Json(PaginatedResponse { items, next_cursor }))
}

/// DELETE /api-keys/:id — Revoke key. 204. Key stops working immediately.
pub async fn revoke_api_key(
    State(state): State<AppState>,
    Path(key_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let result = sqlx::query("DELETE FROM api_keys WHERE id = $1")
        .bind(&key_id)
        .execute(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!(
            "api key '{}' not found",
            key_id
        )));
    }

    info!(key_id = %key_id, "API key revoked");

    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_api_key_length() {
        let key = generate_api_key();
        assert_eq!(key.len(), 64); // 32 bytes hex-encoded
    }

    #[test]
    fn test_generate_api_key_unique() {
        let key1 = generate_api_key();
        let key2 = generate_api_key();
        assert_ne!(key1, key2);
    }

    #[test]
    fn test_hash_api_key_deterministic() {
        let key = "test-api-key-12345";
        let hash1 = hash_api_key(key);
        let hash2 = hash_api_key(key);
        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 64);
    }

    #[test]
    fn test_hash_api_key_different_inputs() {
        let hash1 = hash_api_key("key-a");
        let hash2 = hash_api_key("key-b");
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_bootstrap_key_response_serialization() {
        let resp = CreateApiKeyResponse {
            id: "id-1".to_string(),
            key: "plain-key".to_string(),
            label: Some("test".to_string()),
            created_at: Utc::now(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"key\":\"plain-key\""));
        assert!(json.contains("\"label\":\"test\""));
    }
}
