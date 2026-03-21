//! `POST/GET/DELETE /api-keys` and bootstrap ([`docs/API_OVERVIEW.md`](../../docs/API_OVERVIEW.md) §4c).

use crate::auth::AuthError;
use crate::key_material::{generate_api_key_plaintext, hash_api_key_secret};
use crate::AppState;
use api_types::{
    ApiKeyCreatedResponse, ApiKeySummary, CreateApiKeyRequest, PaginatedApiKeySummaries,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use base64::prelude::{Engine as _, BASE64_URL_SAFE_NO_PAD};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;

const DEFAULT_PAGE_LIMIT: i64 = 20;
const MAX_PAGE_LIMIT: i64 = 100;

#[derive(Debug, Deserialize)]
pub struct ListKeysQuery {
    limit: Option<i64>,
    cursor: Option<String>,
}

/// `POST /api-keys/bootstrap` — unauthenticated; 403 if any env or DB key exists.
pub async fn bootstrap_api_key(
    State(state): State<AppState>,
    Json(body): Json<CreateApiKeyRequest>,
) -> Result<(StatusCode, Json<ApiKeyCreatedResponse>), AuthError> {
    if !state.config.api_key_hashes_env.is_empty() {
        return Err(AuthError::new(
            StatusCode::FORBIDDEN,
            "forbidden",
            "Bootstrap is disabled: API_KEY or API_KEYS is set in the server environment",
        ));
    }

    let Some(pool) = &state.db else {
        return Err(AuthError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "not_ready",
            "Database is not configured; cannot bootstrap API keys",
        ));
    };

    let active_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*)::bigint FROM api_keys WHERE revoked_at IS NULL")
            .fetch_one(pool)
            .await
            .map_err(|e| {
                AuthError::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "server_error",
                    &format!("database error: {e}"),
                )
            })?;

    if active_count > 0 {
        return Err(AuthError::new(
            StatusCode::FORBIDDEN,
            "forbidden",
            "Bootstrap is disabled: at least one API key already exists",
        ));
    }

    insert_new_key(pool, body.label)
        .await
        .map(|r| (StatusCode::CREATED, Json(r)))
}

/// `POST /api-keys` — authenticated.
pub async fn create_api_key(
    State(state): State<AppState>,
    Json(body): Json<CreateApiKeyRequest>,
) -> Result<(StatusCode, Json<ApiKeyCreatedResponse>), AuthError> {
    let Some(pool) = &state.db else {
        return Err(AuthError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "not_ready",
            "Database is not configured; cannot create API keys",
        ));
    };

    insert_new_key(pool, body.label)
        .await
        .map(|r| (StatusCode::CREATED, Json(r)))
}

async fn insert_new_key(
    pool: &PgPool,
    label: Option<String>,
) -> Result<ApiKeyCreatedResponse, AuthError> {
    let plaintext = generate_api_key_plaintext();
    let key_hash = hash_api_key_secret(&plaintext);

    let row: (Uuid, Option<String>, DateTime<Utc>) = sqlx::query_as(
        r#"
        INSERT INTO api_keys (key_hash, label)
        VALUES ($1, $2)
        RETURNING id, label, created_at
        "#,
    )
    .bind(&key_hash)
    .bind(label.as_deref())
    .fetch_one(pool)
    .await
    .map_err(|e| {
        AuthError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            &format!("database error: {e}"),
        )
    })?;

    Ok(ApiKeyCreatedResponse {
        id: row.0.to_string(),
        key: plaintext,
        label: row.1,
        created_at: row.2.to_rfc3339_opts(SecondsFormat::Millis, true),
    })
}

/// `GET /api-keys` — authenticated; cursor pagination (newest first).
pub async fn list_api_keys(
    State(state): State<AppState>,
    Query(q): Query<ListKeysQuery>,
) -> Result<Json<PaginatedApiKeySummaries>, AuthError> {
    let Some(pool) = &state.db else {
        return Err(AuthError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "not_ready",
            "Database is not configured; cannot list API keys",
        ));
    };

    let limit = q
        .limit
        .unwrap_or(DEFAULT_PAGE_LIMIT)
        .clamp(1, MAX_PAGE_LIMIT);

    let cursor = q
        .cursor
        .as_deref()
        .map(parse_cursor)
        .transpose()
        .map_err(|msg| {
            AuthError::new(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                &format!("invalid cursor: {msg}"),
            )
        })?;

    let fetch = limit + 1;
    let rows: Vec<(Uuid, Option<String>, DateTime<Utc>)> = if let Some((c_at, c_id)) = cursor {
        sqlx::query_as(
            r#"
            SELECT id, label, created_at
            FROM api_keys
            WHERE revoked_at IS NULL
              AND (created_at, id) < ($1, $2)
            ORDER BY created_at DESC, id DESC
            LIMIT $3
            "#,
        )
        .bind(c_at)
        .bind(c_id)
        .bind(fetch)
        .fetch_all(pool)
        .await
    } else {
        sqlx::query_as(
            r#"
            SELECT id, label, created_at
            FROM api_keys
            WHERE revoked_at IS NULL
            ORDER BY created_at DESC, id DESC
            LIMIT $1
            "#,
        )
        .bind(fetch)
        .fetch_all(pool)
        .await
    }
    .map_err(|e| {
        AuthError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            &format!("database error: {e}"),
        )
    })?;

    let has_more = rows.len() > limit as usize;
    let page_rows: Vec<_> = rows.into_iter().take(limit as usize).collect();

    let next_cursor = if has_more {
        page_rows
            .last()
            .map(|(id, _, created_at)| encode_cursor(*created_at, *id))
    } else {
        None
    };

    let items: Vec<ApiKeySummary> = page_rows
        .into_iter()
        .map(|(id, label, created_at)| ApiKeySummary {
            id: id.to_string(),
            label,
            created_at: created_at.to_rfc3339_opts(SecondsFormat::Millis, true),
        })
        .collect();

    Ok(Json(PaginatedApiKeySummaries { items, next_cursor }))
}

fn encode_cursor(created_at: DateTime<Utc>, id: Uuid) -> String {
    let raw = format!(
        "{}|{}",
        created_at.to_rfc3339_opts(SecondsFormat::Millis, true),
        id
    );
    BASE64_URL_SAFE_NO_PAD.encode(raw.as_bytes())
}

fn parse_cursor(s: &str) -> Result<(DateTime<Utc>, Uuid), String> {
    let bytes = BASE64_URL_SAFE_NO_PAD
        .decode(s.as_bytes())
        .map_err(|e| e.to_string())?;
    let raw = String::from_utf8(bytes).map_err(|e| e.to_string())?;
    let (t, id) = raw
        .split_once('|')
        .ok_or_else(|| "expected '<rfc3339>|<uuid>'".to_string())?;
    let created_at = DateTime::parse_from_rfc3339(t)
        .map_err(|e| e.to_string())?
        .with_timezone(&Utc);
    let id = Uuid::parse_str(id).map_err(|e| e.to_string())?;
    Ok((created_at, id))
}

/// `DELETE /api-keys/:id` — soft revoke; 404 if missing.
pub async fn delete_api_key(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AuthError> {
    let Some(pool) = &state.db else {
        return Err(AuthError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "not_ready",
            "Database is not configured; cannot revoke API keys",
        ));
    };

    let res = sqlx::query(
        r#"
        UPDATE api_keys
        SET revoked_at = now()
        WHERE id = $1 AND revoked_at IS NULL
        "#,
    )
    .bind(id)
    .execute(pool)
    .await
    .map_err(|e| {
        AuthError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            &format!("database error: {e}"),
        )
    })?;

    if res.rows_affected() == 0 {
        return Err(AuthError::new(
            StatusCode::NOT_FOUND,
            "not_found",
            "API key not found",
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}
