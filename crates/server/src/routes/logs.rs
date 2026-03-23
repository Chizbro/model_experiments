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

use api_types::{LogEntry, LogLevel, PaginatedResponse, SendLogsRequest};

use crate::error::AppError;
use crate::sse::{LogBroadcast, LogEntryPayload};
use crate::state::AppState;

/// POST /workers/tasks/:id/logs
pub async fn ingest_logs(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    Json(body): Json<SendLogsRequest>,
) -> Result<impl IntoResponse, AppError> {
    if body.entries.is_empty() {
        return Ok((StatusCode::ACCEPTED, Json(json!({ "accepted": true }))));
    }

    // Look up the job to get session_id and worker_id
    let job_row = sqlx::query_as::<_, (String, String, Option<String>)>(
        "SELECT id::text, session_id::text, worker_id FROM jobs WHERE id = $1::uuid",
    )
    .bind(&task_id)
    .fetch_optional(&state.pool)
    .await?;

    let (job_id_str, session_id_str, worker_id) =
        job_row.ok_or_else(|| AppError::not_found("Task not found"))?;

    // Bulk insert log entries
    for entry in &body.entries {
        let level_str = serde_json::to_value(&entry.level)
            .unwrap_or(json!("info"))
            .as_str()
            .unwrap_or("info")
            .to_string();

        sqlx::query(
            r#"
            INSERT INTO logs (session_id, job_id, worker_id, level, source, message, timestamp)
            VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, $7)
            "#,
        )
        .bind(&session_id_str)
        .bind(&job_id_str)
        .bind(worker_id.as_deref())
        .bind(&level_str)
        .bind(&entry.source)
        .bind(&entry.message)
        .bind(entry.timestamp)
        .execute(&state.pool)
        .await?;
    }

    // Broadcast full log entries for SSE subscribers
    for entry in &body.entries {
        let level_str_bc = serde_json::to_value(&entry.level)
            .unwrap_or(json!("info"))
            .as_str()
            .unwrap_or("info")
            .to_string();

        let payload = LogEntryPayload {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: entry.timestamp.to_rfc3339(),
            level: level_str_bc.clone(),
            session_id: session_id_str.clone(),
            job_id: job_id_str.clone(),
            worker_id: worker_id.clone(),
            source: entry.source.clone(),
            message: entry.message.clone(),
        };

        let broadcast = LogBroadcast {
            session_id: session_id_str.clone(),
            job_id: job_id_str.clone(),
            entry: payload,
        };
        let _ = state.log_tx.send(broadcast);
    }

    // Dual-write: append to local log file
    if let Some(ref log_dir) = state.config.log_dir {
        let _ = write_local_logs(log_dir, &session_id_str, &body).await;
    }

    Ok((StatusCode::ACCEPTED, Json(json!({ "accepted": true }))))
}

async fn write_local_logs(
    log_dir: &str,
    session_id: &str,
    body: &SendLogsRequest,
) {
    let dir = std::path::Path::new(log_dir);
    if let Err(e) = tokio::fs::create_dir_all(dir).await {
        tracing::warn!(?e, "Failed to create log directory");
        return;
    }

    let file_path = dir.join(format!("{}.jsonl", session_id));
    let mut lines = String::new();
    for entry in &body.entries {
        if let Ok(json_line) = serde_json::to_string(entry) {
            lines.push_str(&json_line);
            lines.push('\n');
        }
    }

    if let Err(e) = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&file_path)
        .await
    {
        tracing::warn!(?e, path = %file_path.display(), "Failed to open log file");
        return;
    }

    // Use tokio write
    use tokio::io::AsyncWriteExt;
    match tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&file_path)
        .await
    {
        Ok(mut file) => {
            if let Err(e) = file.write_all(lines.as_bytes()).await {
                tracing::warn!(?e, "Failed to write to log file");
            }
        }
        Err(e) => {
            tracing::warn!(?e, "Failed to open log file for writing");
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct GetLogsQuery {
    pub limit: Option<i64>,
    pub cursor: Option<String>,
    pub job_id: Option<String>,
    pub level: Option<String>,
    pub last: Option<i64>,
}

/// GET /sessions/:id/logs
pub async fn get_session_logs(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<GetLogsQuery>,
) -> Result<impl IntoResponse, AppError> {
    // Check session exists
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sessions WHERE id = $1::uuid)")
            .bind(&id)
            .fetch_one(&state.pool)
            .await?;

    if !exists {
        return Err(AppError::not_found("Session not found"));
    }

    let limit = query.limit.unwrap_or(100).min(1000);

    // "last" mode: get last N entries (ordered by timestamp DESC, then reverse)
    if let Some(last_n) = query.last {
        let last_n = last_n.min(1000);
        let rows = if let Some(ref job_id) = query.job_id {
            sqlx::query_as::<_, (String, DateTime<Utc>, String, String, Option<String>, String, Option<String>)>(
                r#"
                SELECT id::text, timestamp, level, source, worker_id, message, job_id::text
                FROM logs
                WHERE session_id = $1::uuid AND job_id = $2::uuid
                ORDER BY timestamp DESC, id DESC
                LIMIT $3
                "#,
            )
            .bind(&id)
            .bind(job_id)
            .bind(last_n)
            .fetch_all(&state.pool)
            .await?
        } else {
            sqlx::query_as::<_, (String, DateTime<Utc>, String, String, Option<String>, String, Option<String>)>(
                r#"
                SELECT id::text, timestamp, level, source, worker_id, message, job_id::text
                FROM logs
                WHERE session_id = $1::uuid
                ORDER BY timestamp DESC, id DESC
                LIMIT $2
                "#,
            )
            .bind(&id)
            .bind(last_n)
            .fetch_all(&state.pool)
            .await?
        };

        let mut items: Vec<LogEntry> = rows
            .into_iter()
            .map(|(log_id, ts, level, source, worker_id, message, job_id)| {
                to_log_entry(log_id, ts, level, source, worker_id, message, job_id, &id)
            })
            .collect();

        // Reverse to get chronological order
        items.reverse();

        return Ok(Json(PaginatedResponse {
            items,
            next_cursor: None,
        }));
    }

    // Standard paginated mode (ASC order with cursor)
    let rows = if let Some(ref cursor) = query.cursor {
        // Decode cursor: base64 of "timestamp|id"
        let decoded = decode_cursor(cursor)?;

        if let Some(ref job_id) = query.job_id {
            if let Some(ref level) = query.level {
                sqlx::query_as::<_, (String, DateTime<Utc>, String, String, Option<String>, String, Option<String>)>(
                    r#"
                    SELECT id::text, timestamp, level, source, worker_id, message, job_id::text
                    FROM logs
                    WHERE session_id = $1::uuid AND job_id = $2::uuid AND level = $3
                      AND (timestamp, id) > ($4, $5::uuid)
                    ORDER BY timestamp ASC, id ASC
                    LIMIT $6
                    "#,
                )
                .bind(&id)
                .bind(job_id)
                .bind(level)
                .bind(decoded.timestamp)
                .bind(&decoded.id)
                .bind(limit + 1)
                .fetch_all(&state.pool)
                .await?
            } else {
                sqlx::query_as::<_, (String, DateTime<Utc>, String, String, Option<String>, String, Option<String>)>(
                    r#"
                    SELECT id::text, timestamp, level, source, worker_id, message, job_id::text
                    FROM logs
                    WHERE session_id = $1::uuid AND job_id = $2::uuid
                      AND (timestamp, id) > ($3, $4::uuid)
                    ORDER BY timestamp ASC, id ASC
                    LIMIT $5
                    "#,
                )
                .bind(&id)
                .bind(job_id)
                .bind(decoded.timestamp)
                .bind(&decoded.id)
                .bind(limit + 1)
                .fetch_all(&state.pool)
                .await?
            }
        } else if let Some(ref level) = query.level {
            sqlx::query_as::<_, (String, DateTime<Utc>, String, String, Option<String>, String, Option<String>)>(
                r#"
                SELECT id::text, timestamp, level, source, worker_id, message, job_id::text
                FROM logs
                WHERE session_id = $1::uuid AND level = $2
                  AND (timestamp, id) > ($3, $4::uuid)
                ORDER BY timestamp ASC, id ASC
                LIMIT $5
                "#,
            )
            .bind(&id)
            .bind(level)
            .bind(decoded.timestamp)
            .bind(&decoded.id)
            .bind(limit + 1)
            .fetch_all(&state.pool)
            .await?
        } else {
            sqlx::query_as::<_, (String, DateTime<Utc>, String, String, Option<String>, String, Option<String>)>(
                r#"
                SELECT id::text, timestamp, level, source, worker_id, message, job_id::text
                FROM logs
                WHERE session_id = $1::uuid
                  AND (timestamp, id) > ($2, $3::uuid)
                ORDER BY timestamp ASC, id ASC
                LIMIT $4
                "#,
            )
            .bind(&id)
            .bind(decoded.timestamp)
            .bind(&decoded.id)
            .bind(limit + 1)
            .fetch_all(&state.pool)
            .await?
        }
    } else {
        // No cursor - start from beginning
        if let Some(ref job_id) = query.job_id {
            if let Some(ref level) = query.level {
                sqlx::query_as::<_, (String, DateTime<Utc>, String, String, Option<String>, String, Option<String>)>(
                    r#"
                    SELECT id::text, timestamp, level, source, worker_id, message, job_id::text
                    FROM logs
                    WHERE session_id = $1::uuid AND job_id = $2::uuid AND level = $3
                    ORDER BY timestamp ASC, id ASC
                    LIMIT $4
                    "#,
                )
                .bind(&id)
                .bind(job_id)
                .bind(level)
                .bind(limit + 1)
                .fetch_all(&state.pool)
                .await?
            } else {
                sqlx::query_as::<_, (String, DateTime<Utc>, String, String, Option<String>, String, Option<String>)>(
                    r#"
                    SELECT id::text, timestamp, level, source, worker_id, message, job_id::text
                    FROM logs
                    WHERE session_id = $1::uuid AND job_id = $2::uuid
                    ORDER BY timestamp ASC, id ASC
                    LIMIT $3
                    "#,
                )
                .bind(&id)
                .bind(job_id)
                .bind(limit + 1)
                .fetch_all(&state.pool)
                .await?
            }
        } else if let Some(ref level) = query.level {
            sqlx::query_as::<_, (String, DateTime<Utc>, String, String, Option<String>, String, Option<String>)>(
                r#"
                SELECT id::text, timestamp, level, source, worker_id, message, job_id::text
                FROM logs
                WHERE session_id = $1::uuid AND level = $2
                ORDER BY timestamp ASC, id ASC
                LIMIT $3
                "#,
            )
            .bind(&id)
            .bind(level)
            .bind(limit + 1)
            .fetch_all(&state.pool)
            .await?
        } else {
            sqlx::query_as::<_, (String, DateTime<Utc>, String, String, Option<String>, String, Option<String>)>(
                r#"
                SELECT id::text, timestamp, level, source, worker_id, message, job_id::text
                FROM logs
                WHERE session_id = $1::uuid
                ORDER BY timestamp ASC, id ASC
                LIMIT $2
                "#,
            )
            .bind(&id)
            .bind(limit + 1)
            .fetch_all(&state.pool)
            .await?
        }
    };

    let has_more = rows.len() as i64 > limit;

    let items: Vec<LogEntry> = rows
        .into_iter()
        .take(limit as usize)
        .map(|(log_id, ts, level, source, worker_id, message, job_id)| {
            to_log_entry(log_id, ts, level, source, worker_id, message, job_id, &id)
        })
        .collect();

    let next_cursor = if has_more {
        items.last().map(|entry| encode_cursor(&entry.timestamp, &entry.id))
    } else {
        None
    };

    Ok(Json(PaginatedResponse { items, next_cursor }))
}

#[derive(Debug, Deserialize)]
pub struct DeleteLogsQuery {
    pub job_id: Option<String>,
}

/// DELETE /sessions/:id/logs
pub async fn delete_session_logs(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<DeleteLogsQuery>,
) -> Result<impl IntoResponse, AppError> {
    // Check session exists
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sessions WHERE id = $1::uuid)")
            .bind(&id)
            .fetch_one(&state.pool)
            .await?;

    if !exists {
        return Err(AppError::not_found("Session not found"));
    }

    if let Some(ref job_id) = query.job_id {
        // Verify job belongs to session
        let job_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM jobs WHERE id = $1::uuid AND session_id = $2::uuid)",
        )
        .bind(job_id)
        .bind(&id)
        .fetch_one(&state.pool)
        .await?;

        if !job_exists {
            return Err(AppError::not_found("Job not found in this session"));
        }

        sqlx::query("DELETE FROM logs WHERE session_id = $1::uuid AND job_id = $2::uuid")
            .bind(&id)
            .bind(job_id)
            .execute(&state.pool)
            .await?;
    } else {
        sqlx::query("DELETE FROM logs WHERE session_id = $1::uuid")
            .bind(&id)
            .execute(&state.pool)
            .await?;
    }

    Ok(StatusCode::NO_CONTENT)
}

// --- Cursor helpers ---

struct DecodedCursor {
    timestamp: DateTime<Utc>,
    id: String,
}

fn encode_cursor(timestamp: &DateTime<Utc>, id: &str) -> String {
    use base64::Engine;
    let raw = format!("{}|{}", timestamp.to_rfc3339(), id);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(raw.as_bytes())
}

fn decode_cursor(cursor: &str) -> Result<DecodedCursor, AppError> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(cursor)
        .map_err(|_| AppError::bad_request("Invalid cursor"))?;
    let raw = String::from_utf8(bytes).map_err(|_| AppError::bad_request("Invalid cursor"))?;
    let parts: Vec<&str> = raw.splitn(2, '|').collect();
    if parts.len() != 2 {
        return Err(AppError::bad_request("Invalid cursor format"));
    }
    let timestamp: DateTime<Utc> = parts[0]
        .parse()
        .map_err(|_| AppError::bad_request("Invalid cursor timestamp"))?;
    Ok(DecodedCursor {
        timestamp,
        id: parts[1].to_string(),
    })
}

#[allow(clippy::too_many_arguments)]
fn to_log_entry(
    log_id: String,
    timestamp: DateTime<Utc>,
    level: String,
    source: String,
    worker_id: Option<String>,
    message: String,
    job_id: Option<String>,
    session_id: &str,
) -> LogEntry {
    let level = match level.as_str() {
        "debug" => LogLevel::Debug,
        "warn" => LogLevel::Warn,
        "error" => LogLevel::Error,
        _ => LogLevel::Info,
    };
    LogEntry {
        id: log_id,
        timestamp,
        level,
        session_id: api_types::SessionId::from_string(session_id),
        job_id: api_types::JobId::from_string(job_id.unwrap_or_default()),
        worker_id: worker_id.map(api_types::WorkerId::from_string),
        source,
        message,
    }
}

#[derive(Debug, Deserialize)]
pub struct StreamLogsQuery {
    pub job_id: Option<String>,
    pub level: Option<String>,
}

/// GET /sessions/:id/logs/stream — SSE log stream
pub async fn stream_session_logs(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<StreamLogsQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, AppError> {
    // Check session exists
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sessions WHERE id = $1::uuid)")
            .bind(&id)
            .fetch_one(&state.pool)
            .await?;

    if !exists {
        return Err(AppError::not_found("Session not found"));
    }

    // Check if session is already in a terminal state
    let session_status: String =
        sqlx::query_scalar("SELECT status FROM sessions WHERE id = $1::uuid")
            .bind(&id)
            .fetch_one(&state.pool)
            .await?;

    let job_id_filter = query.job_id.clone();
    let level_filter = query.level.clone();
    let session_id = id.clone();
    let is_terminal = session_status == "completed" || session_status == "failed";

    let rx = state.log_tx.subscribe();
    let event_rx = state.event_tx.subscribe();

    let log_stream = BroadcastStream::new(rx).filter_map(move |msg| {
        match msg {
            Ok(broadcast) => {
                if broadcast.session_id != session_id {
                    return None;
                }
                // Filter by job_id if specified
                if let Some(ref jid) = job_id_filter {
                    if &broadcast.job_id != jid {
                        return None;
                    }
                }
                // Filter by level if specified
                if let Some(ref lvl) = level_filter {
                    if &broadcast.entry.level != lvl {
                        return None;
                    }
                }
                let data = serde_json::to_string(&broadcast.entry).unwrap_or_default();
                Some(Ok(Event::default().event("log").data(data)))
            }
            Err(_) => None,
        }
    });

    // Watch for session terminal events to close the stream
    let session_id_for_close = id.clone();
    let close_stream = BroadcastStream::new(event_rx).filter_map(move |msg| {
        match msg {
            Ok(event) => {
                if event.session_id != session_id_for_close {
                    return None;
                }
                if event.event == "completed" || event.event == "failed" {
                    let data = serde_json::to_string(&event).unwrap_or_default();
                    Some(Ok(Event::default().event("session_event").data(data)))
                } else {
                    None
                }
            }
            Err(_) => None,
        }
    });

    // Merge log events and close events; if session is already terminal, return empty stream
    let merged: Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>> = if is_terminal {
        Box::pin(futures::stream::empty())
    } else {
        Box::pin(log_stream.merge(close_stream))
    };

    Ok(Sse::new(merged).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keepalive"),
    ))
}

/// Insert a control-plane event log entry directly into the database.
/// Also broadcasts to the log SSE channel if a sender is provided.
pub async fn insert_control_plane_log(
    pool: &sqlx::PgPool,
    session_id: &str,
    job_id: Option<&str>,
    level: &str,
    message: &str,
) {
    let result = sqlx::query(
        r#"
        INSERT INTO logs (session_id, job_id, level, source, message, timestamp)
        VALUES ($1::uuid, $2::uuid, $3, 'control_plane', $4, now())
        "#,
    )
    .bind(session_id)
    .bind(job_id)
    .bind(level)
    .bind(message)
    .execute(pool)
    .await;

    if let Err(e) = result {
        tracing::warn!(?e, "Failed to insert control plane log");
    }
}

/// Insert a control-plane log and broadcast it to SSE subscribers.
pub async fn insert_and_broadcast_control_plane_log(
    pool: &sqlx::PgPool,
    log_tx: &tokio::sync::broadcast::Sender<LogBroadcast>,
    session_id: &str,
    job_id: Option<&str>,
    level: &str,
    message: &str,
) {
    insert_control_plane_log(pool, session_id, job_id, level, message).await;

    let payload = LogEntryPayload {
        id: uuid::Uuid::new_v4().to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        level: level.to_string(),
        session_id: session_id.to_string(),
        job_id: job_id.unwrap_or("").to_string(),
        worker_id: None,
        source: "control_plane".to_string(),
        message: message.to_string(),
    };

    let broadcast = LogBroadcast {
        session_id: session_id.to_string(),
        job_id: job_id.unwrap_or("").to_string(),
        entry: payload,
    };
    let _ = log_tx.send(broadcast);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cursor_roundtrip() {
        let ts = Utc::now();
        let id = "550e8400-e29b-41d4-a716-446655440000";
        let encoded = encode_cursor(&ts, id);
        let decoded = decode_cursor(&encoded).unwrap();
        assert_eq!(decoded.id, id);
        // Timestamps should be very close (rfc3339 roundtrip loses sub-nanosecond precision)
        assert!((decoded.timestamp - ts).num_milliseconds().abs() < 1);
    }

    #[test]
    fn test_decode_invalid_cursor() {
        assert!(decode_cursor("not-valid-base64!!!").is_err());
    }

    #[test]
    fn test_to_log_entry() {
        let entry = to_log_entry(
            "log-1".to_string(),
            Utc::now(),
            "error".to_string(),
            "worker".to_string(),
            Some("w-1".to_string()),
            "something broke".to_string(),
            Some("j-1".to_string()),
            "s-1",
        );
        assert_eq!(entry.level, LogLevel::Error);
        assert_eq!(entry.message, "something broke");
    }
}
