//! Worker registration, heartbeat, list/get/delete ([`docs/API_OVERVIEW.md`](../../docs/API_OVERVIEW.md) §5, §9).

use crate::auth::AuthError;
use crate::inbox::validate_inbox_agent_id;
use crate::AppState;
use api_types::{
    PaginatedWorkerSummaries, PostWorkerInboxListenerRequest, PostWorkerInboxListenerResponse,
    RegisterWorkerRequest, RegisterWorkerResponse, WorkerHeartbeatRequest, WorkerHeartbeatResponse,
    WorkerSummary,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use base64::prelude::{Engine as _, BASE64_URL_SAFE_NO_PAD};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::Deserialize;
use sqlx::types::Json as PgJson;
use sqlx::PgPool;

const DEFAULT_PAGE_LIMIT: i64 = 20;
const MAX_PAGE_LIMIT: i64 = 100;

type WorkerListRow = (
    String,
    Option<String>,
    serde_json::Value,
    Option<DateTime<Utc>>,
    DateTime<Utc>,
);

type WorkerDetailRow = (
    String,
    Option<String>,
    serde_json::Value,
    serde_json::Value,
    Option<DateTime<Utc>>,
);

#[derive(Debug, Deserialize)]
pub struct ListWorkersQuery {
    limit: Option<i64>,
    cursor: Option<String>,
}

fn control_plane_version() -> Result<semver::Version, AuthError> {
    semver::Version::parse(api_types::CRATE_VERSION).map_err(|_| {
        AuthError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "control plane version constant is not valid semver",
        )
    })
}

fn assert_worker_version_compatible(
    client_version: &str,
    server: &semver::Version,
) -> Result<(), AuthError> {
    let w = semver::Version::parse(client_version.trim()).map_err(|_| {
        AuthError::new(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "client_version must be a valid semver string (e.g. 0.1.0)",
        )
    })?;
    if w.major != server.major || w.minor != server.minor {
        return Err(AuthError::new(
            StatusCode::BAD_REQUEST,
            "worker_version_incompatible",
            &format!(
                "Worker client_version {} is incompatible with control plane {}. Required: same major.minor as the server release ({}.{})",
                client_version,
                api_types::CRATE_VERSION,
                server.major,
                server.minor
            ),
        ));
    }
    Ok(())
}

fn heartbeat_stale(last_seen: Option<DateTime<Utc>>, cutoff: DateTime<Utc>) -> bool {
    match last_seen {
        None => true,
        Some(t) => t < cutoff,
    }
}

fn stale_cutoff(state: &AppState) -> DateTime<Utc> {
    let secs = state
        .config
        .worker_stale_threshold
        .as_secs()
        .min(i64::MAX as u64) as i64;
    Utc::now() - chrono::Duration::seconds(secs)
}

fn worker_status(last_seen: Option<DateTime<Utc>>, cutoff: DateTime<Utc>) -> &'static str {
    match last_seen {
        None => "stale",
        Some(t) if t < cutoff => "stale",
        _ => "active",
    }
}

fn row_to_summary_list(
    id: String,
    host: Option<String>,
    labels: serde_json::Value,
    last_seen: Option<DateTime<Utc>>,
    cutoff: DateTime<Utc>,
) -> WorkerSummary {
    WorkerSummary {
        worker_id: id,
        host,
        labels,
        status: worker_status(last_seen, cutoff).to_string(),
        last_seen_at: last_seen.map(|t| t.to_rfc3339_opts(SecondsFormat::Millis, true)),
        capabilities: None,
    }
}

fn row_to_summary_detail(
    id: String,
    host: Option<String>,
    labels: serde_json::Value,
    caps: serde_json::Value,
    last_seen: Option<DateTime<Utc>>,
    cutoff: DateTime<Utc>,
) -> WorkerSummary {
    let mut s = row_to_summary_list(id, host, labels, last_seen, cutoff);
    s.capabilities = Some(json_array_strings(&caps));
    s
}

fn json_array_strings(v: &serde_json::Value) -> Vec<String> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(std::string::ToString::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// `POST /workers/register`
pub async fn register_worker(
    State(state): State<AppState>,
    Json(body): Json<RegisterWorkerRequest>,
) -> Result<(StatusCode, Json<RegisterWorkerResponse>), AuthError> {
    let Some(pool) = &state.db else {
        return Err(AuthError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "not_ready",
            "Database is not configured; cannot register workers",
        ));
    };

    let id = body.id.trim().to_string();
    if id.is_empty() {
        return Err(AuthError::new(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "worker id must not be empty",
        ));
    }

    let labels = if body.labels.is_null() {
        serde_json::json!({})
    } else if body.labels.is_object() {
        body.labels
    } else {
        return Err(AuthError::new(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "labels must be a JSON object",
        ));
    };

    let server_ver = control_plane_version()?;
    match body
        .client_version
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(v) => assert_worker_version_compatible(v, &server_ver)?,
        None => {
            eprintln!(
                "remote-harness server: POST /workers/register for id={id:?} without client_version; accepting (transitional). New workers should send client_version."
            );
        }
    }

    let client_version_store = body
        .client_version
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let caps = PgJson(body.capabilities.clone());

    let res = sqlx::query(
        r#"
        INSERT INTO workers (id, host, labels, capabilities, client_version, last_seen_at)
        VALUES ($1, $2, $3, $4, $5, now())
        "#,
    )
    .bind(&id)
    .bind(
        body.host
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty()),
    )
    .bind(PgJson(labels))
    .bind(caps)
    .bind(client_version_store.as_deref())
    .execute(pool)
    .await;

    match res {
        Ok(_) => Ok((
            StatusCode::CREATED,
            Json(RegisterWorkerResponse { worker_id: id }),
        )),
        Err(e) => {
            if let Some(db) = e.as_database_error() {
                if db.is_unique_violation() {
                    return Err(AuthError::new(
                        StatusCode::CONFLICT,
                        "conflict",
                        "A worker with this id is already registered",
                    ));
                }
            }
            Err(AuthError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                &format!("database error: {e}"),
            ))
        }
    }
}

/// `POST /workers/:id/heartbeat`
pub async fn heartbeat_worker(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(_body): Json<WorkerHeartbeatRequest>,
) -> Result<Json<WorkerHeartbeatResponse>, AuthError> {
    let Some(pool) = &state.db else {
        return Err(AuthError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "not_ready",
            "Database is not configured; cannot heartbeat workers",
        ));
    };

    let id = id.trim().to_string();
    if id.is_empty() {
        return Err(AuthError::new(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "worker id must not be empty",
        ));
    }

    let r = sqlx::query(
        r#"
        UPDATE workers SET last_seen_at = now() WHERE id = $1
        "#,
    )
    .bind(&id)
    .execute(pool)
    .await
    .map_err(|e| {
        AuthError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            &format!("database error: {e}"),
        )
    })?;

    if r.rows_affected() == 0 {
        return Err(AuthError::new(
            StatusCode::NOT_FOUND,
            "not_found",
            "Worker not found",
        ));
    }

    Ok(Json(WorkerHeartbeatResponse { ok: true }))
}

/// `GET /workers`
pub async fn list_workers(
    State(state): State<AppState>,
    Query(q): Query<ListWorkersQuery>,
) -> Result<Json<PaginatedWorkerSummaries>, AuthError> {
    let Some(pool) = &state.db else {
        return Err(AuthError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "not_ready",
            "Database is not configured; cannot list workers",
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
    let cutoff = stale_cutoff(&state);

    let rows: Vec<WorkerListRow> = if let Some((c_at, c_id)) = cursor {
        sqlx::query_as(
            r#"
            SELECT id, host, labels::jsonb, last_seen_at, created_at
            FROM workers
            WHERE (created_at, id) < ($1, $2)
            ORDER BY created_at DESC, id DESC
            LIMIT $3
            "#,
        )
        .bind(c_at)
        .bind(&c_id)
        .bind(fetch)
        .fetch_all(pool)
        .await
    } else {
        sqlx::query_as(
            r#"
            SELECT id, host, labels::jsonb, last_seen_at, created_at
            FROM workers
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
            .map(|(id, _, _, _, created_at)| encode_cursor(*created_at, id))
    } else {
        None
    };

    let items: Vec<WorkerSummary> = page_rows
        .into_iter()
        .map(|(id, host, labels, last_seen, _created)| {
            row_to_summary_list(id, host, labels, last_seen, cutoff)
        })
        .collect();

    Ok(Json(PaginatedWorkerSummaries { items, next_cursor }))
}

/// `GET /workers/:id`
pub async fn get_worker(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<WorkerSummary>, AuthError> {
    let Some(pool) = &state.db else {
        return Err(AuthError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "not_ready",
            "Database is not configured; cannot get worker",
        ));
    };

    let id = id.trim().to_string();
    let cutoff = stale_cutoff(&state);

    let row: Option<WorkerDetailRow> = sqlx::query_as(
        r#"
        SELECT id, host, labels::jsonb, capabilities::jsonb, last_seen_at
        FROM workers
        WHERE id = $1
        "#,
    )
    .bind(&id)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        AuthError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            &format!("database error: {e}"),
        )
    })?;

    let Some((id, host, labels, caps, last_seen)) = row else {
        return Err(AuthError::new(
            StatusCode::NOT_FOUND,
            "not_found",
            "Worker not found",
        ));
    };

    Ok(Json(row_to_summary_detail(
        id, host, labels, caps, last_seen, cutoff,
    )))
}

/// `DELETE /workers/:id` — remove worker and reclaim or fail assigned jobs ([`docs/ARCHITECTURE.md`](../../docs/ARCHITECTURE.md) §3b).
pub async fn delete_worker(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, AuthError> {
    let Some(pool) = &state.db else {
        return Err(AuthError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "not_ready",
            "Database is not configured; cannot delete workers",
        ));
    };

    let id = id.trim().to_string();
    if id.is_empty() {
        return Err(AuthError::new(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "worker id must not be empty",
        ));
    }

    delete_worker_tx(pool, &state, &id).await
}

async fn delete_worker_tx(
    pool: &PgPool,
    state: &AppState,
    id: &str,
) -> Result<StatusCode, AuthError> {
    let max = state.config.max_job_reclaims;

    let mut tx = pool.begin().await.map_err(|e| {
        AuthError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            &format!("database error: {e}"),
        )
    })?;

    let exists: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS(SELECT 1 FROM workers WHERE id = $1)
        "#,
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        AuthError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            &format!("database error: {e}"),
        )
    })?;

    if !exists {
        return Err(AuthError::new(
            StatusCode::NOT_FOUND,
            "not_found",
            "Worker not found",
        ));
    }

    sqlx::query(
        r#"
        UPDATE jobs
        SET worker_id = NULL,
            status = 'pending',
            reclaim_count = reclaim_count + 1,
            assigned_at = NULL,
            updated_at = now()
        WHERE status = 'assigned'
          AND worker_id = $1
          AND reclaim_count < $2
        "#,
    )
    .bind(id)
    .bind(max)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        AuthError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            &format!("database error: {e}"),
        )
    })?;

    sqlx::query(
        r#"
        UPDATE jobs
        SET worker_id = NULL,
            status = 'failed',
            updated_at = now(),
            error_message = CASE
                WHEN error_message IS NULL OR error_message = '' THEN '[MAX_WORKER_LOSS_RETRIES]'
                ELSE error_message || E'\n' || '[MAX_WORKER_LOSS_RETRIES]'
            END
        WHERE status = 'assigned'
          AND worker_id = $1
          AND reclaim_count >= $2
        "#,
    )
    .bind(id)
    .bind(max)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        AuthError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            &format!("database error: {e}"),
        )
    })?;

    let del = sqlx::query(
        r#"
        DELETE FROM workers WHERE id = $1
        "#,
    )
    .bind(id)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        AuthError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            &format!("database error: {e}"),
        )
    })?;

    if del.rows_affected() == 0 {
        return Err(AuthError::new(
            StatusCode::NOT_FOUND,
            "not_found",
            "Worker not found",
        ));
    }

    tx.commit().await.map_err(|e| {
        AuthError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            &format!("database error: {e}"),
        )
    })?;

    Ok(StatusCode::NO_CONTENT)
}

/// `POST /workers/:id/inbox-listener` — register this worker as the inbox consumer for `agent_id`
/// ([`docs/API_OVERVIEW.md`](../../docs/API_OVERVIEW.md) §8, PHASE2 listener claim).
pub async fn post_worker_inbox_listener(
    State(state): State<AppState>,
    Path(worker_path_id): Path<String>,
    Json(body): Json<PostWorkerInboxListenerRequest>,
) -> Result<Json<PostWorkerInboxListenerResponse>, AuthError> {
    let Some(pool) = &state.db else {
        return Err(AuthError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "not_ready",
            "Database is not configured",
        ));
    };

    let worker_id = worker_path_id.trim();
    if worker_id.is_empty() {
        return Err(AuthError::new(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "worker id must not be empty",
        ));
    }

    let agent_id = body.agent_id.trim();
    validate_inbox_agent_id(agent_id)?;

    let mut tx = pool.begin().await.map_err(|e| {
        AuthError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            &format!("database error: {e}"),
        )
    })?;

    let worker_row: Option<Option<DateTime<Utc>>> =
        sqlx::query_scalar(r#"SELECT last_seen_at FROM workers WHERE id = $1"#)
            .bind(worker_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| {
                AuthError::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "server_error",
                    &format!("database error: {e}"),
                )
            })?;

    let Some(last_seen) = worker_row else {
        return Err(AuthError::new(
            StatusCode::NOT_FOUND,
            "not_found",
            "Worker not found",
        ));
    };

    let stale_at = stale_cutoff(&state);
    if heartbeat_stale(last_seen, stale_at) {
        return Err(AuthError::new(
            StatusCode::CONFLICT,
            "worker_stale",
            "Worker heartbeat is stale; send a heartbeat before claiming an inbox listener",
        ));
    }

    let agent_exists: bool = sqlx::query_scalar(
        r#"SELECT EXISTS(SELECT 1 FROM agents WHERE id = $1)"#,
    )
    .bind(agent_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        AuthError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            &format!("database error: {e}"),
        )
    })?;

    if !agent_exists {
        return Err(AuthError::new(
            StatusCode::NOT_FOUND,
            "not_found",
            "Agent id is unknown; create an inbox session with this agent_id first",
        ));
    }

    let current: Option<String> = sqlx::query_scalar(
        r#"
        SELECT worker_id FROM inbox_listeners WHERE agent_id = $1 FOR UPDATE
        "#,
    )
    .bind(agent_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| {
        AuthError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            &format!("database error: {e}"),
        )
    })?;

    if let Some(ref other) = current {
        if other != worker_id {
            let other_seen: Option<Option<DateTime<Utc>>> =
                sqlx::query_scalar(r#"SELECT last_seen_at FROM workers WHERE id = $1"#)
                    .bind(other)
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(|e| {
                        AuthError::new(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            "server_error",
                            &format!("database error: {e}"),
                        )
                    })?;
            let steal = match other_seen {
                None | Some(None) => true,
                Some(Some(t)) => heartbeat_stale(Some(t), stale_at),
            };
            if !steal {
                return Err(AuthError::new(
                    StatusCode::CONFLICT,
                    "inbox_listener_taken",
                    "Another worker already holds the inbox listener for this agent_id",
                ));
            }
        }
    }

    sqlx::query(
        r#"
        INSERT INTO inbox_listeners (agent_id, worker_id)
        VALUES ($1, $2)
        ON CONFLICT (agent_id) DO UPDATE
        SET worker_id = EXCLUDED.worker_id, claimed_at = now()
        "#,
    )
    .bind(agent_id)
    .bind(worker_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        AuthError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            &format!("database error: {e}"),
        )
    })?;

    tx.commit().await.map_err(|e| {
        AuthError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            &format!("database error: {e}"),
        )
    })?;

    Ok(Json(PostWorkerInboxListenerResponse { ok: true }))
}

fn encode_cursor(created_at: DateTime<Utc>, id: &str) -> String {
    let raw = format!(
        "{}|{}",
        created_at.to_rfc3339_opts(SecondsFormat::Millis, true),
        id
    );
    BASE64_URL_SAFE_NO_PAD.encode(raw.as_bytes())
}

fn parse_cursor(s: &str) -> Result<(DateTime<Utc>, String), String> {
    let bytes = BASE64_URL_SAFE_NO_PAD
        .decode(s.as_bytes())
        .map_err(|e| e.to_string())?;
    let raw = String::from_utf8(bytes).map_err(|e| e.to_string())?;
    let (t, id) = raw
        .split_once('|')
        .ok_or_else(|| "expected '<rfc3339>|<worker_id>'".to_string())?;
    let created_at = DateTime::parse_from_rfc3339(t)
        .map_err(|e| e.to_string())?
        .with_timezone(&Utc);
    if id.is_empty() {
        return Err("empty worker id in cursor".to_string());
    }
    Ok((created_at, id.to_string()))
}
