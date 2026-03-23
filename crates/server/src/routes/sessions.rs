use crate::engine;
use crate::error::AppError;
use crate::state::AppState;
use api_types::{
    CreateSessionRequest, CreateSessionResponse, JobSummary, ListSessionsParams,
    PaginatedResponse, SendInputRequest, SendInputResponse, SessionDetail, SessionListItem,
    UpdateJobRequest, UpdateSessionRequest,
};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use chrono::{DateTime, Utc};
use tracing::info;

/// POST /sessions — Create a new session.
pub async fn create_session(
    State(state): State<AppState>,
    Json(req): Json<CreateSessionRequest>,
) -> Result<(StatusCode, Json<CreateSessionResponse>), AppError> {
    // Validate required fields
    if req.repo_url.is_empty() {
        return Err(AppError::InvalidRequest(
            "repo_url is required".to_string(),
        ));
    }

    let response = engine::create_session(&state.db, req).await?;

    Ok((StatusCode::CREATED, Json(response)))
}

/// GET /sessions — List sessions with optional status filter and cursor-based pagination.
pub async fn list_sessions(
    State(state): State<AppState>,
    Query(params): Query<ListSessionsParams>,
) -> Result<Json<PaginatedResponse<SessionListItem>>, AppError> {
    let limit = params.limit.unwrap_or(20).min(100) as i64;
    // Fetch one extra to determine if there's a next page
    let fetch_limit = limit + 1;

    let rows = if let Some(ref status) = params.status {
        if let Some(ref cursor) = params.cursor {
            // Cursor is the created_at timestamp of the last item + "|" + id
            let (cursor_ts, cursor_id) = parse_cursor(cursor)?;
            sqlx::query_as::<_, (String, String, String, String, String, DateTime<Utc>)>(
                "SELECT id, repo_url, ref_name, workflow, status, created_at
                 FROM sessions
                 WHERE status = $1 AND (created_at, id) < ($2, $3)
                 ORDER BY created_at DESC, id DESC
                 LIMIT $4"
            )
            .bind(status)
            .bind(cursor_ts)
            .bind(cursor_id)
            .bind(fetch_limit)
            .fetch_all(&state.db)
            .await
            .map_err(|e| AppError::Internal(e.into()))?
        } else {
            sqlx::query_as::<_, (String, String, String, String, String, DateTime<Utc>)>(
                "SELECT id, repo_url, ref_name, workflow, status, created_at
                 FROM sessions
                 WHERE status = $1
                 ORDER BY created_at DESC, id DESC
                 LIMIT $2"
            )
            .bind(status)
            .bind(fetch_limit)
            .fetch_all(&state.db)
            .await
            .map_err(|e| AppError::Internal(e.into()))?
        }
    } else if let Some(ref cursor) = params.cursor {
        let (cursor_ts, cursor_id) = parse_cursor(cursor)?;
        sqlx::query_as::<_, (String, String, String, String, String, DateTime<Utc>)>(
            "SELECT id, repo_url, ref_name, workflow, status, created_at
             FROM sessions
             WHERE (created_at, id) < ($1, $2)
             ORDER BY created_at DESC, id DESC
             LIMIT $3"
        )
        .bind(cursor_ts)
        .bind(cursor_id)
        .bind(fetch_limit)
        .fetch_all(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?
    } else {
        sqlx::query_as::<_, (String, String, String, String, String, DateTime<Utc>)>(
            "SELECT id, repo_url, ref_name, workflow, status, created_at
             FROM sessions
             ORDER BY created_at DESC, id DESC
             LIMIT $1"
        )
        .bind(fetch_limit)
        .fetch_all(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?
    };

    let has_more = rows.len() as i64 > limit;
    let items: Vec<SessionListItem> = rows
        .into_iter()
        .take(limit as usize)
        .map(|(id, repo_url, ref_name, workflow, status, created_at)| SessionListItem {
            session_id: id,
            repo_url,
            ref_name,
            workflow,
            status,
            created_at,
        })
        .collect();

    let next_cursor = if has_more {
        items
            .last()
            .map(|item| format!("{}|{}", item.created_at.to_rfc3339(), item.session_id))
    } else {
        None
    };

    Ok(Json(PaginatedResponse { items, next_cursor }))
}

/// GET /sessions/:id — Get session detail with jobs array.
pub async fn get_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<SessionDetail>, AppError> {
    let session = sqlx::query_as::<_, (String, String, String, String, String, serde_json::Value, DateTime<Utc>, DateTime<Utc>)>(
        "SELECT id, repo_url, ref_name, workflow, status, params, created_at, updated_at
         FROM sessions WHERE id = $1"
    )
    .bind(&session_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?
    .ok_or_else(|| AppError::NotFound(format!("session '{}' not found", session_id)))?;

    let (id, repo_url, ref_name, workflow, status, params, created_at, updated_at) = session;

    // Fetch jobs for this session
    let job_rows = sqlx::query_as::<_, (String, String, DateTime<Utc>, Option<String>, Option<String>)>(
        "SELECT id, status, created_at, error_message, pull_request_url
         FROM jobs WHERE session_id = $1
         ORDER BY iteration_index ASC"
    )
    .bind(&session_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    let jobs: Vec<JobSummary> = job_rows
        .into_iter()
        .map(|(job_id, job_status, job_created_at, error_message, pull_request_url)| JobSummary {
            job_id,
            status: job_status,
            created_at: job_created_at,
            error_message,
            pull_request_url,
        })
        .collect();

    Ok(Json(SessionDetail {
        session_id: id,
        repo_url,
        ref_name,
        workflow,
        status,
        params,
        jobs,
        created_at,
        updated_at,
    }))
}

/// DELETE /sessions/:id — Delete session (cascades to jobs and logs).
pub async fn delete_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let result = sqlx::query("DELETE FROM sessions WHERE id = $1")
        .bind(&session_id)
        .execute(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!(
            "session '{}' not found",
            session_id
        )));
    }

    info!(session_id = %session_id, "Session deleted");

    Ok(StatusCode::NO_CONTENT)
}

/// PATCH /sessions/:id — Update session (retain_forever).
pub async fn update_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(req): Json<UpdateSessionRequest>,
) -> Result<impl IntoResponse, AppError> {
    if let Some(retain_forever) = req.retain_forever {
        let now = Utc::now();
        let result = sqlx::query(
            "UPDATE sessions SET retain_forever = $1, updated_at = $2 WHERE id = $3",
        )
        .bind(retain_forever)
        .bind(now)
        .bind(&session_id)
        .execute(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!(
                "session '{}' not found",
                session_id
            )));
        }
    } else {
        // Verify the session exists even if no update fields provided
        let exists = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM sessions WHERE id = $1",
        )
        .bind(&session_id)
        .fetch_one(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

        if exists == 0 {
            return Err(AppError::NotFound(format!(
                "session '{}' not found",
                session_id
            )));
        }
    }

    Ok(StatusCode::NO_CONTENT)
}

/// PATCH /sessions/:id/jobs/:job_id — Update job (retain_forever).
pub async fn update_job(
    State(state): State<AppState>,
    Path((session_id, job_id)): Path<(String, String)>,
    Json(req): Json<UpdateJobRequest>,
) -> Result<impl IntoResponse, AppError> {
    // Check session exists
    let session_exists = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM sessions WHERE id = $1",
    )
    .bind(&session_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    if session_exists == 0 {
        return Err(AppError::NotFound(format!(
            "session '{}' not found",
            session_id
        )));
    }

    if let Some(retain_forever) = req.retain_forever {
        let now = Utc::now();
        let result = sqlx::query(
            "UPDATE jobs SET retain_forever = $1, updated_at = $2 WHERE id = $3 AND session_id = $4",
        )
        .bind(retain_forever)
        .bind(now)
        .bind(&job_id)
        .bind(&session_id)
        .execute(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!(
                "job '{}' not found in session '{}'",
                job_id, session_id
            )));
        }
    } else {
        // Verify the job exists in the session
        let job_exists = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM jobs WHERE id = $1 AND session_id = $2",
        )
        .bind(&job_id)
        .bind(&session_id)
        .fetch_one(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

        if job_exists == 0 {
            return Err(AppError::NotFound(format!(
                "job '{}' not found in session '{}'",
                job_id, session_id
            )));
        }
    }

    Ok(StatusCode::NO_CONTENT)
}

/// POST /sessions/:id/input — Send a follow-up message to a chat session.
pub async fn send_input(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(req): Json<SendInputRequest>,
) -> Result<(StatusCode, Json<SendInputResponse>), AppError> {
    if req.message.trim().is_empty() {
        return Err(AppError::InvalidRequest(
            "message is required and must not be empty".to_string(),
        ));
    }

    let response = engine::send_input(
        &state.db,
        &session_id,
        &req.message,
        state.config.chat_history_max_turns,
        &state.event_broadcaster,
    )
    .await?;

    info!(session_id = %session_id, "Follow-up message sent");

    Ok((StatusCode::ACCEPTED, Json(response)))
}

/// Parse a cursor string "timestamp|id" into its components.
fn parse_cursor(cursor: &str) -> Result<(DateTime<Utc>, String), AppError> {
    let parts: Vec<&str> = cursor.splitn(2, '|').collect();
    if parts.len() != 2 {
        return Err(AppError::InvalidRequest(
            "invalid cursor format".to_string(),
        ));
    }
    let ts = parts[0]
        .parse::<DateTime<Utc>>()
        .map_err(|_| AppError::InvalidRequest("invalid cursor timestamp".to_string()))?;
    let id = parts[1].to_string();
    Ok((ts, id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cursor_valid() {
        let ts = Utc::now();
        let cursor = format!("{}|some-id", ts.to_rfc3339());
        let (parsed_ts, parsed_id) = parse_cursor(&cursor).unwrap();
        assert_eq!(parsed_id, "some-id");
        // Timestamps should be close (within a second)
        assert!((parsed_ts - ts).num_seconds().abs() < 1);
    }

    #[test]
    fn test_parse_cursor_invalid() {
        let result = parse_cursor("bad-cursor");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_cursor_bad_timestamp() {
        let result = parse_cursor("not-a-timestamp|some-id");
        assert!(result.is_err());
    }
}
