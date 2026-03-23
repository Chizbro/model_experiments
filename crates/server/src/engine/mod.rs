pub mod pr;
pub mod workflows;

use crate::error::AppError;
use crate::sse::EventBroadcaster;
use api_types::{
    CreateSessionRequest, CreateSessionResponse, SendInputResponse, SessionEvent, SessionStatus,
    WorkflowType,
};
use chrono::Utc;
use sqlx::PgPool;
use std::env;
use std::sync::Arc;
use tracing::info;
use uuid::Uuid;

/// Validate that the identity associated with the session has both agent_token and git_token.
/// Tokens can come from the identity row OR from session params (params override/fill).
async fn validate_credentials(
    pool: &PgPool,
    identity_id: &str,
    params: &serde_json::Value,
) -> Result<(), AppError> {
    // Query the identity row
    let row = sqlx::query_as::<_, (Option<String>, Option<String>)>(
        "SELECT agent_token, git_token FROM identities WHERE id = $1",
    )
    .bind(identity_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| AppError::Internal(e.into()))?
    .ok_or_else(|| {
        AppError::InvalidRequest(format!("identity '{}' not found", identity_id))
    })?;

    let (identity_agent_token, identity_git_token) = row;

    // Merge: params override identity
    let has_agent_token = params
        .get("agent_token")
        .and_then(|v| v.as_str())
        .map(|s| !s.is_empty())
        .unwrap_or(false)
        || identity_agent_token
            .as_ref()
            .map(|t| !t.is_empty())
            .unwrap_or(false);

    let has_git_token = params
        .get("git_token")
        .and_then(|v| v.as_str())
        .map(|s| !s.is_empty())
        .unwrap_or(false)
        || identity_git_token
            .as_ref()
            .map(|t| !t.is_empty())
            .unwrap_or(false);

    if !has_agent_token || !has_git_token {
        return Err(AppError::InvalidRequest(
            "both Git and agent tokens are required".to_string(),
        ));
    }

    Ok(())
}

/// Validate the session creation request params based on workflow type.
fn validate_params(workflow: &WorkflowType, params: &serde_json::Value) -> Result<(), AppError> {
    // All workflows except inbox require prompt and agent_cli
    match workflow {
        WorkflowType::Chat | WorkflowType::LoopN | WorkflowType::LoopUntilSentinel => {
            if params.get("prompt").and_then(|v| v.as_str()).map(|s| s.is_empty()).unwrap_or(true) {
                return Err(AppError::InvalidRequest(
                    "params.prompt is required".to_string(),
                ));
            }
            if params.get("agent_cli").is_none() {
                return Err(AppError::InvalidRequest(
                    "params.agent_cli is required".to_string(),
                ));
            }
            // Validate agent_cli value
            let agent_cli = params.get("agent_cli").and_then(|v| v.as_str()).unwrap_or("");
            if agent_cli != "claude_code" && agent_cli != "cursor" {
                return Err(AppError::InvalidRequest(
                    "params.agent_cli must be 'claude_code' or 'cursor'".to_string(),
                ));
            }
        }
        WorkflowType::Inbox => {
            if params.get("agent_cli").is_none() {
                return Err(AppError::InvalidRequest(
                    "params.agent_cli is required".to_string(),
                ));
            }
            let agent_cli = params.get("agent_cli").and_then(|v| v.as_str()).unwrap_or("");
            if agent_cli != "claude_code" && agent_cli != "cursor" {
                return Err(AppError::InvalidRequest(
                    "params.agent_cli must be 'claude_code' or 'cursor'".to_string(),
                ));
            }
        }
    }

    // loop_n requires n
    if *workflow == WorkflowType::LoopN {
        let n = params.get("n").and_then(|v| v.as_u64());
        if n.is_none() || n == Some(0) {
            return Err(AppError::InvalidRequest(
                "params.n is required and must be > 0 for loop_n workflow".to_string(),
            ));
        }
    }

    // loop_until_sentinel requires sentinel
    if *workflow == WorkflowType::LoopUntilSentinel
        && params.get("sentinel").and_then(|v| v.as_str()).map(|s| s.is_empty()).unwrap_or(true)
    {
        return Err(AppError::InvalidRequest(
            "params.sentinel is required for loop_until_sentinel workflow".to_string(),
        ));
    }

    Ok(())
}

/// Create a new session with its initial jobs.
pub async fn create_session(
    pool: &PgPool,
    req: CreateSessionRequest,
) -> Result<CreateSessionResponse, AppError> {
    let identity_id = req.identity_id.as_deref().unwrap_or("default");

    // Validate params for the workflow
    validate_params(&req.workflow, &req.params)?;

    // Validate credentials
    validate_credentials(pool, identity_id, &req.params).await?;

    // Generate session ID
    let session_id = Uuid::new_v4().to_string();
    let now = Utc::now();

    // Serialize workflow type
    let workflow_str = serde_json::to_value(&req.workflow)
        .map_err(|e| AppError::Internal(e.into()))?
        .as_str()
        .unwrap_or("chat")
        .to_string();

    // Insert session row
    sqlx::query(
        "INSERT INTO sessions (id, repo_url, ref_name, workflow, status, params, identity_id, persona_id, retain_forever, created_at, updated_at)
         VALUES ($1, $2, $3, $4, 'pending', $5, $6, $7, $8, $9, $9)"
    )
    .bind(&session_id)
    .bind(&req.repo_url)
    .bind(&req.ref_name)
    .bind(&workflow_str)
    .bind(&req.params)
    .bind(identity_id)
    .bind(&req.persona_id)
    .bind(req.retain_forever)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    info!(session_id = %session_id, workflow = %workflow_str, "Session created");

    // Create initial jobs based on workflow type
    let job_ids = match req.workflow {
        WorkflowType::Chat => workflows::create_chat_jobs(pool, &session_id, &req.params).await?,
        WorkflowType::LoopN => {
            workflows::create_loop_n_jobs(pool, &session_id, &req.params).await?
        }
        WorkflowType::LoopUntilSentinel => {
            workflows::create_loop_until_sentinel_jobs(pool, &session_id, &req.params).await?
        }
        WorkflowType::Inbox => {
            workflows::create_inbox_jobs(pool, &session_id, &req.params).await?
        }
    };

    info!(
        session_id = %session_id,
        job_count = job_ids.len(),
        "Initial jobs created"
    );

    // Build web_url if WEB_UI_URL is configured
    let web_url = env::var("WEB_UI_URL")
        .ok()
        .filter(|u| !u.is_empty())
        .map(|base| format!("{}/sessions/{}", base.trim_end_matches('/'), session_id));

    Ok(CreateSessionResponse {
        session_id,
        status: SessionStatus::Pending,
        web_url,
    })
}

/// Build chat history from completed jobs in a session.
/// Returns (history, history_assistant, history_truncated).
pub async fn build_chat_history(
    pool: &PgPool,
    session_id: &str,
    max_turns: usize,
) -> Result<(Vec<String>, Vec<String>, bool), AppError> {
    // Query all completed jobs for the session, ordered by created_at
    let rows = sqlx::query_as::<_, (serde_json::Value, Option<String>)>(
        "SELECT task_input, assistant_reply FROM jobs
         WHERE session_id = $1 AND status = 'completed'
         ORDER BY created_at ASC",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    let mut history = Vec::new();
    let mut history_assistant = Vec::new();

    for (task_input, assistant_reply) in &rows {
        // Extract the user message: could be "message" (follow-up) or "prompt" (first job)
        let user_msg = task_input
            .get("message")
            .and_then(|v| v.as_str())
            .or_else(|| task_input.get("prompt").and_then(|v| v.as_str()))
            .unwrap_or("")
            .to_string();
        history.push(user_msg);

        let reply = assistant_reply.as_deref().unwrap_or("").to_string();
        history_assistant.push(reply);
    }

    let original_len = history.len();
    let truncated = original_len > max_turns;

    if truncated {
        // Take last N entries
        let skip = original_len - max_turns;
        history = history.into_iter().skip(skip).collect();
        history_assistant = history_assistant.into_iter().skip(skip).collect();
    }

    Ok((history, history_assistant, truncated))
}

/// Accept a follow-up message for a chat session. Creates a new job with history.
pub async fn send_input(
    pool: &PgPool,
    session_id: &str,
    message: &str,
    max_turns: usize,
    event_broadcaster: &Arc<EventBroadcaster>,
) -> Result<SendInputResponse, AppError> {
    // Validate: session exists
    let session_row: Option<(String, String, serde_json::Value)> = sqlx::query_as(
        "SELECT workflow, status, params FROM sessions WHERE id = $1",
    )
    .bind(session_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    let (workflow, status, params) = session_row.ok_or_else(|| {
        AppError::NotFound(format!("session '{}' not found", session_id))
    })?;

    // Must be a chat workflow
    if workflow != "chat" {
        return Err(AppError::Conflict(
            "follow-up messages are only supported for chat sessions".to_string(),
        ));
    }

    // Session must not be in a terminal state that prevents new input
    // Allow: pending, running, completed (completed means last job done, ready for follow-up)
    if status == "failed" {
        return Err(AppError::Conflict(
            "cannot send input to a failed session".to_string(),
        ));
    }

    // Check if there's a currently running/assigned/pending job (don't allow input while busy)
    let active_jobs = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM jobs WHERE session_id = $1 AND status IN ('pending', 'assigned', 'running')",
    )
    .bind(session_id)
    .fetch_one(pool)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    if active_jobs > 0 {
        return Err(AppError::Conflict(
            "session has active jobs; wait for completion before sending follow-up".to_string(),
        ));
    }

    // Build history from completed jobs
    let (history, history_assistant, history_truncated) =
        build_chat_history(pool, session_id, max_turns).await?;

    // Extract session_prompt from params
    let session_prompt = params
        .get("prompt")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // Determine the next iteration index
    let max_index: Option<i32> = sqlx::query_scalar(
        "SELECT MAX(iteration_index) FROM jobs WHERE session_id = $1",
    )
    .bind(session_id)
    .fetch_one(pool)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    let next_index = max_index.unwrap_or(-1) + 1;

    // Create new job with full history
    let job_id = Uuid::new_v4().to_string();
    let now = Utc::now();

    let task_input = serde_json::json!({
        "session_prompt": session_prompt,
        "message": message,
        "history": history,
        "history_assistant": history_assistant,
        "history_truncated": history_truncated,
    });

    sqlx::query(
        "INSERT INTO jobs (id, session_id, status, iteration_index, task_input, created_at, updated_at)
         VALUES ($1, $2, 'pending', $3, $4, $5, $5)",
    )
    .bind(&job_id)
    .bind(session_id)
    .bind(next_index)
    .bind(&task_input)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    // Update session status back to pending (so it gets picked up)
    sqlx::query("UPDATE sessions SET status = 'pending', updated_at = NOW() WHERE id = $1")
        .bind(session_id)
        .execute(pool)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    info!(
        session_id = %session_id,
        job_id = %job_id,
        iteration_index = next_index,
        history_len = history.len(),
        history_truncated = history_truncated,
        "Chat follow-up job created"
    );

    // Emit event
    event_broadcaster.send(SessionEvent {
        session_id: session_id.to_string(),
        event: "job_created".to_string(),
        job_id: Some(job_id),
        payload: None,
    });

    Ok(SendInputResponse { accepted: true })
}

/// Emit a session lifecycle event to the event broadcaster.
fn emit_event(
    event_broadcaster: &Arc<EventBroadcaster>,
    session_id: &str,
    event_name: &str,
    job_id: Option<&str>,
) {
    let event = SessionEvent {
        session_id: session_id.to_string(),
        event: event_name.to_string(),
        job_id: job_id.map(|s| s.to_string()),
        payload: None,
    };
    event_broadcaster.send(event);
}

/// Handle session state machine transitions after a task completes.
///
/// - chat: session status = job status
/// - loop_n: completed only when ALL jobs done; failed if any failed and none pending
/// - loop_until_sentinel: completed when sentinel reached; failed on failure; create next job otherwise
pub async fn handle_task_complete(
    pool: &PgPool,
    session_id: &str,
    job_id: &str,
    job_status: &str,
    iteration_index: i32,
    sentinel_reached: bool,
    event_broadcaster: &Arc<EventBroadcaster>,
) -> Result<(), AppError> {
    // Get the session workflow
    let session_row: Option<(String,)> =
        sqlx::query_as("SELECT workflow FROM sessions WHERE id = $1")
            .bind(session_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;

    let (workflow,) = session_row.ok_or_else(|| {
        AppError::NotFound(format!("session '{}' not found", session_id))
    })?;

    // Emit job completion/failure event
    match job_status {
        "completed" => emit_event(event_broadcaster, session_id, "job_completed", Some(job_id)),
        "failed" => emit_event(event_broadcaster, session_id, "job_failed", Some(job_id)),
        _ => {}
    }

    match workflow.as_str() {
        "chat" => {
            // Chat: session status mirrors the job status
            let session_status = match job_status {
                "completed" => "completed",
                "failed" => "failed",
                _ => "running",
            };
            sqlx::query("UPDATE sessions SET status = $1, updated_at = NOW() WHERE id = $2")
                .bind(session_status)
                .bind(session_id)
                .execute(pool)
                .await
                .map_err(|e| AppError::Internal(e.into()))?;

            // Emit session-level event
            match session_status {
                "completed" => emit_event(event_broadcaster, session_id, "completed", None),
                "failed" => emit_event(event_broadcaster, session_id, "failed", None),
                _ => {}
            }
        }
        "loop_n" => {
            // loop_n: check if all jobs are done
            let old_status = get_session_status(pool, session_id).await?;
            update_session_status(pool, session_id).await?;
            let new_status = get_session_status(pool, session_id).await?;

            if old_status != new_status {
                match new_status.as_str() {
                    "completed" => {
                        emit_event(event_broadcaster, session_id, "completed", None)
                    }
                    "failed" => emit_event(event_broadcaster, session_id, "failed", None),
                    _ => {}
                }
            }
        }
        "loop_until_sentinel" => {
            if job_status == "failed" {
                // Failed -> session failed
                sqlx::query(
                    "UPDATE sessions SET status = 'failed', updated_at = NOW() WHERE id = $1",
                )
                .bind(session_id)
                .execute(pool)
                .await
                .map_err(|e| AppError::Internal(e.into()))?;
                emit_event(event_broadcaster, session_id, "failed", None);
            } else if sentinel_reached {
                // Sentinel reached -> session completed
                sqlx::query(
                    "UPDATE sessions SET status = 'completed', updated_at = NOW() WHERE id = $1",
                )
                .bind(session_id)
                .execute(pool)
                .await
                .map_err(|e| AppError::Internal(e.into()))?;
                emit_event(event_broadcaster, session_id, "completed", None);
            } else {
                // Success but no sentinel -> create next job
                let params_row: Option<(serde_json::Value,)> =
                    sqlx::query_as("SELECT params FROM sessions WHERE id = $1")
                        .bind(session_id)
                        .fetch_optional(pool)
                        .await
                        .map_err(|e| AppError::Internal(e.into()))?;

                let params = params_row
                    .map(|(p,)| p)
                    .unwrap_or(serde_json::json!({}));

                let next_index = iteration_index + 1;
                let new_job_id = uuid::Uuid::new_v4().to_string();
                let task_input = serde_json::json!({
                    "prompt": params.get("prompt").cloned().unwrap_or(serde_json::Value::Null),
                    "sentinel": params.get("sentinel").cloned().unwrap_or(serde_json::Value::Null),
                    "iteration": next_index,
                });

                sqlx::query(
                    "INSERT INTO jobs (id, session_id, status, iteration_index, task_input, created_at, updated_at)
                     VALUES ($1, $2, 'pending', $3, $4, NOW(), NOW())"
                )
                .bind(&new_job_id)
                .bind(session_id)
                .bind(next_index)
                .bind(&task_input)
                .execute(pool)
                .await
                .map_err(|e| AppError::Internal(e.into()))?;

                info!(
                    session_id = %session_id,
                    new_job_id = %new_job_id,
                    iteration_index = next_index,
                    "Created next loop_until_sentinel job"
                );
            }
        }
        _ => {
            // For other workflows (inbox, etc.), use generic status update
            let old_status = get_session_status(pool, session_id).await?;
            update_session_status(pool, session_id).await?;
            let new_status = get_session_status(pool, session_id).await?;

            if old_status != new_status {
                match new_status.as_str() {
                    "completed" => {
                        emit_event(event_broadcaster, session_id, "completed", None)
                    }
                    "failed" => emit_event(event_broadcaster, session_id, "failed", None),
                    _ => {}
                }
            }
        }
    }

    Ok(())
}

/// Get the current session status from the database.
async fn get_session_status(pool: &PgPool, session_id: &str) -> Result<String, AppError> {
    let row: Option<(String,)> = sqlx::query_as("SELECT status FROM sessions WHERE id = $1")
        .bind(session_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(row.map(|(s,)| s).unwrap_or_else(|| "pending".to_string()))
}

/// Update session status based on its jobs' statuses.
/// Called when job statuses change (e.g., from worker task_complete).
pub async fn update_session_status(pool: &PgPool, session_id: &str) -> Result<(), AppError> {
    // Count jobs by status
    let rows = sqlx::query_as::<_, (String, i64)>(
        "SELECT status, COUNT(*) as cnt FROM jobs WHERE session_id = $1 GROUP BY status",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    let mut pending: i64 = 0;
    let mut assigned: i64 = 0;
    let mut running: i64 = 0;
    let mut completed: i64 = 0;
    let mut failed: i64 = 0;

    for (status, count) in &rows {
        match status.as_str() {
            "pending" => pending = *count,
            "assigned" => assigned = *count,
            "running" => running = *count,
            "completed" => completed = *count,
            "failed" => failed = *count,
            _ => {}
        }
    }

    let total = pending + assigned + running + completed + failed;

    let new_status = derive_session_status(pending, assigned, running, completed, failed, total);

    let now = Utc::now();
    sqlx::query("UPDATE sessions SET status = $1, updated_at = $2 WHERE id = $3")
        .bind(new_status)
        .bind(now)
        .bind(session_id)
        .execute(pool)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    Ok(())
}

/// Pure function: derive session status from job status counts.
/// Extracted for testability.
fn derive_session_status(
    pending: i64,
    assigned: i64,
    running: i64,
    completed: i64,
    failed: i64,
    total: i64,
) -> &'static str {
    if total == 0 {
        "pending"
    } else if running > 0 || assigned > 0 {
        "running"
    } else if completed == total {
        "completed"
    } else if failed > 0 && pending == 0 {
        "failed"
    } else {
        "pending"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_params_chat_missing_prompt() {
        let params = serde_json::json!({"agent_cli": "claude_code"});
        let result = validate_params(&WorkflowType::Chat, &params);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("prompt"));
    }

    #[test]
    fn test_validate_params_chat_missing_agent_cli() {
        let params = serde_json::json!({"prompt": "hello"});
        let result = validate_params(&WorkflowType::Chat, &params);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("agent_cli"));
    }

    #[test]
    fn test_validate_params_chat_valid() {
        let params = serde_json::json!({"prompt": "hello", "agent_cli": "claude_code"});
        let result = validate_params(&WorkflowType::Chat, &params);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_params_loop_n_missing_n() {
        let params = serde_json::json!({"prompt": "hello", "agent_cli": "claude_code"});
        let result = validate_params(&WorkflowType::LoopN, &params);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("n"));
    }

    #[test]
    fn test_validate_params_loop_n_valid() {
        let params = serde_json::json!({"prompt": "hello", "agent_cli": "claude_code", "n": 3});
        let result = validate_params(&WorkflowType::LoopN, &params);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_params_sentinel_missing() {
        let params = serde_json::json!({"prompt": "hello", "agent_cli": "claude_code"});
        let result = validate_params(&WorkflowType::LoopUntilSentinel, &params);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("sentinel"));
    }

    #[test]
    fn test_validate_params_sentinel_valid() {
        let params = serde_json::json!({"prompt": "hello", "agent_cli": "claude_code", "sentinel": "DONE"});
        let result = validate_params(&WorkflowType::LoopUntilSentinel, &params);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_params_inbox_valid() {
        let params = serde_json::json!({"agent_id": "a1", "agent_cli": "claude_code"});
        let result = validate_params(&WorkflowType::Inbox, &params);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_params_invalid_agent_cli() {
        let params = serde_json::json!({"prompt": "hello", "agent_cli": "invalid"});
        let result = validate_params(&WorkflowType::Chat, &params);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("agent_cli"));
    }

    // ─── Session status derivation tests (spec 004) ─────────────────────────

    #[test]
    fn test_derive_session_status_no_jobs() {
        assert_eq!(derive_session_status(0, 0, 0, 0, 0, 0), "pending");
    }

    #[test]
    fn test_derive_session_status_all_completed() {
        assert_eq!(derive_session_status(0, 0, 0, 3, 0, 3), "completed");
    }

    #[test]
    fn test_derive_session_status_running() {
        assert_eq!(derive_session_status(1, 0, 1, 0, 0, 2), "running");
    }

    #[test]
    fn test_derive_session_status_assigned_is_running() {
        assert_eq!(derive_session_status(1, 1, 0, 0, 0, 2), "running");
    }

    #[test]
    fn test_derive_session_status_failed_no_pending() {
        assert_eq!(derive_session_status(0, 0, 0, 2, 1, 3), "failed");
    }

    #[test]
    fn test_derive_session_status_failed_with_pending_stays_pending() {
        // Some failed but still pending jobs → pending (not failed yet)
        assert_eq!(derive_session_status(1, 0, 0, 0, 1, 2), "pending");
    }

    #[test]
    fn test_derive_session_status_mixed_completed_and_running() {
        assert_eq!(derive_session_status(0, 0, 1, 2, 0, 3), "running");
    }

    #[test]
    fn test_pull_task_request_deserialization_empty() {
        let json = "{}";
        let req: api_types::PullTaskRequest = serde_json::from_str(json).unwrap();
        assert!(req.worker_id.is_none());
    }

    #[test]
    fn test_pull_task_request_deserialization_with_worker() {
        let json = r#"{"worker_id": "w1"}"#;
        let req: api_types::PullTaskRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.worker_id.as_deref(), Some("w1"));
    }

    #[test]
    fn test_task_complete_request_success() {
        let json = r#"{"status": "success", "branch": "feature/x", "sentinel_reached": true}"#;
        let req: api_types::TaskCompleteRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.status, api_types::TaskCompleteStatus::Success);
        assert_eq!(req.branch.as_deref(), Some("feature/x"));
        assert_eq!(req.sentinel_reached, Some(true));
    }

    #[test]
    fn test_task_complete_request_failed() {
        let json = r#"{"status": "failed", "error_message": "compile error"}"#;
        let req: api_types::TaskCompleteRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.status, api_types::TaskCompleteStatus::Failed);
        assert_eq!(req.error_message.as_deref(), Some("compile error"));
    }

    #[test]
    fn test_build_chat_history_capping() {
        // Simulate the capping logic (extracted from build_chat_history)
        let max_turns = 3;
        let mut history: Vec<String> = vec![
            "msg1".into(),
            "msg2".into(),
            "msg3".into(),
            "msg4".into(),
            "msg5".into(),
        ];
        let mut history_assistant: Vec<String> = vec![
            "reply1".into(),
            "reply2".into(),
            "reply3".into(),
            "reply4".into(),
            "reply5".into(),
        ];

        let original_len = history.len();
        let truncated = original_len > max_turns;
        assert!(truncated);

        if truncated {
            let skip = original_len - max_turns;
            history = history.into_iter().skip(skip).collect();
            history_assistant = history_assistant.into_iter().skip(skip).collect();
        }

        assert_eq!(history.len(), 3);
        assert_eq!(history[0], "msg3");
        assert_eq!(history[1], "msg4");
        assert_eq!(history[2], "msg5");
        assert_eq!(history_assistant[0], "reply3");
        assert_eq!(history_assistant[2], "reply5");
    }

    #[test]
    fn test_build_chat_history_no_capping() {
        let max_turns = 50;
        let history: Vec<String> = vec!["msg1".into(), "msg2".into()];
        let truncated = history.len() > max_turns;
        assert!(!truncated);
        assert_eq!(history.len(), 2);
    }

    #[test]
    fn test_chat_task_input_construction() {
        let session_prompt = "Fix the tests";
        let message = "Also fix the linter warnings";
        let history = vec!["Fix the tests".to_string()];
        let history_assistant = vec!["I fixed 3 tests".to_string()];
        let history_truncated = false;

        let task_input = serde_json::json!({
            "session_prompt": session_prompt,
            "message": message,
            "history": history,
            "history_assistant": history_assistant,
            "history_truncated": history_truncated,
        });

        assert_eq!(task_input["session_prompt"], "Fix the tests");
        assert_eq!(task_input["message"], "Also fix the linter warnings");
        assert_eq!(task_input["history"].as_array().unwrap().len(), 1);
        assert_eq!(task_input["history_assistant"].as_array().unwrap().len(), 1);
        assert!(!task_input["history_truncated"].as_bool().unwrap());
    }

    #[test]
    fn test_credential_resolution_params_override_identity() {
        // Test the credential merging logic used in pull_task
        let params = serde_json::json!({"agent_token": "param_token", "git_token": ""});
        let id_agent_token: Option<String> = Some("identity_agent".to_string());
        let id_git_token: Option<String> = Some("identity_git".to_string());

        // params override identity for agent_token (non-empty)
        let agent_token = params
            .get("agent_token")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .or(id_agent_token);
        assert_eq!(agent_token.as_deref(), Some("param_token"));

        // params git_token is empty, so identity wins
        let git_token = params
            .get("git_token")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .or(id_git_token);
        assert_eq!(git_token.as_deref(), Some("identity_git"));
    }
}
