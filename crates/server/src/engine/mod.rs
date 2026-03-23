pub mod chat;
pub mod jobs;

use api_types::{JobId, SessionId, WorkflowType};
use serde_json::json;
use sqlx::PgPool;

use crate::error::AppError;

/// Create initial jobs for a session based on its workflow type.
pub async fn create_jobs_for_session(
    pool: &PgPool,
    session_id: &SessionId,
    workflow: &WorkflowType,
    prompt: Option<&str>,
    n: Option<u32>,
) -> Result<Vec<JobId>, AppError> {
    let mut job_ids = Vec::new();

    match workflow {
        WorkflowType::Chat => {
            let job_id = JobId::new();
            let task_input = json!({
                "chat_first": {
                    "prompt": prompt.unwrap_or("")
                }
            });
            sqlx::query(
                "INSERT INTO jobs (id, session_id, status, task_input) VALUES ($1::uuid, $2::uuid, 'pending', $3)",
            )
            .bind(job_id.as_str())
            .bind(session_id.as_str())
            .bind(&task_input)
            .execute(pool)
            .await?;
            job_ids.push(job_id);
        }
        WorkflowType::LoopN => {
            let count = n.unwrap_or(1);
            for i in 0..count {
                let job_id = JobId::new();
                let task_input = json!({
                    "loop": {
                        "prompt": prompt.unwrap_or(""),
                        "iteration": i
                    }
                });
                sqlx::query(
                    "INSERT INTO jobs (id, session_id, status, task_input) VALUES ($1::uuid, $2::uuid, 'pending', $3)",
                )
                .bind(job_id.as_str())
                .bind(session_id.as_str())
                .bind(&task_input)
                .execute(pool)
                .await?;
                job_ids.push(job_id);
            }
        }
        WorkflowType::LoopUntilSentinel => {
            let job_id = JobId::new();
            let task_input = json!({
                "loop": {
                    "prompt": prompt.unwrap_or(""),
                    "iteration": 0
                }
            });
            sqlx::query(
                "INSERT INTO jobs (id, session_id, status, task_input) VALUES ($1::uuid, $2::uuid, 'pending', $3)",
            )
            .bind(job_id.as_str())
            .bind(session_id.as_str())
            .bind(&task_input)
            .execute(pool)
            .await?;
            job_ids.push(job_id);
        }
        WorkflowType::Inbox => {
            // No immediate jobs for inbox — jobs come from inbox messages
        }
    }

    // Log control plane event
    crate::routes::logs::insert_control_plane_log(
        pool,
        session_id.as_str(),
        None,
        "info",
        &format!(
            "Session created with workflow {:?}, {} job(s)",
            workflow,
            job_ids.len()
        ),
    )
    .await;

    Ok(job_ids)
}

/// Handle job completion for workflows that need follow-up logic.
/// For loop_until_sentinel: if sentinel not reached, create next iteration job.
pub async fn on_job_completed(
    pool: &PgPool,
    session_id: &str,
    job_id: &str,
) -> Result<(), AppError> {
    // Fetch session workflow type and sentinel
    let row = sqlx::query_as::<_, (String, serde_json::Value)>(
        "SELECT workflow, params FROM sessions WHERE id = $1::uuid",
    )
    .bind(session_id)
    .fetch_optional(pool)
    .await?;

    let (workflow_str, params) = match row {
        Some(r) => r,
        None => return Ok(()),
    };

    // Chat sessions stay "running" after job completion — user can keep sending input.
    // Only go to "pending" if there are pending jobs waiting.
    if workflow_str == "chat" {
        return chat::update_chat_session_status(pool, session_id).await;
    }

    if workflow_str != "loop_until_sentinel" {
        // Update session status and return
        jobs::update_session_status(pool, session_id).await?;
        return Ok(());
    }

    // Check if sentinel was reached on this job.
    // Server double-checks: worker flag OR output contains sentinel substring.
    let job_row: Option<(Option<bool>, Option<String>)> =
        sqlx::query_as("SELECT sentinel_reached, output FROM jobs WHERE id = $1::uuid")
            .bind(job_id)
            .fetch_optional(pool)
            .await?;

    let (sentinel_flag, output) = job_row.unwrap_or((None, None));

    let sentinel_str = params
        .get("sentinel")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let sentinel_detected = sentinel_flag.unwrap_or(false)
        || (!sentinel_str.is_empty()
            && matches!(output.as_deref(), Some(o) if o.contains(sentinel_str)));

    if sentinel_detected {
        // Sentinel reached — session is done
        jobs::update_session_status(pool, session_id).await?;
        return Ok(());
    }

    // Create next iteration job
    let current_iteration: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM jobs WHERE session_id = $1::uuid",
    )
    .bind(session_id)
    .fetch_one(pool)
    .await?;

    let prompt = params
        .get("prompt")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let new_job_id = JobId::new();
    let task_input = json!({
        "loop": {
            "prompt": prompt,
            "iteration": current_iteration
        }
    });

    sqlx::query(
        "INSERT INTO jobs (id, session_id, status, task_input) VALUES ($1::uuid, $2::uuid, 'pending', $3)",
    )
    .bind(new_job_id.as_str())
    .bind(session_id)
    .bind(&task_input)
    .execute(pool)
    .await?;

    // Session stays running
    jobs::update_session_status(pool, session_id).await?;

    Ok(())
}
