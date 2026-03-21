//! Task queue: pull (reclaim, lease, assign) and complete ([`docs/API_OVERVIEW.md`](../../docs/API_OVERVIEW.md) §9).

use crate::auth::AuthError;
use crate::identities::{fetch_identity, IdentityRow};
use crate::inbox::inbox_payload_message;
use crate::oauth::maybe_refresh_oauth_git_token;
use crate::sessions::chat_histories_for_session;
use crate::sse_hub::SessionEventPayload;
use crate::AppState;
use api_types::{
    PullTaskRequest, PullTaskResponse, TaskCompleteRequest, TaskCompleteResponse, TaskCredentials,
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use chrono::{DateTime, Utc};
use serde_json::{json, Map, Value};
use sqlx::PgPool;
use uuid::Uuid;

fn stale_cutoff(state: &AppState) -> DateTime<Utc> {
    let secs = state
        .config
        .worker_stale_threshold
        .as_secs()
        .min(i64::MAX as u64) as i64;
    Utc::now() - chrono::Duration::seconds(secs)
}

fn worker_is_stale(last_seen: Option<DateTime<Utc>>, cutoff: DateTime<Utc>) -> bool {
    match last_seen {
        None => true,
        Some(t) => t < cutoff,
    }
}

fn bytes_to_secret(b: &[u8]) -> Option<String> {
    if b.is_empty() {
        return None;
    }
    String::from_utf8(b.to_vec())
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn merge_credentials(row: &IdentityRow, params: &Value) -> TaskCredentials {
    let agent_from_params = params
        .get("agent_token")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let git_from_params = params
        .get("git_token")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let agent_token = agent_from_params.unwrap_or_else(|| {
        row.agent_token_ciphertext
            .as_ref()
            .and_then(|b| bytes_to_secret(b))
            .unwrap_or_default()
    });
    let git_token = git_from_params.unwrap_or_else(|| {
        row.git_token_ciphertext
            .as_ref()
            .and_then(|b| bytes_to_secret(b))
            .unwrap_or_default()
    });

    TaskCredentials {
        git_token,
        agent_token,
    }
}

/// Chat follow-up jobs store `session_prompt` + `history` / `history_assistant`; cap on pull only ([`docs/API_OVERVIEW.md`](../../docs/API_OVERVIEW.md#pull-task)).
fn cap_json_array_tail(obj: &mut Map<String, Value>, key: &str, max_turns: usize) -> bool {
    let Some(Value::Array(arr)) = obj.get_mut(key) else {
        return false;
    };
    if arr.len() <= max_turns {
        return false;
    }
    let start = arr.len() - max_turns;
    *arr = arr[start..].to_vec();
    true
}

fn apply_chat_history_cap_on_pull(mut task_input: Value, max_turns: u32) -> Value {
    if max_turns == 0 {
        return task_input;
    }
    let follow_up = task_input
        .as_object()
        .is_some_and(|o| o.contains_key("session_prompt"));
    if !follow_up {
        return task_input;
    }
    let max = max_turns as usize;
    let Some(obj) = task_input.as_object_mut() else {
        return task_input;
    };
    let mut truncated = cap_json_array_tail(obj, "history", max);
    truncated |= cap_json_array_tail(obj, "history_assistant", max);
    obj.insert("history_truncated".to_string(), Value::Bool(truncated));
    task_input
}

fn build_task_input(
    job_input: Option<Value>,
    workflow: &str,
    params: &Value,
    chat_history_max_turns: u32,
) -> Value {
    let base = if let Some(v) = job_input {
        if v.is_object() && !v.as_object().is_some_and(|o| o.is_empty()) && !v.is_null() {
            v
        } else if workflow == "chat" {
            let prompt = params.get("prompt").and_then(|v| v.as_str()).unwrap_or("");
            serde_json::json!({ "prompt": prompt })
        } else {
            serde_json::json!({})
        }
    } else if workflow == "chat" {
        let prompt = params.get("prompt").and_then(|v| v.as_str()).unwrap_or("");
        serde_json::json!({ "prompt": prompt })
    } else {
        serde_json::json!({})
    };

    if workflow == "chat" || workflow == "inbox" {
        apply_chat_history_cap_on_pull(base, chat_history_max_turns)
    } else {
        base
    }
}

#[derive(Debug, sqlx::FromRow)]
struct ClaimedJobRow {
    job_id: Uuid,
    session_id: Uuid,
    repo_url: String,
    git_ref: String,
    workflow: String,
    params: sqlx::types::Json<Value>,
    #[allow(dead_code)]
    persona_id: Option<String>,
    identity_id: String,
    task_input: Option<sqlx::types::Json<Value>>,
}

/// Promote the next pending inbox row into an **assigned** job when this worker holds the listener
/// ([`docs/API_OVERVIEW.md`](../../docs/API_OVERVIEW.md) §8).
async fn try_promote_inbox_for_listener(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    worker_id: &str,
) -> Result<Option<ClaimedJobRow>, AuthError> {
    let agent_id: Option<String> =
        sqlx::query_scalar(r#"SELECT agent_id FROM inbox_listeners WHERE worker_id = $1"#)
            .bind(worker_id)
            .fetch_optional(&mut **tx)
            .await
            .map_err(|e| {
                AuthError::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "server_error",
                    &format!("database error: {e}"),
                )
            })?;

    let Some(agent_id) = agent_id else {
        return Ok(None);
    };

    type SessRow = (
        Uuid,
        String,
        String,
        sqlx::types::Json<Value>,
        Option<String>,
        String,
        bool,
    );
    let session: Option<SessRow> = sqlx::query_as(
        r#"
        SELECT id, repo_url, git_ref, params, persona_id, identity_id, retain_forever
        FROM sessions
        WHERE workflow = 'inbox'
          AND status = 'running'
          AND params->>'agent_id' = $1
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .bind(&agent_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|e| {
        AuthError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            &format!("database error: {e}"),
        )
    })?;

    let Some((
        session_id,
        repo_url,
        git_ref,
        params,
        persona_id,
        identity_id,
        retain_forever,
    )) = session
    else {
        return Ok(None);
    };

    let task: Option<(Uuid, sqlx::types::Json<Value>)> = sqlx::query_as(
        r#"
        SELECT id, payload
        FROM inbox_tasks
        WHERE agent_id = $1 AND status = 'pending'
        ORDER BY enqueued_at ASC, id ASC
        FOR UPDATE SKIP LOCKED
        LIMIT 1
        "#,
    )
    .bind(&agent_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|e| {
        AuthError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            &format!("database error: {e}"),
        )
    })?;

    let Some((inbox_task_id, payload)) = task else {
        return Ok(None);
    };

    let msg = inbox_payload_message(&payload.0)?;
    let (history, history_assistant) = chat_histories_for_session(tx, session_id)
        .await
        .map_err(|e| {
            AuthError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                &format!("database error: {e}"),
            )
        })?;

    let session_prompt = params
        .0
        .get("prompt")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let task_input = json!({
        "session_prompt": session_prompt,
        "message": msg,
        "history": history,
        "history_assistant": history_assistant,
        "history_truncated": false
    });

    let next_ord: i32 = sqlx::query_scalar(
        r#"SELECT COALESCE(MAX(queue_ordinal), 0) + 1 FROM jobs WHERE session_id = $1"#,
    )
    .bind(session_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(|e| {
        AuthError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            &format!("database error: {e}"),
        )
    })?;

    let job_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO jobs (session_id, status, task_input, retain_forever, worker_id, assigned_at, updated_at, queue_ordinal)
        VALUES ($1, 'assigned', $2, $3, $4, now(), now(), $5)
        RETURNING id
        "#,
    )
    .bind(session_id)
    .bind(sqlx::types::Json(task_input.clone()))
    .bind(retain_forever)
    .bind(worker_id)
    .bind(next_ord)
    .fetch_one(&mut **tx)
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
        UPDATE inbox_tasks
        SET status = 'promoted', promoted_job_id = $2
        WHERE id = $1
        "#,
    )
    .bind(inbox_task_id)
    .bind(job_id)
    .execute(&mut **tx)
    .await
    .map_err(|e| {
        AuthError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            &format!("database error: {e}"),
        )
    })?;

    Ok(Some(ClaimedJobRow {
        job_id,
        session_id,
        repo_url,
        git_ref,
        workflow: "inbox".to_string(),
        params,
        persona_id,
        identity_id,
        task_input: Some(sqlx::types::Json(task_input)),
    }))
}

/// `POST /workers/tasks/pull`
pub async fn pull_task(
    State(state): State<AppState>,
    Json(body): Json<PullTaskRequest>,
) -> Result<Response, AuthError> {
    let Some(pool) = &state.db else {
        return Err(AuthError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "not_ready",
            "Database is not configured; cannot pull tasks",
        ));
    };

    let worker_id = body
        .worker_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            AuthError::new(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "worker_id is required in the request body",
            )
        })?
        .to_string();

    let max_r = state.config.max_job_reclaims;
    let lease_secs = state.config.job_lease_seconds as i64;
    let stale_at = stale_cutoff(&state);

    let mut tx = pool.begin().await.map_err(|e| {
        AuthError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            &format!("database error: {e}"),
        )
    })?;

    if lease_secs > 0 {
        let lease = sqlx::query(
            r#"
            UPDATE jobs
            SET
                status = 'failed',
                worker_id = NULL,
                completed_at = now(),
                updated_at = now(),
                assigned_at = NULL,
                error_message = CASE
                    WHEN error_message IS NULL OR error_message = '' THEN '[JOB_LEASE_EXPIRED]'
                    ELSE error_message || E'\n' || '[JOB_LEASE_EXPIRED]'
                END
            WHERE status = 'assigned'
              AND assigned_at IS NOT NULL
              AND assigned_at + ($1::bigint * INTERVAL '1 second') < now()
            "#,
        )
        .bind(lease_secs)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            AuthError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                &format!("database error: {e}"),
            )
        })?;
        let n = lease.rows_affected();
        if n > 0 {
            eprintln!("remote-harness server: job lease expired for {n} job(s) on pull_task");
        }
    }

    let stale_reclaimed = sqlx::query(
        r#"
        UPDATE jobs
        SET
            worker_id = NULL,
            status = 'pending',
            reclaim_count = reclaim_count + 1,
            assigned_at = NULL,
            updated_at = now()
        WHERE status = 'assigned'
          AND worker_id IN (
            SELECT id FROM workers WHERE last_seen_at < $1 OR last_seen_at IS NULL
          )
          AND reclaim_count < $2
        "#,
    )
    .bind(stale_at)
    .bind(max_r)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        AuthError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            &format!("database error: {e}"),
        )
    })?;
    if stale_reclaimed.rows_affected() > 0 {
        eprintln!(
            "remote-harness server: reclaimed {} job(s) from stale workers on pull_task",
            stale_reclaimed.rows_affected()
        );
    }

    sqlx::query(
        r#"
        UPDATE jobs
        SET
            worker_id = NULL,
            status = 'failed',
            updated_at = now(),
            completed_at = now(),
            assigned_at = NULL,
            error_message = CASE
                WHEN error_message IS NULL OR error_message = '' THEN '[MAX_WORKER_LOSS_RETRIES]'
                ELSE error_message || E'\n' || '[MAX_WORKER_LOSS_RETRIES]'
            END
        WHERE status = 'assigned'
          AND worker_id IN (
            SELECT id FROM workers WHERE last_seen_at < $1 OR last_seen_at IS NULL
          )
          AND reclaim_count >= $2
        "#,
    )
    .bind(stale_at)
    .bind(max_r)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        AuthError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            &format!("database error: {e}"),
        )
    })?;

    let worker_row: Option<Option<DateTime<Utc>>> =
        sqlx::query_scalar(r#"SELECT last_seen_at FROM workers WHERE id = $1"#)
            .bind(&worker_id)
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
        tx.commit().await.map_err(|e| {
            AuthError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                &format!("database error: {e}"),
            )
        })?;
        return Err(AuthError::new(
            StatusCode::NOT_FOUND,
            "not_found",
            "Worker not found",
        ));
    };

    if worker_is_stale(last_seen, stale_at) {
        tx.commit().await.map_err(|e| {
            AuthError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                &format!("database error: {e}"),
            )
        })?;
        return Err(AuthError::new(
            StatusCode::CONFLICT,
            "worker_stale",
            "Worker heartbeat is stale; send a heartbeat before pulling tasks",
        ));
    }

    let existing: Option<ClaimedJobRow> = sqlx::query_as(
        r#"
        SELECT
            j.id AS job_id,
            j.session_id,
            s.repo_url,
            s.git_ref,
            s.workflow,
            s.params AS params,
            s.persona_id,
            s.identity_id,
            j.task_input AS task_input
        FROM jobs j
        INNER JOIN sessions s ON s.id = j.session_id
        WHERE j.worker_id = $1 AND j.status = 'assigned'
        ORDER BY j.created_at ASC, j.queue_ordinal ASC, j.id ASC
        LIMIT 1
        "#,
    )
    .bind(&worker_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| {
        AuthError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            &format!("database error: {e}"),
        )
    })?;

    let (claimed, fresh_assign) = if let Some(row) = existing {
        (Some(row), false)
    } else if let Some(row) = try_promote_inbox_for_listener(&mut tx, &worker_id).await? {
        (Some(row), true)
    } else {
        let row = sqlx::query_as::<_, ClaimedJobRow>(
            r#"
            WITH c AS (
                SELECT j.id
                FROM jobs j
                WHERE j.status = 'pending'
                ORDER BY j.created_at ASC, j.queue_ordinal ASC, j.id ASC
                FOR UPDATE OF j SKIP LOCKED
                LIMIT 1
            ),
            u AS (
                UPDATE jobs j
                SET
                    status = 'assigned',
                    worker_id = $1,
                    assigned_at = now(),
                    updated_at = now()
                FROM c
                WHERE j.id = c.id
                RETURNING
                    j.id AS job_id,
                    j.session_id,
                    j.task_input AS task_input
            )
            SELECT
                u.job_id,
                u.session_id,
                s.repo_url,
                s.git_ref,
                s.workflow,
                s.params AS params,
                s.persona_id,
                s.identity_id,
                u.task_input AS task_input
            FROM u
            INNER JOIN sessions s ON s.id = u.session_id
            "#,
        )
        .bind(&worker_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| {
            AuthError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                &format!("database error: {e}"),
            )
        })?;
        match row {
            Some(r) => (Some(r), true),
            None => (None, false),
        }
    };

    let mut session_became_running = false;
    if fresh_assign {
        if let Some(ref c) = claimed {
            let r = sqlx::query(
                r#"
                UPDATE sessions
                SET status = 'running', updated_at = now()
                WHERE id = $1 AND status = 'pending'
                "#,
            )
            .bind(c.session_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                AuthError::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "server_error",
                    &format!("database error: {e}"),
                )
            })?;
            session_became_running = r.rows_affected() > 0;
        }
    }

    tx.commit().await.map_err(|e| {
        AuthError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            &format!("database error: {e}"),
        )
    })?;

    if fresh_assign {
        if let Some(ref c) = claimed {
            if session_became_running {
                state.sse.emit_session_event(
                    c.session_id,
                    SessionEventPayload {
                        event: "started".to_string(),
                        job_id: Some(c.job_id.to_string()),
                        payload: json!({}),
                    },
                );
            }
            state.sse.emit_session_event(
                c.session_id,
                SessionEventPayload {
                    event: "job_started".to_string(),
                    job_id: Some(c.job_id.to_string()),
                    payload: json!({}),
                },
            );
        }
    }

    let Some(claimed) = claimed else {
        return Ok(StatusCode::NO_CONTENT.into_response());
    };

    finish_pull_response(&state, pool, claimed).await
}

async fn finish_pull_response(
    state: &AppState,
    pool: &PgPool,
    claimed: ClaimedJobRow,
) -> Result<Response, AuthError> {
    let mut id_row = fetch_identity(pool, &claimed.identity_id)
        .await
        .map_err(|e| {
            AuthError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                &format!("database error: {e}"),
            )
        })?
        .ok_or_else(|| {
            AuthError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                "Session identity missing",
            )
        })?;

    maybe_refresh_oauth_git_token(state, pool, &claimed.identity_id, &mut id_row).await?;

    let params = claimed.params.0.clone();
    let task_input = build_task_input(
        claimed.task_input.map(|j| j.0),
        &claimed.workflow,
        &params,
        state.config.chat_history_max_turns,
    );
    let credentials = merge_credentials(&id_row, &params);

    let body = PullTaskResponse {
        task_id: claimed.job_id.to_string(),
        job_id: claimed.job_id.to_string(),
        session_id: claimed.session_id.to_string(),
        repo_url: claimed.repo_url,
        git_ref: claimed.git_ref,
        workflow: claimed.workflow,
        prompt_context: String::new(),
        task_input,
        params,
        credentials,
    };

    Ok((StatusCode::OK, Json(body)).into_response())
}

/// `POST /workers/tasks/:id/complete`
pub async fn complete_task(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    Json(body): Json<TaskCompleteRequest>,
) -> Result<Json<TaskCompleteResponse>, AuthError> {
    let Some(pool) = &state.db else {
        return Err(AuthError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "not_ready",
            "Database is not configured; cannot complete tasks",
        ));
    };

    let task_id = task_id.trim();
    if task_id.is_empty() {
        return Err(AuthError::new(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "task id must not be empty",
        ));
    }

    let job_uuid = Uuid::parse_str(task_id).map_err(|_| {
        AuthError::new(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "task id must be a UUID",
        )
    })?;

    let db_status = match body.status.trim() {
        "success" => "completed",
        "failed" => "failed",
        _ => {
            return Err(AuthError::new(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "status must be \"success\" or \"failed\"",
            ));
        }
    };

    let mut tx = pool.begin().await.map_err(|e| {
        AuthError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            &format!("database error: {e}"),
        )
    })?;

    type JobCheck = (String, Option<String>, Uuid, String);
    let row: Option<JobCheck> = sqlx::query_as(
        r#"
        SELECT j.status, j.worker_id, j.session_id, s.workflow
        FROM jobs j
        INNER JOIN sessions s ON s.id = j.session_id
        WHERE j.id = $1
        "#,
    )
    .bind(job_uuid)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| {
        AuthError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            &format!("database error: {e}"),
        )
    })?;

    let Some((status, assigned_worker, session_id, workflow)) = row else {
        return Err(AuthError::new(
            StatusCode::NOT_FOUND,
            "not_found",
            "Task not found",
        ));
    };

    if status != "assigned" {
        return Err(AuthError::new(
            StatusCode::NOT_FOUND,
            "not_found",
            "Task not found or already completed",
        ));
    }

    if let Some(ref w) = body.worker_id {
        let w = w.trim();
        if !w.is_empty() && assigned_worker.as_deref() != Some(w) {
            return Err(AuthError::new(
                StatusCode::CONFLICT,
                "conflict",
                "Task is not assigned to this worker",
            ));
        }
    }

    let err_msg = body.error_message.clone();
    let err_update = match db_status {
        "failed" => err_msg
            .filter(|s| !s.trim().is_empty())
            .or_else(|| Some("failed".to_string())),
        _ => err_msg,
    };

    let res = sqlx::query(
        r#"
        UPDATE jobs
        SET
            status = $2,
            worker_id = NULL,
            completed_at = now(),
            updated_at = now(),
            error_message = $3,
            branch = $4,
            commit_ref = $5,
            mr_title = $6,
            mr_description = $7,
            output_snippet = $8,
            assistant_reply = $9,
            sentinel_reached = $10
        WHERE id = $1 AND status = 'assigned'
        "#,
    )
    .bind(job_uuid)
    .bind(db_status)
    .bind(err_update)
    .bind(body.branch.as_deref())
    .bind(body.commit_ref.as_deref())
    .bind(body.mr_title.as_deref())
    .bind(body.mr_description.as_deref())
    .bind(body.output.as_deref())
    .bind(body.assistant_reply.as_deref())
    .bind(body.sentinel_reached)
    .execute(&mut *tx)
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
            "Task not found or already completed",
        ));
    }

    if db_status == "failed" && matches!(workflow.as_str(), "loop_n" | "loop_until_sentinel") {
        sqlx::query(
            r#"
            UPDATE jobs
            SET
                status = 'failed',
                completed_at = now(),
                updated_at = now(),
                error_message = CASE
                    WHEN error_message IS NULL OR error_message = '' THEN '[CANCELLED_SESSION_FAILED]'
                    ELSE error_message || E'\n[CANCELLED_SESSION_FAILED]'
                END
            WHERE session_id = $1 AND status = 'pending'
            "#,
        )
        .bind(session_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            AuthError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                &format!("database error: {e}"),
            )
        })?;
    }

    let mut loop_sentinel_enqueued_followup = false;

    if db_status == "completed" && workflow == "loop_until_sentinel" {
        let done_success: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)::bigint FROM jobs WHERE session_id = $1 AND status = 'completed'",
        )
        .bind(session_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| {
            AuthError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                &format!("database error: {e}"),
            )
        })?;

        let max_sentinel = state.config.loop_until_sentinel_max_iterations.max(1) as i64;
        let sentinel_hit = body.sentinel_reached == Some(true);
        if !sentinel_hit && done_success < max_sentinel {
            type SessRow = (sqlx::types::Json<Value>, bool);
            let srow: Option<SessRow> = sqlx::query_as(
                r#"SELECT params, retain_forever FROM sessions WHERE id = $1 FOR UPDATE"#,
            )
            .bind(session_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| {
                AuthError::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "server_error",
                    &format!("database error: {e}"),
                )
            })?;

            let Some((params_json, session_retain)) = srow else {
                return Err(AuthError::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "server_error",
                    "Session missing after job completion",
                ));
            };

            let prompt = params_json
                .0
                .get("prompt")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let next_iteration = done_success + 1;
            let task_in = json!({
                "prompt": prompt,
                "iteration": next_iteration,
            });

            let next_ord: i32 = sqlx::query_scalar(
                "SELECT COALESCE(MAX(queue_ordinal), 0) + 1 FROM jobs WHERE session_id = $1",
            )
            .bind(session_id)
            .fetch_one(&mut *tx)
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
                INSERT INTO jobs (session_id, status, task_input, retain_forever, queue_ordinal)
                VALUES ($1, 'pending', $2, $3, $4)
                "#,
            )
            .bind(session_id)
            .bind(sqlx::types::Json(task_in))
            .bind(session_retain)
            .bind(next_ord)
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                AuthError::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "server_error",
                    &format!("database error: {e}"),
                )
            })?;

            loop_sentinel_enqueued_followup = true;
        }
    }

    let active_loop_n: Option<i64> = if db_status == "completed" && workflow == "loop_n" {
        Some(
            sqlx::query_scalar(
                "SELECT COUNT(*)::bigint FROM jobs WHERE session_id = $1 AND status IN ('pending','assigned')",
            )
            .bind(session_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| {
                AuthError::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "server_error",
                    &format!("database error: {e}"),
                )
            })?,
        )
    } else {
        None
    };

    let session_status = if db_status == "failed" {
        "failed"
    } else if workflow == "chat" || workflow == "inbox" {
        "running"
    } else if workflow == "loop_n" {
        match active_loop_n {
            Some(n) if n > 0 => "running",
            _ => "completed",
        }
    } else if workflow == "loop_until_sentinel" {
        if loop_sentinel_enqueued_followup {
            "running"
        } else {
            "completed"
        }
    } else {
        "completed"
    };

    sqlx::query(
        r#"
        UPDATE sessions
        SET status = $2, updated_at = now()
        WHERE id = $1
        "#,
    )
    .bind(session_id)
    .bind(session_status)
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

    let worker_reported = body.status.trim().to_string();
    state.sse.emit_session_event(
        session_id,
        SessionEventPayload {
            event: "job_completed".to_string(),
            job_id: Some(job_uuid.to_string()),
            payload: json!({ "worker_reported": worker_reported }),
        },
    );
    match session_status {
        "completed" => {
            state.sse.emit_session_event(
                session_id,
                SessionEventPayload {
                    event: "completed".to_string(),
                    job_id: Some(job_uuid.to_string()),
                    payload: json!({}),
                },
            );
        }
        "failed" => {
            state.sse.emit_session_event(
                session_id,
                SessionEventPayload {
                    event: "failed".to_string(),
                    job_id: Some(job_uuid.to_string()),
                    payload: json!({}),
                },
            );
        }
        _ => {}
    }

    Ok(Json(TaskCompleteResponse { ok: true }))
}

#[cfg(test)]
mod chat_history_cap_tests {
    use super::*;

    #[test]
    fn cap_keeps_last_n_and_sets_truncated_when_dropped() {
        let v = serde_json::json!({
            "session_prompt": "goal",
            "message": "now",
            "history": ["u1", "u2", "u3"],
            "history_assistant": ["a0", "a1", "a2", "a3"],
            "history_truncated": false
        });
        let out = apply_chat_history_cap_on_pull(v, 2);
        assert_eq!(
            out,
            serde_json::json!({
                "session_prompt": "goal",
                "message": "now",
                "history": ["u2", "u3"],
                "history_assistant": ["a2", "a3"],
                "history_truncated": true
            })
        );
    }

    #[test]
    fn cap_sets_truncated_false_when_under_limit() {
        let v = serde_json::json!({
            "session_prompt": "goal",
            "message": "m2",
            "history": ["m1"],
            "history_assistant": ["a0", "a1"],
            "history_truncated": false
        });
        let out = apply_chat_history_cap_on_pull(v, 2);
        assert_eq!(
            out,
            serde_json::json!({
                "session_prompt": "goal",
                "message": "m2",
                "history": ["m1"],
                "history_assistant": ["a0", "a1"],
                "history_truncated": false
            })
        );
    }

    #[test]
    fn cap_skips_first_chat_job_shape() {
        let v = serde_json::json!({ "prompt": "hi" });
        let out = apply_chat_history_cap_on_pull(v, 2);
        assert_eq!(out, serde_json::json!({ "prompt": "hi" }));
    }

    #[test]
    fn cap_disabled_when_max_turns_zero() {
        let v = serde_json::json!({
            "session_prompt": "goal",
            "message": "now",
            "history": ["u1", "u2", "u3"],
            "history_assistant": ["a"],
            "history_truncated": false
        });
        let out = apply_chat_history_cap_on_pull(v, 0);
        assert_eq!(out["history"].as_array().unwrap().len(), 3);
        assert_eq!(out["history_truncated"], false);
    }
}
