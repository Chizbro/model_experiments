use std::convert::Infallible;
use std::pin::Pin;
use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive};
use axum::response::{IntoResponse, Sse};
use axum::Json;
use chrono::{DateTime, Utc};
use futures::stream::Stream;
use serde::Deserialize;
use serde_json::json;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use api_types::{
    CreateSessionRequest, JobId, JobSummary, PaginatedResponse, SendInputRequest, SessionDetail,
    SessionId, SessionParams, SessionStatus, SessionSummary, WorkflowType,
};

use crate::engine;
use crate::error::AppError;
use crate::state::AppState;

/// POST /sessions
pub async fn create_session(
    State(state): State<AppState>,
    Json(body): Json<CreateSessionRequest>,
) -> Result<impl IntoResponse, AppError> {
    // Validate repo_url
    if body.repo_url.is_empty() {
        return Err(AppError::bad_request("repo_url is required"));
    }

    // Validate workflow-specific params
    let params = body.params.as_ref();
    match &body.workflow {
        WorkflowType::Chat => {
            if params.and_then(|p| p.prompt.as_ref()).is_none() {
                return Err(AppError::bad_request(
                    "params.prompt is required for chat workflow",
                ));
            }
        }
        WorkflowType::LoopN => {
            if params.and_then(|p| p.prompt.as_ref()).is_none() {
                return Err(AppError::bad_request(
                    "params.prompt is required for loop_n workflow",
                ));
            }
            if params.and_then(|p| p.n).is_none() {
                return Err(AppError::bad_request(
                    "params.n is required for loop_n workflow",
                ));
            }
        }
        WorkflowType::LoopUntilSentinel => {
            if params.and_then(|p| p.prompt.as_ref()).is_none() {
                return Err(AppError::bad_request(
                    "params.prompt is required for loop_until_sentinel workflow",
                ));
            }
            if params.and_then(|p| p.sentinel.as_ref()).is_none() {
                return Err(AppError::bad_request(
                    "params.sentinel is required for loop_until_sentinel workflow",
                ));
            }
        }
        WorkflowType::Inbox => {}
    }

    // Validate credentials: identity must have both agent_token and git_token
    let identity_id = body
        .identity_id
        .as_ref()
        .map(|id| id.as_str())
        .unwrap_or("default");

    let cred_row = sqlx::query_as::<_, (Option<String>, Option<String>)>(
        "SELECT agent_token, git_token FROM identities WHERE id = $1",
    )
    .bind(identity_id)
    .fetch_optional(&state.pool)
    .await?;

    match cred_row {
        None => {
            return Err(AppError::bad_request(format!(
                "Identity '{}' not found",
                identity_id
            )));
        }
        Some((agent_token, git_token)) => {
            if agent_token.is_none() || git_token.is_none() {
                return Err(AppError::bad_request(
                    "Identity must have both agent_token and git_token configured",
                ));
            }
        }
    }

    // Create session
    let session_id = SessionId::new();
    let ref_ = body.ref_.as_deref().unwrap_or("main");
    let workflow_str = serde_json::to_value(&body.workflow)
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();
    let params_json = serde_json::to_value(&body.params).unwrap_or(json!({}));
    let retain_forever = body.retain_forever.unwrap_or(false);
    let persona_id_str = body.persona_id.as_ref().map(|p| p.as_str().to_string());

    sqlx::query(
        r#"
        INSERT INTO sessions (id, repo_url, ref, workflow, params, persona_id, identity_id, status, retain_forever)
        VALUES ($1::uuid, $2, $3, $4, $5, $6::uuid, $7, 'pending', $8)
        "#,
    )
    .bind(session_id.as_str())
    .bind(&body.repo_url)
    .bind(ref_)
    .bind(&workflow_str)
    .bind(&params_json)
    .bind(&persona_id_str)
    .bind(identity_id)
    .bind(retain_forever)
    .execute(&state.pool)
    .await?;

    // Create initial jobs
    let prompt = params.and_then(|p| p.prompt.as_deref());
    let n = params.and_then(|p| p.n);
    engine::create_jobs_for_session(&state.pool, &session_id, &body.workflow, prompt, n).await?;

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "session_id": session_id,
            "status": "pending",
            "web_url": format!("/sessions/{}", session_id)
        })),
    ))
}

#[derive(Debug, Deserialize)]
pub struct ListSessionsQuery {
    pub cursor: Option<String>,
    pub limit: Option<i64>,
    pub status: Option<String>,
}

/// GET /sessions
pub async fn list_sessions(
    State(state): State<AppState>,
    Query(query): Query<ListSessionsQuery>,
) -> Result<impl IntoResponse, AppError> {
    let limit = query.limit.unwrap_or(50).min(100);

    let rows = if let Some(ref status) = query.status {
        if let Some(ref cursor) = query.cursor {
            let cursor_time: DateTime<Utc> = cursor
                .parse()
                .map_err(|_| AppError::bad_request("Invalid cursor"))?;
            sqlx::query_as::<_, (String, String, String, String, DateTime<Utc>, Option<DateTime<Utc>>, bool)>(
                "SELECT id, repo_url, workflow, status, created_at, updated_at, retain_forever FROM sessions WHERE status = $1 AND created_at < $2 ORDER BY created_at DESC LIMIT $3",
            )
            .bind(status)
            .bind(cursor_time)
            .bind(limit + 1)
            .fetch_all(&state.pool)
            .await?
        } else {
            sqlx::query_as::<_, (String, String, String, String, DateTime<Utc>, Option<DateTime<Utc>>, bool)>(
                "SELECT id, repo_url, workflow, status, created_at, updated_at, retain_forever FROM sessions WHERE status = $1 ORDER BY created_at DESC LIMIT $2",
            )
            .bind(status)
            .bind(limit + 1)
            .fetch_all(&state.pool)
            .await?
        }
    } else if let Some(ref cursor) = query.cursor {
        let cursor_time: DateTime<Utc> = cursor
            .parse()
            .map_err(|_| AppError::bad_request("Invalid cursor"))?;
        sqlx::query_as::<_, (String, String, String, String, DateTime<Utc>, Option<DateTime<Utc>>, bool)>(
            "SELECT id, repo_url, workflow, status, created_at, updated_at, retain_forever FROM sessions WHERE created_at < $1 ORDER BY created_at DESC LIMIT $2",
        )
        .bind(cursor_time)
        .bind(limit + 1)
        .fetch_all(&state.pool)
        .await?
    } else {
        sqlx::query_as::<_, (String, String, String, String, DateTime<Utc>, Option<DateTime<Utc>>, bool)>(
            "SELECT id, repo_url, workflow, status, created_at, updated_at, retain_forever FROM sessions ORDER BY created_at DESC LIMIT $1",
        )
        .bind(limit + 1)
        .fetch_all(&state.pool)
        .await?
    };

    let has_more = rows.len() as i64 > limit;

    let items: Vec<SessionSummary> = rows
        .into_iter()
        .take(limit as usize)
        .map(
            |(id, repo_url, workflow, status, created_at, updated_at, retain_forever)| {
                let workflow: WorkflowType =
                    serde_json::from_value(json!(workflow)).unwrap_or(WorkflowType::Chat);
                let status: SessionStatus =
                    serde_json::from_value(json!(status)).unwrap_or(SessionStatus::Pending);
                SessionSummary {
                    session_id: SessionId::from_string(id),
                    repo_url,
                    workflow,
                    status,
                    created_at,
                    updated_at,
                    retain_forever: Some(retain_forever),
                }
            },
        )
        .collect();

    let next_cursor = if has_more {
        items.last().map(|s| s.created_at.to_rfc3339())
    } else {
        None
    };

    Ok(Json(PaginatedResponse { items, next_cursor }))
}

/// GET /sessions/:id
pub async fn get_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let session_row = sqlx::query_as::<_, (String, String, Option<String>, String, String, serde_json::Value, Option<String>, Option<String>, DateTime<Utc>, Option<DateTime<Utc>>, bool)>(
        r#"SELECT id::text, repo_url, ref, workflow, status, params, persona_id::text, identity_id, created_at, updated_at, retain_forever FROM sessions WHERE id = $1::uuid"#,
    )
    .bind(&id)
    .fetch_optional(&state.pool)
    .await?;

    let (sid, repo_url, _ref, workflow_str, status_str, params_json, persona_id, identity_id, created_at, updated_at, retain_forever) =
        session_row.ok_or_else(|| AppError::not_found("Session not found"))?;

    // Fetch jobs for this session
    let job_rows = sqlx::query_as::<_, (String, String, DateTime<Utc>, Option<String>, Option<String>, Option<String>, Option<String>)>(
        "SELECT id::text, status, created_at, error_message, pull_request_url, branch, commit_ref FROM jobs WHERE session_id = $1::uuid ORDER BY created_at ASC",
    )
    .bind(&id)
    .fetch_all(&state.pool)
    .await?;

    let jobs: Vec<JobSummary> = job_rows
        .into_iter()
        .map(|(jid, jstatus, jcreated, jerr, jpr, jbranch, jcommit)| {
            let status: SessionStatus =
                serde_json::from_value(json!(jstatus)).unwrap_or(SessionStatus::Pending);
            JobSummary {
                job_id: JobId::from_string(jid),
                status,
                created_at: jcreated,
                error_message: jerr,
                pull_request_url: jpr,
                branch: jbranch,
                commit_ref: jcommit,
            }
        })
        .collect();

    let workflow: WorkflowType =
        serde_json::from_value(json!(workflow_str)).unwrap_or(WorkflowType::Chat);
    let status: SessionStatus =
        serde_json::from_value(json!(status_str)).unwrap_or(SessionStatus::Pending);
    let params: Option<SessionParams> = serde_json::from_value(params_json).ok();

    let detail = SessionDetail {
        session_id: SessionId::from_string(sid),
        repo_url,
        workflow,
        status,
        created_at,
        updated_at,
        params,
        persona_id: persona_id.map(api_types::PersonaId::from_string),
        identity_id: identity_id.map(api_types::IdentityId::from_string),
        retain_forever: Some(retain_forever),
        jobs,
    };

    Ok(Json(detail))
}

#[derive(Debug, Deserialize)]
pub struct UpdateSessionRequest {
    pub retain_forever: Option<bool>,
}

/// PATCH /sessions/:id
pub async fn update_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateSessionRequest>,
) -> Result<impl IntoResponse, AppError> {
    if body.retain_forever.is_none() {
        return Ok(StatusCode::NO_CONTENT);
    }

    let result = sqlx::query(
        "UPDATE sessions SET retain_forever = $2, updated_at = now() WHERE id = $1::uuid",
    )
    .bind(&id)
    .bind(body.retain_forever.unwrap())
    .execute(&state.pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::not_found("Session not found"));
    }

    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub struct UpdateJobRequest {
    pub retain_forever: Option<bool>,
}

/// PATCH /sessions/:session_id/jobs/:job_id
pub async fn update_job(
    State(state): State<AppState>,
    Path((session_id, job_id)): Path<(String, String)>,
    Json(body): Json<UpdateJobRequest>,
) -> Result<impl IntoResponse, AppError> {
    // Note: jobs table doesn't have retain_forever column yet.
    // For now we validate the session and job exist and return 204.
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM jobs WHERE id = $1::uuid AND session_id = $2::uuid)",
    )
    .bind(&job_id)
    .bind(&session_id)
    .fetch_one(&state.pool)
    .await?;

    if !exists {
        return Err(AppError::not_found("Job not found"));
    }

    // If we need to store retain_forever on jobs, we'd need a migration.
    // For now just acknowledge.
    let _ = body;

    Ok(StatusCode::NO_CONTENT)
}

/// POST /sessions/:id/input — chat follow-up
pub async fn send_input(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<SendInputRequest>,
) -> Result<impl IntoResponse, AppError> {
    // Validate session exists, is chat workflow, and is running
    let session_row = sqlx::query_as::<_, (String, String)>(
        "SELECT workflow, status FROM sessions WHERE id = $1::uuid",
    )
    .bind(&id)
    .fetch_optional(&state.pool)
    .await?;

    let (workflow_str, status_str) =
        session_row.ok_or_else(|| AppError::not_found("Session not found"))?;

    if workflow_str != "chat" {
        return Err(AppError::conflict(
            "Input can only be sent to chat workflow sessions",
        ));
    }

    if status_str != "running" && status_str != "pending" {
        return Err(AppError::conflict(format!(
            "Session is '{}', input requires 'running' or 'pending' status",
            status_str
        )));
    }

    // Build history from previous jobs using chat module
    let max_turns = state.config.chat_history_max_turns as usize;
    let chat_history =
        engine::chat::assemble_history(&state.pool, &id, max_turns).await?;

    // Create follow-up job
    let job_id = JobId::new();
    let task_input = json!({
        "chat_followup": {
            "session_prompt": chat_history.session_prompt,
            "message": body.message,
            "history": chat_history.user_messages,
            "history_assistant": chat_history.assistant_replies,
            "history_truncated": chat_history.truncated
        }
    });

    sqlx::query(
        "INSERT INTO jobs (id, session_id, status, task_input) VALUES ($1::uuid, $2::uuid, 'pending', $3)",
    )
    .bind(job_id.as_str())
    .bind(&id)
    .bind(&task_input)
    .execute(&state.pool)
    .await?;

    // Update session status if needed
    engine::jobs::update_session_status(&state.pool, &id).await?;

    Ok((StatusCode::ACCEPTED, Json(json!({ "accepted": true }))))
}

/// GET /sessions/:id/events — SSE session lifecycle events
pub async fn stream_session_events(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, AppError> {
    // Check session exists
    let session_status: Option<String> =
        sqlx::query_scalar("SELECT status FROM sessions WHERE id = $1::uuid")
            .bind(&id)
            .fetch_optional(&state.pool)
            .await?;

    let status = session_status.ok_or_else(|| AppError::not_found("Session not found"))?;
    let is_terminal = status == "completed" || status == "failed";

    let session_id = id.clone();
    let rx = state.event_tx.subscribe();

    let event_stream = BroadcastStream::new(rx).filter_map(move |msg| {
        match msg {
            Ok(event) => {
                if event.session_id != session_id {
                    return None;
                }
                let data = serde_json::to_string(&event).unwrap_or_default();
                Some(Ok(Event::default().event("session_event").data(data)))
            }
            Err(_) => None,
        }
    });

    let stream: Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>> = if is_terminal {
        // Session already terminal — send empty stream
        Box::pin(futures::stream::empty())
    } else {
        Box::pin(event_stream)
    };

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keepalive"),
    ))
}

/// DELETE /sessions/:id
pub async fn delete_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    // CASCADE will delete jobs and logs
    let result = sqlx::query("DELETE FROM sessions WHERE id = $1::uuid")
        .bind(&id)
        .execute(&state.pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::not_found("Session not found"));
    }

    Ok(StatusCode::NO_CONTENT)
}
