use sqlx::PgPool;
use tokio::sync::broadcast;

use crate::error::AppError;
use crate::sse::SessionEvent;

/// Valid job states: pending -> assigned -> running -> completed | failed
const VALID_TRANSITIONS: &[(&str, &str)] = &[
    ("pending", "assigned"),
    ("assigned", "running"),
    ("running", "completed"),
    ("running", "failed"),
    // Allow direct pending -> running (when mark_running called right after assign)
    ("assigned", "completed"),
    ("assigned", "failed"),
    // Allow pending -> failed for immediate failures
    ("pending", "failed"),
];

fn is_valid_transition(from: &str, to: &str) -> bool {
    VALID_TRANSITIONS
        .iter()
        .any(|(f, t)| *f == from && *t == to)
}

/// Publish a session event to the broadcast channel.
pub fn publish_session_event(
    event_tx: &broadcast::Sender<SessionEvent>,
    session_id: &str,
    event: &str,
    job_id: Option<&str>,
    payload: serde_json::Value,
) {
    let evt = SessionEvent {
        session_id: session_id.to_string(),
        event: event.to_string(),
        job_id: job_id.map(|s| s.to_string()),
        payload,
    };
    let _ = event_tx.send(evt);
}

/// Assign a job to a worker. Transitions pending -> assigned.
pub async fn assign_to_worker(pool: &PgPool, job_id: &str, worker_id: &str) -> Result<(), AppError> {
    let current_status: Option<String> =
        sqlx::query_scalar("SELECT status FROM jobs WHERE id = $1::uuid")
            .bind(job_id)
            .fetch_optional(pool)
            .await?;

    let current = current_status.ok_or_else(|| AppError::not_found("Job not found"))?;

    if !is_valid_transition(&current, "assigned") {
        return Err(AppError::conflict(format!(
            "Cannot transition job from '{}' to 'assigned'",
            current
        )));
    }

    sqlx::query(
        "UPDATE jobs SET status = 'assigned', worker_id = $2, assigned_at = now(), updated_at = now() WHERE id = $1::uuid",
    )
    .bind(job_id)
    .bind(worker_id)
    .execute(pool)
    .await?;

    // Log control plane event
    let session_id: Option<String> =
        sqlx::query_scalar("SELECT session_id::text FROM jobs WHERE id = $1::uuid")
            .bind(job_id)
            .fetch_optional(pool)
            .await?;
    if let Some(sid) = session_id {
        crate::routes::logs::insert_control_plane_log(
            pool,
            &sid,
            Some(job_id),
            "info",
            &format!("Job assigned to worker {}", worker_id),
        )
        .await;
    }

    Ok(())
}

/// Mark a job as running. Transitions assigned -> running.
pub async fn mark_running(pool: &PgPool, job_id: &str) -> Result<(), AppError> {
    let current_status: Option<String> =
        sqlx::query_scalar("SELECT status FROM jobs WHERE id = $1::uuid")
            .bind(job_id)
            .fetch_optional(pool)
            .await?;

    let current = current_status.ok_or_else(|| AppError::not_found("Job not found"))?;

    if !is_valid_transition(&current, "running") {
        return Err(AppError::conflict(format!(
            "Cannot transition job from '{}' to 'running'",
            current
        )));
    }

    sqlx::query("UPDATE jobs SET status = 'running', updated_at = now() WHERE id = $1::uuid")
        .bind(job_id)
        .execute(pool)
        .await?;

    Ok(())
}

pub struct CompleteJobParams<'a> {
    pub job_id: &'a str,
    pub success: bool,
    pub error_message: Option<&'a str>,
    pub branch: Option<&'a str>,
    pub commit_ref: Option<&'a str>,
    pub pull_request_url: Option<&'a str>,
    pub output: Option<&'a str>,
    pub sentinel_reached: Option<bool>,
    pub assistant_reply: Option<&'a str>,
}

/// Complete a job (success or failure). Stores output fields.
pub async fn complete_job(pool: &PgPool, params: CompleteJobParams<'_>) -> Result<(), AppError> {
    let job_id = params.job_id;
    let success = params.success;
    let current_status: Option<String> =
        sqlx::query_scalar("SELECT status FROM jobs WHERE id = $1::uuid")
            .bind(job_id)
            .fetch_optional(pool)
            .await?;

    let current = current_status.ok_or_else(|| AppError::not_found("Job not found"))?;

    let target = if success { "completed" } else { "failed" };

    if !is_valid_transition(&current, target) {
        return Err(AppError::conflict(format!(
            "Cannot transition job from '{}' to '{}'",
            current, target
        )));
    }

    sqlx::query(
        r#"
        UPDATE jobs SET
            status = $2,
            error_message = $3,
            branch = $4,
            commit_ref = $5,
            pull_request_url = $6,
            output = $7,
            sentinel_reached = $8,
            assistant_reply = $9,
            updated_at = now()
        WHERE id = $1::uuid
        "#,
    )
    .bind(job_id)
    .bind(target)
    .bind(params.error_message)
    .bind(params.branch)
    .bind(params.commit_ref)
    .bind(params.pull_request_url)
    .bind(params.output)
    .bind(params.sentinel_reached.unwrap_or(false))
    .bind(params.assistant_reply)
    .execute(pool)
    .await?;

    // Log control plane event
    let session_id: Option<String> =
        sqlx::query_scalar("SELECT session_id::text FROM jobs WHERE id = $1::uuid")
            .bind(job_id)
            .fetch_optional(pool)
            .await?;
    if let Some(sid) = session_id {
        let msg = if success {
            format!("Job {} completed successfully", job_id)
        } else {
            format!(
                "Job {} failed: {}",
                job_id,
                params.error_message.unwrap_or("unknown error")
            )
        };
        crate::routes::logs::insert_control_plane_log(pool, &sid, Some(job_id), "info", &msg)
            .await;
    }

    Ok(())
}

/// Derive session status from its jobs.
/// - pending: all jobs pending
/// - running: any job assigned or running
/// - completed: all jobs completed
/// - failed: any job failed and none running/assigned
pub async fn derive_session_status(pool: &PgPool, session_id: &str) -> Result<String, AppError> {
    let statuses: Vec<String> =
        sqlx::query_scalar("SELECT status FROM jobs WHERE session_id = $1::uuid")
            .bind(session_id)
            .fetch_all(pool)
            .await?;

    if statuses.is_empty() {
        return Ok("pending".to_string());
    }

    let has_running = statuses
        .iter()
        .any(|s| s == "running" || s == "assigned");
    let has_failed = statuses.iter().any(|s| s == "failed");
    let all_completed = statuses.iter().all(|s| s == "completed");
    let all_pending = statuses.iter().all(|s| s == "pending");

    if has_running {
        Ok("running".to_string())
    } else if all_completed {
        Ok("completed".to_string())
    } else if has_failed {
        Ok("failed".to_string())
    } else if all_pending {
        Ok("pending".to_string())
    } else {
        // Mixed states (some completed, some pending) — still running conceptually
        Ok("running".to_string())
    }
}

/// Update session status based on current job statuses.
pub async fn update_session_status(pool: &PgPool, session_id: &str) -> Result<(), AppError> {
    let status = derive_session_status(pool, session_id).await?;

    sqlx::query("UPDATE sessions SET status = $2, updated_at = now() WHERE id = $1::uuid")
        .bind(session_id)
        .bind(&status)
        .execute(pool)
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_transitions() {
        assert!(is_valid_transition("pending", "assigned"));
        assert!(is_valid_transition("assigned", "running"));
        assert!(is_valid_transition("running", "completed"));
        assert!(is_valid_transition("running", "failed"));
        assert!(is_valid_transition("assigned", "completed"));
        assert!(is_valid_transition("assigned", "failed"));
        assert!(is_valid_transition("pending", "failed"));
    }

    #[test]
    fn test_invalid_transitions() {
        assert!(!is_valid_transition("completed", "running"));
        assert!(!is_valid_transition("failed", "running"));
        assert!(!is_valid_transition("completed", "pending"));
        assert!(!is_valid_transition("running", "pending"));
        assert!(!is_valid_transition("running", "assigned"));
        assert!(!is_valid_transition("pending", "completed"));
    }
}
