use api_types::{
    HeartbeatRequest, HeartbeatResponse, LogEntry, PaginatedResponse, PaginationParams,
    PullTaskRequest, PullTaskResponse, RegisterWorkerRequest, RegisterWorkerResponse,
    SendLogEntry, SendLogsResponse, TaskCompleteRequest, TaskCompleteResponse, TaskCredentials,
    WorkerDetail, WorkerListItem,
};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use chrono::{DateTime, Utc};
use serde_json::Value as JsonValue;
use tracing::{info, warn};
use uuid::Uuid;

use crate::engine;
use crate::error::AppError;
use crate::state::AppState;

/// Row type for worker list queries (avoids clippy type_complexity).
type WorkerListRow = (String, String, JsonValue, String, DateTime<Utc>, DateTime<Utc>);

/// Row type for worker detail queries.
type WorkerDetailRow = (
    String,
    String,
    JsonValue,
    JsonValue,
    String,
    DateTime<Utc>,
    Option<String>,
);

/// POST /workers/register — Create or update (upsert) a worker registration.
pub async fn register_worker(
    State(state): State<AppState>,
    Json(req): Json<RegisterWorkerRequest>,
) -> Result<impl IntoResponse, AppError> {
    // Version check: v1 just checks client_version is present and non-empty.
    // If missing, accept with a warning log.
    if let Some(ref version) = req.client_version {
        if version.is_empty() {
            warn!(worker_id = %req.id, "Worker registered with empty client_version");
        }
    } else {
        warn!(worker_id = %req.id, "Worker registered without client_version");
    }

    let labels = serde_json::to_value(&req.labels)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to serialize labels: {}", e)))?;
    let capabilities = serde_json::to_value(&req.capabilities)
        .map_err(|e| {
            AppError::Internal(anyhow::anyhow!("Failed to serialize capabilities: {}", e))
        })?;

    // Upsert: insert or update if same ID exists
    sqlx::query(
        r#"
        INSERT INTO workers (id, host, labels, capabilities, client_version, status, last_seen_at, created_at)
        VALUES ($1, $2, $3, $4, $5, 'active', NOW(), NOW())
        ON CONFLICT (id) DO UPDATE SET
            host = EXCLUDED.host,
            labels = EXCLUDED.labels,
            capabilities = EXCLUDED.capabilities,
            client_version = EXCLUDED.client_version,
            status = 'active',
            last_seen_at = NOW()
        "#,
    )
    .bind(&req.id)
    .bind(&req.host)
    .bind(&labels)
    .bind(&capabilities)
    .bind(&req.client_version)
    .execute(&state.db)
    .await
    .map_err(|e| AppError::Internal(anyhow::anyhow!("DB error: {}", e)))?;

    info!(worker_id = %req.id, host = %req.host, "Worker registered");

    Ok((
        StatusCode::CREATED,
        Json(RegisterWorkerResponse {
            worker_id: req.id,
        }),
    ))
}

/// POST /workers/:id/heartbeat — Update last_seen_at, accept status and current_job_id.
pub async fn heartbeat(
    State(state): State<AppState>,
    Path(worker_id): Path<String>,
    Json(req): Json<HeartbeatRequest>,
) -> Result<Json<HeartbeatResponse>, AppError> {
    let result = sqlx::query(
        r#"
        UPDATE workers SET last_seen_at = NOW(), status = 'active'
        WHERE id = $1
        "#,
    )
    .bind(&worker_id)
    .execute(&state.db)
    .await
    .map_err(|e| AppError::Internal(anyhow::anyhow!("DB error: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!(
            "Worker '{}' not found",
            worker_id
        )));
    }

    info!(
        worker_id = %worker_id,
        status = ?req.status,
        current_job_id = ?req.current_job_id,
        "Worker heartbeat received"
    );

    Ok(Json(HeartbeatResponse { ok: true }))
}

/// Internal struct to hold row data with created_at for cursor pagination.
struct WorkerRow {
    id: String,
    host: String,
    labels: JsonValue,
    status: String,
    last_seen_at: DateTime<Utc>,
    created_at: DateTime<Utc>,
}

/// GET /workers — List all workers with cursor-based pagination.
pub async fn list_workers(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<PaginatedResponse<WorkerListItem>>, AppError> {
    let limit = params.limit.unwrap_or(20).min(100) as i64;
    let fetch_limit = limit + 1;

    let rows: Vec<WorkerListRow> = if let Some(ref cursor) = params.cursor {
        let cursor_time: DateTime<Utc> = cursor
            .parse()
            .map_err(|_| AppError::InvalidRequest("Invalid cursor".to_string()))?;

        sqlx::query_as(
            r#"
                SELECT id, host, labels, status, last_seen_at, created_at
                FROM workers
                WHERE created_at < $1
                ORDER BY created_at DESC
                LIMIT $2
                "#,
        )
        .bind(cursor_time)
        .bind(fetch_limit)
        .fetch_all(&state.db)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("DB error: {}", e)))?
    } else {
        sqlx::query_as(
            r#"
                SELECT id, host, labels, status, last_seen_at, created_at
                FROM workers
                ORDER BY created_at DESC
                LIMIT $1
                "#,
        )
        .bind(fetch_limit)
        .fetch_all(&state.db)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("DB error: {}", e)))?
    };

    let has_next = rows.len() as i64 > limit;

    let worker_rows: Vec<WorkerRow> = rows
        .into_iter()
        .take(limit as usize)
        .map(|(id, host, labels, status, last_seen_at, created_at)| WorkerRow {
            id,
            host,
            labels,
            status,
            last_seen_at,
            created_at,
        })
        .collect();

    let next_cursor = if has_next {
        worker_rows.last().map(|r| r.created_at.to_rfc3339())
    } else {
        None
    };

    let items: Vec<WorkerListItem> = worker_rows
        .into_iter()
        .map(|r| {
            let labels_map = serde_json::from_value(r.labels).unwrap_or_default();
            WorkerListItem {
                worker_id: r.id,
                host: r.host,
                labels: labels_map,
                status: r.status,
                last_seen_at: r.last_seen_at,
            }
        })
        .collect();

    Ok(Json(PaginatedResponse { items, next_cursor }))
}

/// GET /workers/:id — Get a single worker by ID.
pub async fn get_worker(
    State(state): State<AppState>,
    Path(worker_id): Path<String>,
) -> Result<Json<WorkerDetail>, AppError> {
    let row: Option<WorkerDetailRow> = sqlx::query_as(
        r#"
            SELECT id, host, labels, capabilities, status, last_seen_at, client_version
            FROM workers
            WHERE id = $1
            "#,
    )
    .bind(&worker_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| AppError::Internal(anyhow::anyhow!("DB error: {}", e)))?;

    match row {
        Some((id, host, labels, capabilities, status, last_seen_at, _client_version)) => {
            let labels_map = serde_json::from_value(labels).unwrap_or_default();
            let caps: Option<Vec<String>> = serde_json::from_value(capabilities).ok();
            Ok(Json(WorkerDetail {
                worker_id: id,
                host,
                labels: labels_map,
                status,
                last_seen_at,
                capabilities: caps,
            }))
        }
        None => Err(AppError::NotFound(format!(
            "Worker '{}' not found",
            worker_id
        ))),
    }
}

/// DELETE /workers/:id — Remove worker and return assigned jobs to pending.
pub async fn delete_worker(
    State(state): State<AppState>,
    Path(worker_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    // Check worker exists
    let exists: Option<(String,)> =
        sqlx::query_as("SELECT id FROM workers WHERE id = $1")
            .bind(&worker_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("DB error: {}", e)))?;

    if exists.is_none() {
        return Err(AppError::NotFound(format!(
            "Worker '{}' not found",
            worker_id
        )));
    }

    // Return assigned/running jobs to pending (increment reclaim_count)
    let reclaimed = sqlx::query(
        r#"
        UPDATE jobs
        SET status = 'pending',
            worker_id = NULL,
            assigned_at = NULL,
            reclaim_count = reclaim_count + 1,
            updated_at = NOW()
        WHERE worker_id = $1 AND status IN ('assigned', 'running')
        "#,
    )
    .bind(&worker_id)
    .execute(&state.db)
    .await
    .map_err(|e| AppError::Internal(anyhow::anyhow!("DB error: {}", e)))?;

    if reclaimed.rows_affected() > 0 {
        info!(
            worker_id = %worker_id,
            reclaimed_jobs = reclaimed.rows_affected(),
            "Reclaimed jobs from deleted worker"
        );
    }

    // Delete the worker
    sqlx::query("DELETE FROM workers WHERE id = $1")
        .bind(&worker_id)
        .execute(&state.db)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("DB error: {}", e)))?;

    info!(worker_id = %worker_id, "Worker deleted");

    Ok(StatusCode::NO_CONTENT)
}

/// POST /workers/tasks/pull — Worker requests work.
///
/// Steps (in a single transaction):
/// 1. Reclaim jobs from stale workers (bounded by max_job_reclaims).
/// 2. Fail over-reclaimed jobs.
/// 3. Reclaim lease-expired jobs if job_lease_seconds > 0.
/// 4. Select oldest pending job.
/// 5. Assign to worker (set worker_id, status=assigned, assigned_at).
/// 6. Build task payload with credentials and return.
pub async fn pull_task(
    State(state): State<AppState>,
    Json(req): Json<PullTaskRequest>,
) -> Result<impl IntoResponse, AppError> {
    let worker_id = req.worker_id.as_deref().unwrap_or("");

    // Verify worker exists if worker_id provided
    if !worker_id.is_empty() {
        let exists: Option<(String,)> =
            sqlx::query_as("SELECT id FROM workers WHERE id = $1")
                .bind(worker_id)
                .fetch_optional(&state.db)
                .await
                .map_err(|e| AppError::Internal(anyhow::anyhow!("DB error: {}", e)))?;
        if exists.is_none() {
            return Err(AppError::NotFound(format!(
                "Worker '{}' not found",
                worker_id
            )));
        }
    }

    let stale_threshold_secs = state.config.worker_stale_seconds as f64;
    let max_reclaims = state.config.max_job_reclaims;
    let lease_secs = state.config.job_lease_seconds as f64;

    // Begin transaction for reclaim + select + assign
    let mut tx = state
        .db
        .begin()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("DB error: {}", e)))?;

    // Step 1: Reclaim jobs from stale workers (under reclaim limit)
    let reclaimed = sqlx::query(
        r#"
        UPDATE jobs SET worker_id = NULL, status = 'pending',
            reclaim_count = reclaim_count + 1, updated_at = NOW()
        WHERE status IN ('assigned', 'running')
          AND worker_id IN (
              SELECT id FROM workers
              WHERE last_seen_at < NOW() - make_interval(secs => $1)
          )
          AND reclaim_count < $2
        "#,
    )
    .bind(stale_threshold_secs)
    .bind(max_reclaims)
    .execute(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(anyhow::anyhow!("DB error: {}", e)))?;

    if reclaimed.rows_affected() > 0 {
        warn!(
            reclaimed = reclaimed.rows_affected(),
            "Reclaimed jobs from stale workers during pull"
        );
    }

    // Step 2: Fail over-reclaimed jobs
    let failed_over = sqlx::query(
        r#"
        UPDATE jobs SET status = 'failed',
            error_message = '[MAX_WORKER_LOSS_RETRIES]', updated_at = NOW()
        WHERE status IN ('assigned', 'running')
          AND worker_id IN (
              SELECT id FROM workers
              WHERE last_seen_at < NOW() - make_interval(secs => $1)
          )
          AND reclaim_count >= $2
        "#,
    )
    .bind(stale_threshold_secs)
    .bind(max_reclaims)
    .execute(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(anyhow::anyhow!("DB error: {}", e)))?;

    if failed_over.rows_affected() > 0 {
        warn!(
            failed = failed_over.rows_affected(),
            "Failed over-reclaimed jobs"
        );
    }

    // Step 3: Reclaim lease-expired jobs (if job_lease_seconds > 0)
    if lease_secs > 0.0 {
        let lease_reclaimed = sqlx::query(
            r#"
            UPDATE jobs SET worker_id = NULL, status = 'pending',
                reclaim_count = reclaim_count + 1, updated_at = NOW()
            WHERE status IN ('assigned', 'running')
              AND assigned_at IS NOT NULL
              AND assigned_at < NOW() - make_interval(secs => $1)
              AND reclaim_count < $2
            "#,
        )
        .bind(lease_secs)
        .bind(max_reclaims)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("DB error: {}", e)))?;

        if lease_reclaimed.rows_affected() > 0 {
            warn!(
                reclaimed = lease_reclaimed.rows_affected(),
                "Reclaimed lease-expired jobs during pull"
            );
        }
    }

    // Step 4: Select oldest pending job
    let pending_job: Option<(String, String, JsonValue, i32)> = sqlx::query_as(
        r#"
        SELECT id, session_id, task_input, iteration_index
        FROM jobs
        WHERE status = 'pending' AND worker_id IS NULL
        ORDER BY created_at ASC
        LIMIT 1
        FOR UPDATE SKIP LOCKED
        "#,
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(anyhow::anyhow!("DB error: {}", e)))?;

    let (job_id, session_id, task_input, _iteration_index) = match pending_job {
        Some(row) => row,
        None => {
            tx.commit()
                .await
                .map_err(|e| AppError::Internal(anyhow::anyhow!("DB error: {}", e)))?;
            return Ok(StatusCode::NO_CONTENT.into_response());
        }
    };

    // Step 5: Assign job to worker
    let assign_worker = if worker_id.is_empty() {
        None
    } else {
        Some(worker_id)
    };
    sqlx::query(
        r#"
        UPDATE jobs SET status = 'assigned', worker_id = $1,
            assigned_at = NOW(), updated_at = NOW()
        WHERE id = $2
        "#,
    )
    .bind(assign_worker)
    .bind(&job_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(anyhow::anyhow!("DB error: {}", e)))?;

    // Emit job_started event
    state.event_broadcaster.send(api_types::SessionEvent {
        session_id: session_id.clone(),
        event: "job_started".to_string(),
        job_id: Some(job_id.clone()),
        payload: None,
    });

    // Also mark the session as running if it's still pending
    let session_update_result = sqlx::query(
        "UPDATE sessions SET status = 'running', updated_at = NOW() WHERE id = $1 AND status = 'pending'",
    )
    .bind(&session_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(anyhow::anyhow!("DB error: {}", e)))?;

    // Emit session started event if this was the first job
    if session_update_result.rows_affected() > 0 {
        state.event_broadcaster.send(api_types::SessionEvent {
            session_id: session_id.clone(),
            event: "started".to_string(),
            job_id: None,
            payload: None,
        });
    }

    // Step 6: Build task payload
    // Fetch session details
    let session_row: (String, String, String, JsonValue, Option<String>, String) = sqlx::query_as(
        r#"
        SELECT repo_url, ref_name, workflow, params, persona_id, identity_id
        FROM sessions WHERE id = $1
        "#,
    )
    .bind(&session_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(anyhow::anyhow!("DB error: {}", e)))?;

    let (repo_url, ref_name, workflow, params, persona_id, identity_id) = session_row;

    // Resolve persona prompt
    let prompt_context = if let Some(ref pid) = persona_id {
        let persona: Option<(String,)> =
            sqlx::query_as("SELECT prompt FROM personas WHERE id = $1")
                .bind(pid)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|e| AppError::Internal(anyhow::anyhow!("DB error: {}", e)))?;
        persona.map(|(prompt,)| prompt)
    } else {
        None
    };

    // Resolve credentials from identity + params
    let identity_row: Option<(Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT agent_token, git_token FROM identities WHERE id = $1",
    )
    .bind(&identity_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(anyhow::anyhow!("DB error: {}", e)))?;

    let (id_agent_token, id_git_token) = identity_row.unwrap_or((None, None));

    // Params override identity tokens
    let agent_token = params
        .get("agent_token")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .or(id_agent_token);

    let git_token = params
        .get("git_token")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .or(id_git_token);

    tx.commit()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("DB error: {}", e)))?;

    // Attempt token refresh if needed (after tx commit so we don't hold the tx open during HTTP call)
    let git_token = match super::oauth::maybe_refresh_token(&state.db, &state.config, &identity_id).await {
        Ok(Some(refreshed)) => Some(refreshed),
        Ok(None) => git_token,
        Err(e) => {
            warn!(error = %e, identity_id = %identity_id, "Token refresh check failed, using existing token");
            git_token
        }
    };

    info!(
        job_id = %job_id,
        session_id = %session_id,
        worker_id = %worker_id,
        has_agent_token = agent_token.is_some(),
        has_git_token = git_token.is_some(),
        "Task assigned to worker"
    );

    let task_id = job_id.clone();
    let response = PullTaskResponse {
        task_id: Some(task_id),
        job_id: Some(job_id),
        session_id: Some(session_id),
        repo_url: Some(repo_url),
        ref_name: Some(ref_name),
        workflow: Some(workflow),
        prompt_context,
        task_input: Some(task_input),
        params: Some(params),
        credentials: Some(TaskCredentials {
            git_token,
            agent_token,
        }),
    };

    Ok((StatusCode::OK, Json(response)).into_response())
}

/// POST /workers/tasks/:id/complete — Worker reports task done.
pub async fn task_complete(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    Json(req): Json<TaskCompleteRequest>,
) -> Result<Json<TaskCompleteResponse>, AppError> {
    // Look up the job
    let job_row: Option<(String, String, Option<String>, i32)> = sqlx::query_as(
        "SELECT id, session_id, worker_id, iteration_index FROM jobs WHERE id = $1",
    )
    .bind(&task_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| AppError::Internal(anyhow::anyhow!("DB error: {}", e)))?;

    let (job_id, session_id, existing_worker_id, iteration_index) = job_row.ok_or_else(|| {
        AppError::NotFound(format!("Task '{}' not found", task_id))
    })?;

    // If worker_id provided, validate it matches
    if let Some(ref req_worker) = req.worker_id {
        if let Some(ref existing) = existing_worker_id {
            if req_worker != existing {
                return Err(AppError::InvalidRequest(format!(
                    "Task '{}' is assigned to worker '{}', not '{}'",
                    task_id, existing, req_worker
                )));
            }
        }
    }

    // Determine job status from request
    let job_status = match req.status {
        api_types::TaskCompleteStatus::Success => "completed",
        api_types::TaskCompleteStatus::Failed => "failed",
    };

    // Update job with completion data
    sqlx::query(
        r#"
        UPDATE jobs SET
            status = $1,
            branch = $2,
            commit_ref = $3,
            mr_title = $4,
            mr_description = $5,
            error_message = $6,
            output = $7,
            sentinel_reached = COALESCE($8, FALSE),
            assistant_reply = $9,
            updated_at = NOW()
        WHERE id = $10
        "#,
    )
    .bind(job_status)
    .bind(&req.branch)
    .bind(&req.commit_ref)
    .bind(&req.mr_title)
    .bind(&req.mr_description)
    .bind(&req.error_message)
    .bind(&req.output)
    .bind(req.sentinel_reached)
    .bind(&req.assistant_reply)
    .bind(&job_id)
    .execute(&state.db)
    .await
    .map_err(|e| AppError::Internal(anyhow::anyhow!("DB error: {}", e)))?;

    info!(
        job_id = %job_id,
        session_id = %session_id,
        status = %job_status,
        "Task completed"
    );

    // Handle session state machine transitions
    engine::handle_task_complete(
        &state.db,
        &session_id,
        &job_id,
        job_status,
        iteration_index,
        req.sentinel_reached.unwrap_or(false),
        &state.event_broadcaster,
    )
    .await?;

    // Attempt PR/MR creation for successful jobs with branch_mode=pr (non-blocking)
    if job_status == "completed" {
        let db = state.db.clone();
        let config = state.config.clone();
        let jid = job_id.clone();
        let sid = session_id.clone();
        tokio::spawn(async move {
            engine::pr::attempt_pr_creation(&db, &config, &jid, &sid).await;
        });
    }

    Ok(Json(TaskCompleteResponse { ok: true }))
}

/// POST /workers/tasks/:id/logs — Worker sends log batch.
pub async fn send_logs(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    Json(entries): Json<Vec<SendLogEntry>>,
) -> Result<(StatusCode, Json<SendLogsResponse>), AppError> {
    // Look up job context for session_id and worker_id
    let job_row: Option<(String, Option<String>)> = sqlx::query_as(
        "SELECT session_id, worker_id FROM jobs WHERE id = $1",
    )
    .bind(&task_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| AppError::Internal(anyhow::anyhow!("DB error: {}", e)))?;

    let (session_id, worker_id) = job_row.ok_or_else(|| {
        AppError::NotFound(format!("Task '{}' not found", task_id))
    })?;

    // Insert log entries and broadcast
    for entry in &entries {
        let log_id = Uuid::new_v4().to_string();
        sqlx::query(
            r#"
            INSERT INTO logs (id, session_id, job_id, worker_id, level, source, message, timestamp)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(&log_id)
        .bind(&session_id)
        .bind(&task_id)
        .bind(&worker_id)
        .bind(&entry.level)
        .bind(&entry.source)
        .bind(&entry.message)
        .bind(entry.timestamp)
        .execute(&state.db)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("DB error: {}", e)))?;

        // Broadcast to SSE subscribers
        let log_entry = LogEntry {
            id: log_id,
            timestamp: entry.timestamp,
            level: entry.level.clone(),
            session_id: session_id.clone(),
            job_id: Some(task_id.clone()),
            worker_id: worker_id.clone(),
            source: entry.source.clone(),
            message: entry.message.clone(),
        };
        state.log_broadcaster.send(log_entry);
    }

    info!(
        task_id = %task_id,
        session_id = %session_id,
        count = entries.len(),
        "Log entries received and stored"
    );

    Ok((
        StatusCode::ACCEPTED,
        Json(SendLogsResponse { accepted: true }),
    ))
}

/// Background task: mark workers as stale when NOW() - last_seen_at > worker_stale_seconds.
pub async fn stale_detection_loop(state: AppState) {
    let interval_secs = 30;
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));

    loop {
        interval.tick().await;

        let stale_threshold_secs = state.config.worker_stale_seconds as f64;

        let result = sqlx::query(
            r#"
            UPDATE workers
            SET status = 'stale'
            WHERE status = 'active'
              AND last_seen_at < NOW() - make_interval(secs => $1)
            "#,
        )
        .bind(stale_threshold_secs)
        .execute(&state.db)
        .await;

        match result {
            Ok(r) => {
                if r.rows_affected() > 0 {
                    warn!(
                        count = r.rows_affected(),
                        threshold_secs = state.config.worker_stale_seconds,
                        "Marked workers as stale"
                    );
                }
            }
            Err(e) => {
                warn!(error = %e, "Stale detection query failed");
            }
        }
    }
}
