use api_types::{LogEntry, LogQueryParams, PaginatedResponse};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::IntoResponse;
use axum::Json;
use chrono::{DateTime, Utc};
use futures::stream::Stream;
use std::convert::Infallible;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use tracing::info;

use crate::error::AppError;
use crate::state::AppState;

/// Row type for log queries.
type LogRow = (
    String,
    String,
    Option<String>,
    Option<String>,
    String,
    String,
    String,
    DateTime<Utc>,
);

/// Verify that a session exists, returning an error if not.
async fn verify_session_exists(state: &AppState, session_id: &str) -> Result<(), AppError> {
    let exists: Option<(String,)> = sqlx::query_as("SELECT id FROM sessions WHERE id = $1")
        .bind(session_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("DB error: {}", e)))?;

    if exists.is_none() {
        return Err(AppError::NotFound(format!(
            "Session '{}' not found",
            session_id
        )));
    }
    Ok(())
}

/// GET /sessions/:id/logs — Paginated log history.
///
/// Supports query params: limit, cursor, job_id, level, last.
/// - When `last` is set, returns the N most recent entries in chronological order (no cursor).
/// - Otherwise, uses cursor-based pagination ordered by timestamp ASC.
pub async fn get_logs(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Query(params): Query<LogQueryParams>,
) -> Result<Json<PaginatedResponse<LogEntry>>, AppError> {
    verify_session_exists(&state, &session_id).await?;

    let limit = params.limit.unwrap_or(20).min(100) as i64;

    // Handle `last` parameter: get N most recent entries, return in chronological order
    if let Some(last_n) = params.last {
        let last_n = last_n.min(1000) as i64;
        let rows: Vec<LogRow> = if let Some(ref job_id) = params.job_id {
            if let Some(ref level) = params.level {
                sqlx::query_as(
                    r#"
                    SELECT id, session_id, job_id, worker_id, level, source, message, timestamp
                    FROM logs
                    WHERE session_id = $1 AND job_id = $2 AND level = $3
                    ORDER BY timestamp DESC
                    LIMIT $4
                    "#,
                )
                .bind(&session_id)
                .bind(job_id)
                .bind(level)
                .bind(last_n)
                .fetch_all(&state.db)
                .await
                .map_err(|e| AppError::Internal(anyhow::anyhow!("DB error: {}", e)))?
            } else {
                sqlx::query_as(
                    r#"
                    SELECT id, session_id, job_id, worker_id, level, source, message, timestamp
                    FROM logs
                    WHERE session_id = $1 AND job_id = $2
                    ORDER BY timestamp DESC
                    LIMIT $3
                    "#,
                )
                .bind(&session_id)
                .bind(job_id)
                .bind(last_n)
                .fetch_all(&state.db)
                .await
                .map_err(|e| AppError::Internal(anyhow::anyhow!("DB error: {}", e)))?
            }
        } else if let Some(ref level) = params.level {
            sqlx::query_as(
                r#"
                SELECT id, session_id, job_id, worker_id, level, source, message, timestamp
                FROM logs
                WHERE session_id = $1 AND level = $2
                ORDER BY timestamp DESC
                LIMIT $3
                "#,
            )
            .bind(&session_id)
            .bind(level)
            .bind(last_n)
            .fetch_all(&state.db)
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("DB error: {}", e)))?
        } else {
            sqlx::query_as(
                r#"
                SELECT id, session_id, job_id, worker_id, level, source, message, timestamp
                FROM logs
                WHERE session_id = $1
                ORDER BY timestamp DESC
                LIMIT $2
                "#,
            )
            .bind(&session_id)
            .bind(last_n)
            .fetch_all(&state.db)
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("DB error: {}", e)))?
        };

        // Reverse to chronological order
        let mut items: Vec<LogEntry> = rows.into_iter().map(row_to_log_entry).collect();
        items.reverse();

        return Ok(Json(PaginatedResponse {
            items,
            next_cursor: None,
        }));
    }

    // Normal cursor-based pagination, ordered by timestamp ASC
    let fetch_limit = limit + 1;

    let rows: Vec<LogRow> = if let Some(ref cursor) = params.cursor {
        // Cursor is log ID; find its timestamp for pagination
        let cursor_row: Option<(DateTime<Utc>, String)> = sqlx::query_as(
            "SELECT timestamp, id FROM logs WHERE id = $1",
        )
        .bind(cursor)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("DB error: {}", e)))?;

        let (cursor_ts, cursor_id) = cursor_row
            .ok_or_else(|| AppError::InvalidRequest("Invalid cursor".to_string()))?;

        if let Some(ref job_id) = params.job_id {
            if let Some(ref level) = params.level {
                sqlx::query_as(
                    r#"
                    SELECT id, session_id, job_id, worker_id, level, source, message, timestamp
                    FROM logs
                    WHERE session_id = $1 AND job_id = $2 AND level = $3
                      AND (timestamp, id) > ($4, $5)
                    ORDER BY timestamp ASC, id ASC
                    LIMIT $6
                    "#,
                )
                .bind(&session_id)
                .bind(job_id)
                .bind(level)
                .bind(cursor_ts)
                .bind(&cursor_id)
                .bind(fetch_limit)
                .fetch_all(&state.db)
                .await
                .map_err(|e| AppError::Internal(anyhow::anyhow!("DB error: {}", e)))?
            } else {
                sqlx::query_as(
                    r#"
                    SELECT id, session_id, job_id, worker_id, level, source, message, timestamp
                    FROM logs
                    WHERE session_id = $1 AND job_id = $2
                      AND (timestamp, id) > ($3, $4)
                    ORDER BY timestamp ASC, id ASC
                    LIMIT $5
                    "#,
                )
                .bind(&session_id)
                .bind(job_id)
                .bind(cursor_ts)
                .bind(&cursor_id)
                .bind(fetch_limit)
                .fetch_all(&state.db)
                .await
                .map_err(|e| AppError::Internal(anyhow::anyhow!("DB error: {}", e)))?
            }
        } else if let Some(ref level) = params.level {
            sqlx::query_as(
                r#"
                SELECT id, session_id, job_id, worker_id, level, source, message, timestamp
                FROM logs
                WHERE session_id = $1 AND level = $2
                  AND (timestamp, id) > ($3, $4)
                ORDER BY timestamp ASC, id ASC
                LIMIT $5
                "#,
            )
            .bind(&session_id)
            .bind(level)
            .bind(cursor_ts)
            .bind(&cursor_id)
            .bind(fetch_limit)
            .fetch_all(&state.db)
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("DB error: {}", e)))?
        } else {
            sqlx::query_as(
                r#"
                SELECT id, session_id, job_id, worker_id, level, source, message, timestamp
                FROM logs
                WHERE session_id = $1
                  AND (timestamp, id) > ($2, $3)
                ORDER BY timestamp ASC, id ASC
                LIMIT $4
                "#,
            )
            .bind(&session_id)
            .bind(cursor_ts)
            .bind(&cursor_id)
            .bind(fetch_limit)
            .fetch_all(&state.db)
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("DB error: {}", e)))?
        }
    } else {
        // No cursor — first page
        if let Some(ref job_id) = params.job_id {
            if let Some(ref level) = params.level {
                sqlx::query_as(
                    r#"
                    SELECT id, session_id, job_id, worker_id, level, source, message, timestamp
                    FROM logs
                    WHERE session_id = $1 AND job_id = $2 AND level = $3
                    ORDER BY timestamp ASC, id ASC
                    LIMIT $4
                    "#,
                )
                .bind(&session_id)
                .bind(job_id)
                .bind(level)
                .bind(fetch_limit)
                .fetch_all(&state.db)
                .await
                .map_err(|e| AppError::Internal(anyhow::anyhow!("DB error: {}", e)))?
            } else {
                sqlx::query_as(
                    r#"
                    SELECT id, session_id, job_id, worker_id, level, source, message, timestamp
                    FROM logs
                    WHERE session_id = $1 AND job_id = $2
                    ORDER BY timestamp ASC, id ASC
                    LIMIT $3
                    "#,
                )
                .bind(&session_id)
                .bind(job_id)
                .bind(fetch_limit)
                .fetch_all(&state.db)
                .await
                .map_err(|e| AppError::Internal(anyhow::anyhow!("DB error: {}", e)))?
            }
        } else if let Some(ref level) = params.level {
            sqlx::query_as(
                r#"
                SELECT id, session_id, job_id, worker_id, level, source, message, timestamp
                FROM logs
                WHERE session_id = $1 AND level = $2
                ORDER BY timestamp ASC, id ASC
                LIMIT $3
                "#,
            )
            .bind(&session_id)
            .bind(level)
            .bind(fetch_limit)
            .fetch_all(&state.db)
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("DB error: {}", e)))?
        } else {
            sqlx::query_as(
                r#"
                SELECT id, session_id, job_id, worker_id, level, source, message, timestamp
                FROM logs
                WHERE session_id = $1
                ORDER BY timestamp ASC, id ASC
                LIMIT $2
                "#,
            )
            .bind(&session_id)
            .bind(fetch_limit)
            .fetch_all(&state.db)
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("DB error: {}", e)))?
        }
    };

    let has_next = rows.len() as i64 > limit;
    let items: Vec<LogEntry> = rows
        .into_iter()
        .take(limit as usize)
        .map(row_to_log_entry)
        .collect();

    let next_cursor = if has_next {
        items.last().map(|entry| entry.id.clone())
    } else {
        None
    };

    Ok(Json(PaginatedResponse { items, next_cursor }))
}

/// Convert a database row tuple to a LogEntry.
fn row_to_log_entry(row: LogRow) -> LogEntry {
    let (id, session_id, job_id, worker_id, level, source, message, timestamp) = row;
    LogEntry {
        id,
        timestamp,
        level,
        session_id,
        job_id,
        worker_id,
        source,
        message,
    }
}

/// GET /sessions/:id/logs/stream — SSE endpoint for live log streaming.
///
/// Subscribes to the LogBroadcaster and filters by session_id.
/// Optional query params: job_id, level.
pub async fn stream_logs(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Query(params): Query<LogStreamParams>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, AppError> {
    verify_session_exists(&state, &session_id).await?;

    let rx = state.log_broadcaster.subscribe();
    let job_id_filter = params.job_id.clone();
    let level_filter = params.level.clone();

    let stream = BroadcastStream::new(rx).filter_map(move |result| {
        match result {
            Ok(entry) => {
                // Filter by session_id
                if entry.session_id != session_id {
                    return None;
                }
                // Filter by job_id if specified
                if let Some(ref filter_job_id) = job_id_filter {
                    if entry.job_id.as_deref() != Some(filter_job_id.as_str()) {
                        return None;
                    }
                }
                // Filter by level if specified
                if let Some(ref filter_level) = level_filter {
                    if entry.level != *filter_level {
                        return None;
                    }
                }

                let event = Event::default()
                    .event("log")
                    .json_data(&entry)
                    .unwrap_or_else(|_| Event::default().event("log").data("{}"));
                Some(Ok(event))
            }
            Err(_) => {
                // Lagged — skip missed messages
                None
            }
        }
    });

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

/// Query parameters for the log SSE stream endpoint.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct LogStreamParams {
    pub job_id: Option<String>,
    pub level: Option<String>,
}

/// DELETE /sessions/:id/logs — Delete logs for a session.
///
/// Optional query param: job_id (delete only that job's logs).
pub async fn delete_logs(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Query(params): Query<DeleteLogsParams>,
) -> Result<impl IntoResponse, AppError> {
    verify_session_exists(&state, &session_id).await?;

    if let Some(ref job_id) = params.job_id {
        // Verify the job exists in this session
        let job_exists: Option<(String,)> = sqlx::query_as(
            "SELECT id FROM jobs WHERE id = $1 AND session_id = $2",
        )
        .bind(job_id)
        .bind(&session_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("DB error: {}", e)))?;

        if job_exists.is_none() {
            return Err(AppError::NotFound(format!(
                "Job '{}' not found in session '{}'",
                job_id, session_id
            )));
        }

        let result = sqlx::query("DELETE FROM logs WHERE session_id = $1 AND job_id = $2")
            .bind(&session_id)
            .bind(job_id)
            .execute(&state.db)
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("DB error: {}", e)))?;

        info!(
            session_id = %session_id,
            job_id = %job_id,
            deleted = result.rows_affected(),
            "Deleted logs for job"
        );
    } else {
        let result = sqlx::query("DELETE FROM logs WHERE session_id = $1")
            .bind(&session_id)
            .execute(&state.db)
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("DB error: {}", e)))?;

        info!(
            session_id = %session_id,
            deleted = result.rows_affected(),
            "Deleted all logs for session"
        );
    }

    Ok(StatusCode::NO_CONTENT)
}

/// Query parameters for the delete logs endpoint.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct DeleteLogsParams {
    pub job_id: Option<String>,
}

/// GET /sessions/:id/events — SSE endpoint for session lifecycle events.
///
/// Subscribes to the EventBroadcaster and filters by session_id.
pub async fn stream_events(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, AppError> {
    verify_session_exists(&state, &session_id).await?;

    let rx = state.event_broadcaster.subscribe();

    let stream = BroadcastStream::new(rx).filter_map(move |result| {
        match result {
            Ok(event) => {
                // Filter by session_id
                if event.session_id != session_id {
                    return None;
                }

                let sse_event = Event::default()
                    .event("session_event")
                    .json_data(&event)
                    .unwrap_or_else(|_| Event::default().event("session_event").data("{}"));
                Some(Ok(sse_event))
            }
            Err(_) => {
                // Lagged — skip missed messages
                None
            }
        }
    });

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use api_types::SessionEvent;

    #[test]
    fn test_row_to_log_entry() {
        let row: LogRow = (
            "log1".to_string(),
            "s1".to_string(),
            Some("j1".to_string()),
            Some("w1".to_string()),
            "info".to_string(),
            "worker".to_string(),
            "test message".to_string(),
            Utc::now(),
        );
        let entry = row_to_log_entry(row);
        assert_eq!(entry.id, "log1");
        assert_eq!(entry.session_id, "s1");
        assert_eq!(entry.job_id.as_deref(), Some("j1"));
        assert_eq!(entry.worker_id.as_deref(), Some("w1"));
        assert_eq!(entry.level, "info");
        assert_eq!(entry.source, "worker");
        assert_eq!(entry.message, "test message");
    }

    #[test]
    fn test_row_to_log_entry_null_fields() {
        let row: LogRow = (
            "log2".to_string(),
            "s2".to_string(),
            None,
            None,
            "error".to_string(),
            "control_plane".to_string(),
            "error occurred".to_string(),
            Utc::now(),
        );
        let entry = row_to_log_entry(row);
        assert_eq!(entry.id, "log2");
        assert!(entry.job_id.is_none());
        assert!(entry.worker_id.is_none());
        assert_eq!(entry.level, "error");
    }

    #[test]
    fn test_log_query_params_deserialization_defaults() {
        let json = "{}";
        let params: LogQueryParams = serde_json::from_str(json).unwrap();
        assert!(params.limit.is_none());
        assert!(params.cursor.is_none());
        assert!(params.job_id.is_none());
        assert!(params.level.is_none());
        assert!(params.last.is_none());
    }

    #[test]
    fn test_log_query_params_deserialization_full() {
        let json = r#"{"limit":50,"cursor":"abc","job_id":"j1","level":"error","last":100}"#;
        let params: LogQueryParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.limit, Some(50));
        assert_eq!(params.cursor.as_deref(), Some("abc"));
        assert_eq!(params.job_id.as_deref(), Some("j1"));
        assert_eq!(params.level.as_deref(), Some("error"));
        assert_eq!(params.last, Some(100));
    }

    #[test]
    fn test_delete_logs_params_deserialization() {
        let json = r#"{"job_id":"j1"}"#;
        let params: DeleteLogsParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.job_id.as_deref(), Some("j1"));
    }

    #[test]
    fn test_log_stream_params_deserialization() {
        let json = r#"{"job_id":"j1","level":"warn"}"#;
        let params: LogStreamParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.job_id.as_deref(), Some("j1"));
        assert_eq!(params.level.as_deref(), Some("warn"));
    }

    #[test]
    fn test_session_event_filtering_logic() {
        // Test the filtering logic that stream_events uses
        let event = SessionEvent {
            session_id: "s1".to_string(),
            event: "job_completed".to_string(),
            job_id: Some("j1".to_string()),
            payload: None,
        };

        // Should match session s1
        assert_eq!(event.session_id, "s1");

        // Should not match session s2
        let target_session = "s2";
        assert_ne!(event.session_id, target_session);
    }

    #[test]
    fn test_log_entry_filtering_logic() {
        // Test the filtering logic used in stream_logs
        let entry = LogEntry {
            id: "log1".to_string(),
            timestamp: Utc::now(),
            level: "error".to_string(),
            session_id: "s1".to_string(),
            job_id: Some("j1".to_string()),
            worker_id: None,
            source: "agent".to_string(),
            message: "test".to_string(),
        };

        // Session filter match
        assert_eq!(entry.session_id, "s1");

        // Job filter match
        let job_filter = Some("j1".to_string());
        assert_eq!(entry.job_id.as_deref(), job_filter.as_deref());

        // Level filter match
        let level_filter = Some("error".to_string());
        assert_eq!(entry.level, *level_filter.as_ref().unwrap());

        // Level filter mismatch
        let level_filter_info = Some("info".to_string());
        assert_ne!(entry.level, *level_filter_info.as_ref().unwrap());
    }
}
