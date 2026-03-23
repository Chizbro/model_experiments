use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use chrono::{DateTime, Utc};
use serde::Deserialize;

use api_types::{
    HeartbeatRequest, HeartbeatResponse, PaginatedResponse, RegisterWorkerRequest,
    RegisterWorkerResponse, WorkerConnectionStatus, WorkerDetail, WorkerId, WorkerSummary,
};

use crate::error::AppError;
use crate::state::AppState;

const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

fn check_version_compatible(client_version: &str) -> bool {
    let server_parts: Vec<&str> = SERVER_VERSION.split('.').collect();
    let client_parts: Vec<&str> = client_version.split('.').collect();
    if server_parts.len() < 2 || client_parts.len() < 2 {
        return false;
    }
    server_parts[0] == client_parts[0] && server_parts[1] == client_parts[1]
}

fn compute_connection_status(
    last_seen_at: DateTime<Utc>,
    worker_stale_seconds: u64,
) -> WorkerConnectionStatus {
    let elapsed = Utc::now()
        .signed_duration_since(last_seen_at)
        .num_seconds();
    if elapsed <= worker_stale_seconds as i64 {
        WorkerConnectionStatus::Active
    } else {
        WorkerConnectionStatus::Stale
    }
}

/// POST /workers/register
pub async fn register_worker(
    State(state): State<AppState>,
    Json(body): Json<RegisterWorkerRequest>,
) -> Result<impl IntoResponse, AppError> {
    // Version check
    if let Some(ref version) = body.client_version {
        if !check_version_compatible(version) {
            return Err(AppError {
                status: StatusCode::BAD_REQUEST,
                code: "worker_version_incompatible".into(),
                message: format!(
                    "Client version {} is incompatible with server version {}. Must match major.minor.",
                    version, SERVER_VERSION
                ),
                details: None,
            });
        }
    } else {
        tracing::warn!(worker_id = %body.id, "Worker registered without client_version");
    }

    let labels_json = serde_json::to_value(body.labels.unwrap_or_default())
        .unwrap_or(serde_json::Value::Array(vec![]));
    let capabilities_json = serde_json::to_value(body.capabilities.unwrap_or_default())
        .unwrap_or(serde_json::Value::Array(vec![]));

    // Upsert: insert or update on conflict
    sqlx::query(
        r#"
        INSERT INTO workers (id, host, labels, capabilities, client_version, last_seen_at)
        VALUES ($1, $2, $3, $4, $5, now())
        ON CONFLICT (id) DO UPDATE SET
            host = EXCLUDED.host,
            labels = EXCLUDED.labels,
            capabilities = EXCLUDED.capabilities,
            client_version = EXCLUDED.client_version,
            last_seen_at = now()
        "#,
    )
    .bind(body.id.as_str())
    .bind(&body.host)
    .bind(&labels_json)
    .bind(&capabilities_json)
    .bind(&body.client_version)
    .execute(&state.pool)
    .await?;

    let resp = RegisterWorkerResponse {
        worker_id: body.id,
    };

    Ok((StatusCode::CREATED, Json(resp)))
}

/// POST /workers/:id/heartbeat
pub async fn heartbeat(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(_body): Json<HeartbeatRequest>,
) -> Result<impl IntoResponse, AppError> {
    let result = sqlx::query("UPDATE workers SET last_seen_at = now() WHERE id = $1")
        .bind(&id)
        .execute(&state.pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::not_found("Worker not found"));
    }

    Ok(Json(HeartbeatResponse { ok: true }))
}

#[derive(Debug, Deserialize)]
pub struct ListWorkersQuery {
    pub cursor: Option<String>,
    pub limit: Option<i64>,
}

/// GET /workers
pub async fn list_workers(
    State(state): State<AppState>,
    Query(query): Query<ListWorkersQuery>,
) -> Result<impl IntoResponse, AppError> {
    let limit = query.limit.unwrap_or(50).min(100);

    let rows = if let Some(ref cursor) = query.cursor {
        let cursor_time: DateTime<Utc> = cursor
            .parse()
            .map_err(|_| AppError::bad_request("Invalid cursor"))?;

        sqlx::query_as::<_, (String, String, serde_json::Value, DateTime<Utc>)>(
            "SELECT id, host, labels, last_seen_at FROM workers WHERE created_at < $1 ORDER BY created_at DESC LIMIT $2",
        )
        .bind(cursor_time)
        .bind(limit + 1)
        .fetch_all(&state.pool)
        .await?
    } else {
        sqlx::query_as::<_, (String, String, serde_json::Value, DateTime<Utc>)>(
            "SELECT id, host, labels, last_seen_at FROM workers ORDER BY created_at DESC LIMIT $1",
        )
        .bind(limit + 1)
        .fetch_all(&state.pool)
        .await?
    };

    let has_more = rows.len() as i64 > limit;
    let stale_seconds = state.config.worker_stale_seconds;

    let items: Vec<WorkerSummary> = rows
        .into_iter()
        .take(limit as usize)
        .map(|(id, host, labels, last_seen_at)| {
            let labels_vec: Option<Vec<String>> = serde_json::from_value(labels).ok();
            WorkerSummary {
                worker_id: WorkerId::from_string(id),
                host,
                labels: labels_vec,
                status: compute_connection_status(last_seen_at, stale_seconds),
                last_seen_at: Some(last_seen_at),
            }
        })
        .collect();

    let next_cursor = if has_more {
        items
            .last()
            .and_then(|w| w.last_seen_at.map(|t| t.to_rfc3339()))
    } else {
        None
    };

    Ok(Json(PaginatedResponse { items, next_cursor }))
}

/// GET /workers/:id
pub async fn get_worker(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let row = sqlx::query_as::<_, (String, String, serde_json::Value, serde_json::Value, Option<String>, DateTime<Utc>, DateTime<Utc>)>(
        "SELECT id, host, labels, capabilities, client_version, last_seen_at, created_at FROM workers WHERE id = $1",
    )
    .bind(&id)
    .fetch_optional(&state.pool)
    .await?;

    let (id, host, labels, capabilities, client_version, last_seen_at, created_at) =
        row.ok_or_else(|| AppError::not_found("Worker not found"))?;

    let labels_vec: Option<Vec<String>> = serde_json::from_value(labels).ok();
    let capabilities_vec: Option<Vec<String>> = serde_json::from_value(capabilities).ok();

    let detail = WorkerDetail {
        worker_id: WorkerId::from_string(id),
        host,
        labels: labels_vec,
        capabilities: capabilities_vec,
        status: compute_connection_status(last_seen_at, state.config.worker_stale_seconds),
        client_version,
        last_seen_at: Some(last_seen_at),
        created_at,
    };

    Ok(Json(detail))
}

/// DELETE /workers/:id
pub async fn delete_worker(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    // Check worker exists
    let exists = sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM workers WHERE id = $1)")
        .bind(&id)
        .fetch_one(&state.pool)
        .await?;

    if !exists {
        return Err(AppError::not_found("Worker not found"));
    }

    // Reclaim any assigned jobs (set to pending, increment reclaim_count)
    sqlx::query(
        r#"
        UPDATE jobs
        SET status = 'pending',
            worker_id = NULL,
            assigned_at = NULL,
            reclaim_count = reclaim_count + 1,
            updated_at = now()
        WHERE worker_id = $1 AND status = 'running'
        "#,
    )
    .bind(&id)
    .execute(&state.pool)
    .await?;

    // Delete the worker
    sqlx::query("DELETE FROM workers WHERE id = $1")
        .bind(&id)
        .execute(&state.pool)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_compatible() {
        assert!(check_version_compatible("0.1.0"));
        assert!(check_version_compatible("0.1.5"));
        assert!(!check_version_compatible("0.2.0"));
        assert!(!check_version_compatible("1.1.0"));
        assert!(!check_version_compatible("invalid"));
    }

    #[test]
    fn test_compute_connection_status() {
        let now = Utc::now();
        assert_eq!(
            compute_connection_status(now, 90),
            WorkerConnectionStatus::Active
        );

        let old = now - chrono::Duration::seconds(200);
        assert_eq!(
            compute_connection_status(old, 90),
            WorkerConnectionStatus::Stale
        );
    }
}
