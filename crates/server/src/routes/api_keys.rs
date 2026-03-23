use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use chrono::{DateTime, Utc};
use rand::Rng;
use serde::Deserialize;

use api_types::{ApiKeyId, ApiKeySummary, CreateApiKeyRequest, CreateApiKeyResponse, PaginatedResponse};

use crate::error::AppError;
use crate::middleware::auth::hash_api_key;
use crate::state::AppState;

/// Generate a cryptographically random API key with `rh_` prefix.
/// Uses 32 random bytes encoded as hex (64 chars).
fn generate_api_key() -> String {
    let mut rng = rand::thread_rng();
    let bytes: [u8; 32] = rng.gen();
    let encoded: String = bytes.iter().map(|b| format!("{:02x}", b)).collect();
    format!("rh_{encoded}")
}

/// POST /api-keys/bootstrap — create the first API key (no auth required).
pub async fn bootstrap(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    // Check if ANY keys exist (env + DB)
    if !state.config.api_keys.is_empty() {
        return Err(AppError::forbidden("Keys already exist"));
    }

    let db_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM api_keys")
            .fetch_one(&state.pool)
            .await?;

    if db_count > 0 {
        return Err(AppError::forbidden("Keys already exist"));
    }

    // Generate and store key
    let raw_key = generate_api_key();
    let key_hash = hash_api_key(&raw_key);
    let label = "bootstrap";

    let row = sqlx::query_as::<_, (sqlx::types::Uuid, DateTime<Utc>)>(
        "INSERT INTO api_keys (key_hash, label) VALUES ($1, $2) RETURNING id, created_at",
    )
    .bind(&key_hash)
    .bind(label)
    .fetch_one(&state.pool)
    .await?;

    let resp = CreateApiKeyResponse {
        id: ApiKeyId::from_string(row.0.to_string()),
        key: raw_key,
        label: label.to_string(),
        created_at: row.1,
    };

    Ok((StatusCode::CREATED, Json(resp)))
}

/// POST /api-keys — create a new API key (authenticated).
pub async fn create_api_key(
    State(state): State<AppState>,
    Json(body): Json<CreateApiKeyRequest>,
) -> Result<impl IntoResponse, AppError> {
    let raw_key = generate_api_key();
    let key_hash = hash_api_key(&raw_key);

    let row = sqlx::query_as::<_, (sqlx::types::Uuid, DateTime<Utc>)>(
        "INSERT INTO api_keys (key_hash, label) VALUES ($1, $2) RETURNING id, created_at",
    )
    .bind(&key_hash)
    .bind(&body.label)
    .fetch_one(&state.pool)
    .await?;

    let resp = CreateApiKeyResponse {
        id: ApiKeyId::from_string(row.0.to_string()),
        key: raw_key,
        label: body.label,
        created_at: row.1,
    };

    Ok((StatusCode::CREATED, Json(resp)))
}

#[derive(Debug, Deserialize)]
pub struct ListApiKeysQuery {
    pub cursor: Option<String>,
    pub limit: Option<i64>,
}

/// GET /api-keys — list API keys (no secrets).
pub async fn list_api_keys(
    State(state): State<AppState>,
    Query(query): Query<ListApiKeysQuery>,
) -> Result<impl IntoResponse, AppError> {
    let limit = query.limit.unwrap_or(50).min(100);

    let rows = if let Some(ref cursor) = query.cursor {
        // cursor is the created_at of the last item
        let cursor_time: DateTime<Utc> = cursor
            .parse()
            .map_err(|_| AppError::bad_request("Invalid cursor"))?;

        sqlx::query_as::<_, (sqlx::types::Uuid, Option<String>, DateTime<Utc>)>(
            "SELECT id, label, created_at FROM api_keys WHERE created_at < $1 ORDER BY created_at DESC LIMIT $2",
        )
        .bind(cursor_time)
        .bind(limit + 1)
        .fetch_all(&state.pool)
        .await?
    } else {
        sqlx::query_as::<_, (sqlx::types::Uuid, Option<String>, DateTime<Utc>)>(
            "SELECT id, label, created_at FROM api_keys ORDER BY created_at DESC LIMIT $1",
        )
        .bind(limit + 1)
        .fetch_all(&state.pool)
        .await?
    };

    let has_more = rows.len() as i64 > limit;
    let items: Vec<ApiKeySummary> = rows
        .into_iter()
        .take(limit as usize)
        .map(|(id, label, created_at)| ApiKeySummary {
            id: ApiKeyId::from_string(id.to_string()),
            label: label.unwrap_or_default(),
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

/// DELETE /api-keys/:id — revoke an API key.
pub async fn delete_api_key(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let id_uuid: sqlx::types::Uuid = id
        .parse()
        .map_err(|_| AppError::bad_request("Invalid API key ID"))?;

    let result = sqlx::query("DELETE FROM api_keys WHERE id = $1")
        .bind(id_uuid)
        .execute(&state.pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::not_found("API key not found"));
    }

    Ok(StatusCode::NO_CONTENT)
}
