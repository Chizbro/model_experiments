use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use api_types::{
    AgentCli, BranchMode, JobId, PullTaskResponse, SessionParams, TaskCompleteRequest,
    TaskCompleteStatus, TaskId, TaskInput, WorkflowType,
};

use crate::engine;
use crate::engine::jobs;
use crate::error::AppError;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct PullTaskRequest {
    pub worker_id: Option<String>,
}

/// POST /workers/tasks/pull
pub async fn pull_task(
    State(state): State<AppState>,
    Json(body): Json<PullTaskRequest>,
) -> Result<impl IntoResponse, AppError> {
    let worker_id = body
        .worker_id
        .as_deref()
        .ok_or_else(|| AppError::bad_request("worker_id is required"))?;

    // Verify worker exists
    let worker_exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM workers WHERE id = $1)")
            .bind(worker_id)
            .fetch_one(&state.pool)
            .await?;

    if !worker_exists {
        return Err(AppError::not_found("Worker not found"));
    }

    let stale_cutoff = chrono::Utc::now()
        - chrono::Duration::seconds(state.config.worker_stale_seconds as i64);
    let max_reclaims = state.config.max_job_reclaims as i32;

    // Step 1: Reclaim jobs from stale workers (under reclaim cap)
    let reclaimed = sqlx::query(
        r#"
        UPDATE jobs SET worker_id = NULL, status = 'pending', reclaim_count = reclaim_count + 1, updated_at = now()
        WHERE status = 'assigned' AND worker_id IN (SELECT id FROM workers WHERE last_seen_at < $1)
        AND reclaim_count < $2
        "#,
    )
    .bind(stale_cutoff)
    .bind(max_reclaims)
    .execute(&state.pool)
    .await?;

    if reclaimed.rows_affected() > 0 {
        tracing::info!(count = reclaimed.rows_affected(), "Reclaimed jobs from stale workers");
    }

    // Step 1b: Fail jobs over reclaim cap
    let failed_reclaim = sqlx::query(
        r#"
        UPDATE jobs SET status = 'failed', error_message = '[MAX_WORKER_LOSS_RETRIES]', updated_at = now()
        WHERE status = 'assigned' AND worker_id IN (SELECT id FROM workers WHERE last_seen_at < $1)
        AND reclaim_count >= $2
        "#,
    )
    .bind(stale_cutoff)
    .bind(max_reclaims)
    .execute(&state.pool)
    .await?;

    if failed_reclaim.rows_affected() > 0 {
        tracing::warn!(
            count = failed_reclaim.rows_affected(),
            "Failed jobs exceeding max reclaim count"
        );
        // Update session statuses for affected jobs
        let affected_sessions: Vec<String> = sqlx::query_scalar(
            "SELECT DISTINCT session_id::text FROM jobs WHERE status = 'failed' AND error_message = '[MAX_WORKER_LOSS_RETRIES]' AND updated_at > now() - interval '5 seconds'",
        )
        .fetch_all(&state.pool)
        .await?;

        for sid in &affected_sessions {
            let _ = jobs::update_session_status(&state.pool, sid).await;
        }
    }

    // Step 1c: Lease expiry (when job_lease_seconds > 0)
    if state.config.job_lease_seconds > 0 {
        let lease_cutoff = chrono::Utc::now()
            - chrono::Duration::seconds(state.config.job_lease_seconds as i64);

        let failed_lease = sqlx::query(
            r#"
            UPDATE jobs SET status = 'failed', error_message = '[JOB_LEASE_EXPIRED]', updated_at = now()
            WHERE status = 'assigned' AND assigned_at < $1
            "#,
        )
        .bind(lease_cutoff)
        .execute(&state.pool)
        .await?;

        if failed_lease.rows_affected() > 0 {
            tracing::warn!(
                count = failed_lease.rows_affected(),
                "Failed jobs due to lease expiry"
            );
            let affected_sessions: Vec<String> = sqlx::query_scalar(
                "SELECT DISTINCT session_id::text FROM jobs WHERE status = 'failed' AND error_message = '[JOB_LEASE_EXPIRED]' AND updated_at > now() - interval '5 seconds'",
            )
            .fetch_all(&state.pool)
            .await?;

            for sid in &affected_sessions {
                let _ = jobs::update_session_status(&state.pool, sid).await;
            }
        }
    }

    // Step 2: Select one pending job and assign to worker
    let job_row = sqlx::query_as::<_, (String, String, serde_json::Value)>(
        r#"
        SELECT j.id::text, j.session_id::text, j.task_input
        FROM jobs j
        WHERE j.status = 'pending'
        ORDER BY j.created_at ASC
        LIMIT 1
        FOR UPDATE SKIP LOCKED
        "#,
    )
    .fetch_optional(&state.pool)
    .await?;

    let (job_id_str, session_id_str, task_input_json) = match job_row {
        Some(row) => row,
        None => return Ok(StatusCode::NO_CONTENT.into_response()),
    };

    // Assign the job
    jobs::assign_to_worker(&state.pool, &job_id_str, worker_id).await?;

    // Update session status to running
    jobs::update_session_status(&state.pool, &session_id_str).await?;

    // Publish session events: job_started and (if session just became running) started
    jobs::publish_session_event(
        &state.event_tx,
        &session_id_str,
        "job_started",
        Some(&job_id_str),
        json!({ "worker_id": worker_id }),
    );
    // Check if session just transitioned to running
    let new_status = jobs::derive_session_status(&state.pool, &session_id_str).await?;
    if new_status == "running" {
        jobs::publish_session_event(
            &state.event_tx,
            &session_id_str,
            "started",
            None,
            json!({}),
        );
    }

    // Step 3: Build full pull response
    // Fetch session details
    let session_row = sqlx::query_as::<_, (String, Option<String>, String, serde_json::Value, Option<String>, Option<String>)>(
        r#"SELECT repo_url, ref, workflow, params, persona_id::text, identity_id FROM sessions WHERE id = $1::uuid"#,
    )
    .bind(&session_id_str)
    .fetch_one(&state.pool)
    .await?;

    let (repo_url, ref_, workflow_str, params_json, persona_id, identity_id) = session_row;

    // Resolve credentials from identity
    let identity_id_str = identity_id.as_deref().unwrap_or("default");
    let cred_row = sqlx::query_as::<_, (Option<String>, Option<String>)>(
        "SELECT git_token, agent_token FROM identities WHERE id = $1",
    )
    .bind(identity_id_str)
    .fetch_optional(&state.pool)
    .await?;

    let (git_token, agent_token) = cred_row.unwrap_or((None, None));

    // Resolve persona prompt_context
    let prompt_context: Option<String> = if let Some(ref pid) = persona_id {
        sqlx::query_scalar("SELECT prompt FROM personas WHERE id = $1::uuid")
            .bind(pid)
            .fetch_optional(&state.pool)
            .await?
    } else {
        None
    };

    // Parse types
    let workflow: WorkflowType =
        serde_json::from_value(json!(workflow_str)).unwrap_or(WorkflowType::Chat);
    let params: Option<SessionParams> = serde_json::from_value(params_json.clone()).ok();

    // Parse task_input from JSON to TaskInput enum
    let input: TaskInput = serde_json::from_value(task_input_json.clone())
        .map_err(|e| AppError::internal(format!("Failed to parse task_input: {}", e)))?;

    // Extract agent_cli, model, branch_mode from params
    let agent_cli: Option<AgentCli> = params.as_ref().and_then(|p| p.agent_cli.clone());
    let model: Option<String> = params.as_ref().and_then(|p| p.model.clone());
    let branch_mode: Option<BranchMode> = params.as_ref().and_then(|p| p.branch_mode.clone());

    let task_id = TaskId::new();
    let job_id = JobId::from_string(&job_id_str);

    let response = PullTaskResponse {
        task_id,
        session_id: api_types::SessionId::from_string(&session_id_str),
        job_id,
        repo_url,
        ref_,
        workflow,
        params,
        input,
        git_token,
        agent_token,
        agent_cli,
        model,
        branch_mode,
        prompt_context,
    };

    Ok(Json(response).into_response())
}

/// POST /workers/tasks/:id/complete
pub async fn complete_task(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    Json(body): Json<TaskCompleteRequest>,
) -> Result<impl IntoResponse, AppError> {
    // The task_id in the URL is the job_id (task_id is ephemeral, job_id is the DB key)
    // We look up by job_id since that's what's stored
    let job_row = sqlx::query_as::<_, (String, String, Option<String>)>(
        "SELECT id::text, session_id::text, worker_id FROM jobs WHERE id = $1::uuid",
    )
    .bind(&task_id)
    .fetch_optional(&state.pool)
    .await?;

    let (job_id_str, session_id_str, assigned_worker_id) = match job_row {
        Some(row) => row,
        None => return Err(AppError::not_found("Task not found or already completed")),
    };

    // Validate the task is assigned to the reporting worker
    let assigned = assigned_worker_id.as_deref().unwrap_or("");
    if assigned != body.worker_id.as_str() {
        return Err(AppError::forbidden(
            "Task is not assigned to the reporting worker",
        ));
    }

    // Check current status is assigned or running
    let current_status: String =
        sqlx::query_scalar("SELECT status FROM jobs WHERE id = $1::uuid")
            .bind(&job_id_str)
            .fetch_one(&state.pool)
            .await?;

    if current_status != "assigned" && current_status != "running" {
        return Err(AppError::not_found("Task not found or already completed"));
    }

    let success = body.status == TaskCompleteStatus::Completed;

    // Update job via engine
    jobs::complete_job(
        &state.pool,
        jobs::CompleteJobParams {
            job_id: &job_id_str,
            success,
            error_message: body.error_message.as_deref(),
            branch: body.branch.as_deref(),
            commit_ref: body.commit_ref.as_deref(),
            pull_request_url: None, // PR creation deferred to Task 19
            output: body.output.as_deref(),
            sentinel_reached: body.sentinel_reached,
            assistant_reply: body.assistant_reply.as_deref(),
        },
    )
    .await?;

    // PR/MR creation hook
    if success && body.branch.is_some() {
        let session_row = sqlx::query_as::<_, (String, serde_json::Value, Option<String>)>(
            "SELECT repo_url, params, identity_id FROM sessions WHERE id = $1::uuid",
        )
        .bind(&session_id_str)
        .fetch_optional(&state.pool)
        .await?;

        if let Some((repo_url, params_json, identity_id)) = session_row {
            let branch_mode = params_json
                .get("branch_mode")
                .and_then(|v| v.as_str());

            let branch = body.branch.as_deref().unwrap_or("");
            let mr_title = body.mr_title.as_deref();

            if branch_mode == Some("pr") && !branch.is_empty() && mr_title.is_some_and(|t| !t.is_empty()) {
                let identity_id_str = identity_id.as_deref().unwrap_or("default");
                let http_client = reqwest::Client::new();

                match crate::services::pr_service::create_pr_for_job(
                    crate::services::pr_service::CreatePrParams {
                        pool: &state.pool,
                        config: &state.config,
                        http_client: &http_client,
                        job_id: &job_id_str,
                        session_id: &session_id_str,
                        repo_url: &repo_url,
                        branch,
                        mr_title,
                        mr_description: body.mr_description.as_deref(),
                        identity_id: identity_id_str,
                    },
                )
                .await
                {
                    Ok(pr_url) => {
                        tracing::info!(
                            job_id = %job_id_str,
                            session_id = %session_id_str,
                            pr_url = %pr_url,
                            "PR/MR created successfully"
                        );
                    }
                    Err(e) => {
                        // PR failure is non-blocking — job already completed
                        tracing::warn!(
                            job_id = %job_id_str,
                            session_id = %session_id_str,
                            error = %e,
                            "PR/MR creation failed"
                        );
                        crate::routes::logs::insert_control_plane_log(
                            &state.pool,
                            &session_id_str,
                            Some(&job_id_str),
                            "warn",
                            &format!("PR/MR creation failed: {}", e),
                        )
                        .await;
                    }
                }
            }
        }
    }

    // Publish job_completed or job_failed event
    let job_event = if success { "job_completed" } else { "job_failed" };
    jobs::publish_session_event(
        &state.event_tx,
        &session_id_str,
        job_event,
        Some(&job_id_str),
        json!({}),
    );

    // Handle workflow-specific follow-up logic
    if success {
        engine::on_job_completed(&state.pool, &session_id_str, &job_id_str).await?;
    } else {
        jobs::update_session_status(&state.pool, &session_id_str).await?;
    }

    // Check if session reached a terminal state and publish event
    let final_status = jobs::derive_session_status(&state.pool, &session_id_str).await?;
    if final_status == "completed" || final_status == "failed" {
        jobs::publish_session_event(
            &state.event_tx,
            &session_id_str,
            &final_status,
            None,
            json!({}),
        );
    }

    Ok(Json(json!({ "ok": true })))
}
