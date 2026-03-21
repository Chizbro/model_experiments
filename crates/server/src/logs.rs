//! Central log store: list, delete, retention purge ([`docs/API_OVERVIEW.md`](../../docs/API_OVERVIEW.md) §6).

use crate::auth::AuthError;
use crate::AppState;
use api_types::{LogEntry, PaginatedLogEntries, WorkerLogIngestItem, WorkerLogsAcceptedResponse};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use base64::prelude::{Engine as _, BASE64_URL_SAFE_NO_PAD};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct SessionLogsQuery {
    pub limit: Option<i64>,
    pub cursor: Option<String>,
    pub job_id: Option<String>,
    pub level: Option<String>,
    /// When set, return the N newest entries in chronological order (one page; no cursor).
    pub last: Option<i64>,
}

fn encode_log_cursor(occurred_at: DateTime<Utc>, id: i64) -> String {
    let raw = format!(
        "{}|{}",
        occurred_at.to_rfc3339_opts(SecondsFormat::Millis, true),
        id
    );
    BASE64_URL_SAFE_NO_PAD.encode(raw.as_bytes())
}

fn parse_log_cursor(s: &str) -> Result<(DateTime<Utc>, i64), String> {
    let bytes = BASE64_URL_SAFE_NO_PAD
        .decode(s.as_bytes())
        .map_err(|e| e.to_string())?;
    let raw = String::from_utf8(bytes).map_err(|e| e.to_string())?;
    let (t, id) = raw
        .split_once('|')
        .ok_or_else(|| "expected '<rfc3339>|<id>'".to_string())?;
    let occurred_at = DateTime::parse_from_rfc3339(t)
        .map_err(|e| e.to_string())?
        .with_timezone(&Utc);
    let id: i64 = id
        .parse()
        .map_err(|e: std::num::ParseIntError| e.to_string())?;
    Ok((occurred_at, id))
}

fn clamp_limit(n: Option<i64>) -> i64 {
    let n = n.unwrap_or(20);
    n.clamp(1, 100)
}

fn clamp_last(n: i64) -> i64 {
    n.clamp(1, 500)
}

pub(crate) async fn session_exists(pool: &PgPool, sid: Uuid) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM sessions WHERE id = $1)")
        .bind(sid)
        .fetch_one(pool)
        .await
}

/// `GET /sessions/:id/logs`
pub async fn list_session_logs(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<SessionLogsQuery>,
) -> Result<Json<PaginatedLogEntries>, AuthError> {
    let Some(pool) = &state.db else {
        return Err(AuthError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "not_ready",
            "Database is not configured; cannot list logs",
        ));
    };

    let sid = Uuid::parse_str(id.trim()).map_err(|_| {
        AuthError::new(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "session id must be a UUID",
        )
    })?;

    if !session_exists(pool, sid).await.map_err(|e| {
        AuthError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            &format!("database error: {e}"),
        )
    })? {
        return Err(AuthError::new(
            StatusCode::NOT_FOUND,
            "not_found",
            "Session not found",
        ));
    }

    let job_filter = q
        .job_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(Uuid::parse_str)
        .transpose()
        .map_err(|_| {
            AuthError::new(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "job_id must be a UUID",
            )
        })?;

    let level_filter = q
        .level
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_lowercase());

    if let Some(last_n) = q.last {
        let n = clamp_last(last_n);
        type Row = (
            i64,
            DateTime<Utc>,
            String,
            Uuid,
            Option<Uuid>,
            Option<String>,
            String,
            String,
        );
        let rows: Vec<Row> = match (&job_filter, &level_filter) {
            (Some(jid), Some(lv)) => sqlx::query_as(
                r#"
                SELECT id, occurred_at, log_level, session_id, job_id, worker_id, log_source, content
                FROM logs
                WHERE session_id = $1 AND job_id = $2 AND lower(log_level) = $3
                ORDER BY occurred_at DESC, id DESC
                LIMIT $4
                "#,
            )
            .bind(sid)
            .bind(jid)
            .bind(lv)
            .bind(n)
            .fetch_all(pool)
            .await,
            (Some(jid), None) => sqlx::query_as(
                r#"
                SELECT id, occurred_at, log_level, session_id, job_id, worker_id, log_source, content
                FROM logs
                WHERE session_id = $1 AND job_id = $2
                ORDER BY occurred_at DESC, id DESC
                LIMIT $3
                "#,
            )
            .bind(sid)
            .bind(jid)
            .bind(n)
            .fetch_all(pool)
            .await,
            (None, Some(lv)) => sqlx::query_as(
                r#"
                SELECT id, occurred_at, log_level, session_id, job_id, worker_id, log_source, content
                FROM logs
                WHERE session_id = $1 AND lower(log_level) = $2
                ORDER BY occurred_at DESC, id DESC
                LIMIT $3
                "#,
            )
            .bind(sid)
            .bind(lv)
            .bind(n)
            .fetch_all(pool)
            .await,
            (None, None) => sqlx::query_as(
                r#"
                SELECT id, occurred_at, log_level, session_id, job_id, worker_id, log_source, content
                FROM logs
                WHERE session_id = $1
                ORDER BY occurred_at DESC, id DESC
                LIMIT $2
                "#,
            )
            .bind(sid)
            .bind(n)
            .fetch_all(pool)
            .await,
        }
        .map_err(|e| {
            AuthError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                &format!("database error: {e}"),
            )
        })?;

        let mut rows: Vec<Row> = rows;
        rows.reverse();

        let items = rows
            .into_iter()
            .map(|(lid, ts, lvl, sess, jid, wid, src, msg)| LogEntry {
                id: lid.to_string(),
                timestamp: ts,
                level: lvl,
                session_id: sess.to_string(),
                job_id: jid.map(|u| u.to_string()),
                worker_id: wid,
                source: src,
                message: msg,
            })
            .collect();

        return Ok(Json(PaginatedLogEntries {
            items,
            next_cursor: None,
        }));
    }

    let limit = clamp_limit(q.limit);
    let fetch = limit + 1;
    let cursor = q
        .cursor
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(parse_log_cursor)
        .transpose()
        .map_err(|msg| {
            AuthError::new(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                &format!("invalid cursor: {msg}"),
            )
        })?;

    type Row = (
        i64,
        DateTime<Utc>,
        String,
        Uuid,
        Option<Uuid>,
        Option<String>,
        String,
        String,
    );

    let rows: Vec<Row> = match (&job_filter, &level_filter, cursor) {
        (Some(jid), Some(lv), Some((c_at, c_id))) => {
            sqlx::query_as(
                r#"
            SELECT id, occurred_at, log_level, session_id, job_id, worker_id, log_source, content
            FROM logs
            WHERE session_id = $1
              AND job_id = $2
              AND lower(log_level) = $3
              AND (occurred_at, id) > ($4::timestamptz, $5::bigint)
            ORDER BY occurred_at ASC, id ASC
            LIMIT $6
            "#,
            )
            .bind(sid)
            .bind(jid)
            .bind(lv)
            .bind(c_at)
            .bind(c_id)
            .bind(fetch)
            .fetch_all(pool)
            .await
        }
        (Some(jid), Some(lv), None) => {
            sqlx::query_as(
                r#"
            SELECT id, occurred_at, log_level, session_id, job_id, worker_id, log_source, content
            FROM logs
            WHERE session_id = $1 AND job_id = $2 AND lower(log_level) = $3
            ORDER BY occurred_at ASC, id ASC
            LIMIT $4
            "#,
            )
            .bind(sid)
            .bind(jid)
            .bind(lv)
            .bind(fetch)
            .fetch_all(pool)
            .await
        }
        (Some(jid), None, Some((c_at, c_id))) => {
            sqlx::query_as(
                r#"
            SELECT id, occurred_at, log_level, session_id, job_id, worker_id, log_source, content
            FROM logs
            WHERE session_id = $1
              AND job_id = $2
              AND (occurred_at, id) > ($3::timestamptz, $4::bigint)
            ORDER BY occurred_at ASC, id ASC
            LIMIT $5
            "#,
            )
            .bind(sid)
            .bind(jid)
            .bind(c_at)
            .bind(c_id)
            .bind(fetch)
            .fetch_all(pool)
            .await
        }
        (Some(jid), None, None) => {
            sqlx::query_as(
                r#"
            SELECT id, occurred_at, log_level, session_id, job_id, worker_id, log_source, content
            FROM logs
            WHERE session_id = $1 AND job_id = $2
            ORDER BY occurred_at ASC, id ASC
            LIMIT $3
            "#,
            )
            .bind(sid)
            .bind(jid)
            .bind(fetch)
            .fetch_all(pool)
            .await
        }
        (None, Some(lv), Some((c_at, c_id))) => {
            sqlx::query_as(
                r#"
            SELECT id, occurred_at, log_level, session_id, job_id, worker_id, log_source, content
            FROM logs
            WHERE session_id = $1
              AND lower(log_level) = $2
              AND (occurred_at, id) > ($3::timestamptz, $4::bigint)
            ORDER BY occurred_at ASC, id ASC
            LIMIT $5
            "#,
            )
            .bind(sid)
            .bind(lv)
            .bind(c_at)
            .bind(c_id)
            .bind(fetch)
            .fetch_all(pool)
            .await
        }
        (None, Some(lv), None) => {
            sqlx::query_as(
                r#"
            SELECT id, occurred_at, log_level, session_id, job_id, worker_id, log_source, content
            FROM logs
            WHERE session_id = $1 AND lower(log_level) = $2
            ORDER BY occurred_at ASC, id ASC
            LIMIT $3
            "#,
            )
            .bind(sid)
            .bind(lv)
            .bind(fetch)
            .fetch_all(pool)
            .await
        }
        (None, None, Some((c_at, c_id))) => {
            sqlx::query_as(
                r#"
            SELECT id, occurred_at, log_level, session_id, job_id, worker_id, log_source, content
            FROM logs
            WHERE session_id = $1
              AND (occurred_at, id) > ($2::timestamptz, $3::bigint)
            ORDER BY occurred_at ASC, id ASC
            LIMIT $4
            "#,
            )
            .bind(sid)
            .bind(c_at)
            .bind(c_id)
            .bind(fetch)
            .fetch_all(pool)
            .await
        }
        (None, None, None) => {
            sqlx::query_as(
                r#"
            SELECT id, occurred_at, log_level, session_id, job_id, worker_id, log_source, content
            FROM logs
            WHERE session_id = $1
            ORDER BY occurred_at ASC, id ASC
            LIMIT $2
            "#,
            )
            .bind(sid)
            .bind(fetch)
            .fetch_all(pool)
            .await
        }
    }
    .map_err(|e| {
        AuthError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            &format!("database error: {e}"),
        )
    })?;

    let has_more = rows.len() as i64 > limit;
    let mut rows = rows;
    if has_more {
        rows.pop();
    }

    let next_cursor = if has_more {
        rows.last()
            .map(|(lid, occurred_at, _, _, _, _, _, _)| encode_log_cursor(*occurred_at, *lid))
    } else {
        None
    };

    let items = rows
        .into_iter()
        .map(|(lid, ts, lvl, sess, jid, wid, src, msg)| LogEntry {
            id: lid.to_string(),
            timestamp: ts,
            level: lvl,
            session_id: sess.to_string(),
            job_id: jid.map(|u| u.to_string()),
            worker_id: wid,
            source: src,
            message: msg,
        })
        .collect();

    Ok(Json(PaginatedLogEntries { items, next_cursor }))
}

#[derive(Debug, Deserialize)]
pub struct DeleteSessionLogsQuery {
    pub job_id: Option<String>,
}

/// `DELETE /sessions/:id/logs`
pub async fn delete_session_logs(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<DeleteSessionLogsQuery>,
) -> Result<StatusCode, AuthError> {
    let Some(pool) = &state.db else {
        return Err(AuthError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "not_ready",
            "Database is not configured; cannot delete logs",
        ));
    };

    let sid = Uuid::parse_str(id.trim()).map_err(|_| {
        AuthError::new(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "session id must be a UUID",
        )
    })?;

    if !session_exists(pool, sid).await.map_err(|e| {
        AuthError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            &format!("database error: {e}"),
        )
    })? {
        return Err(AuthError::new(
            StatusCode::NOT_FOUND,
            "not_found",
            "Session not found",
        ));
    }

    if let Some(ref raw) = q.job_id {
        let jid = Uuid::parse_str(raw.trim()).map_err(|_| {
            AuthError::new(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "job_id must be a UUID",
            )
        })?;
        let n: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*)::bigint FROM jobs WHERE id = $1 AND session_id = $2"#,
        )
        .bind(jid)
        .bind(sid)
        .fetch_one(pool)
        .await
        .map_err(|e| {
            AuthError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                &format!("database error: {e}"),
            )
        })?;
        if n == 0 {
            return Err(AuthError::new(
                StatusCode::NOT_FOUND,
                "not_found",
                "Session or job not found",
            ));
        }
        sqlx::query(r#"DELETE FROM logs WHERE session_id = $1 AND job_id = $2"#)
            .bind(sid)
            .bind(jid)
            .execute(pool)
            .await
            .map_err(|e| {
                AuthError::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "server_error",
                    &format!("database error: {e}"),
                )
            })?;
    } else {
        sqlx::query(r#"DELETE FROM logs WHERE session_id = $1"#)
            .bind(sid)
            .execute(pool)
            .await
            .map_err(|e| {
                AuthError::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "server_error",
                    &format!("database error: {e}"),
                )
            })?;
    }

    Ok(StatusCode::NO_CONTENT)
}

/// Purge log rows older than `retention_days` unless session or job has `retain_forever`.
pub async fn run_log_retention_purge(
    pool: &PgPool,
    retention_days: u32,
) -> Result<u64, sqlx::Error> {
    let days = retention_days.max(1) as i64;
    let res = sqlx::query(
        r#"
        DELETE FROM logs l
        USING sessions s
        WHERE l.session_id = s.id
          AND l.occurred_at < (now() - ($1::bigint * interval '1 day'))
          AND s.retain_forever = false
          AND (
            l.job_id IS NULL
            OR EXISTS (
              SELECT 1 FROM jobs j
              WHERE j.id = l.job_id AND j.retain_forever = false
            )
          )
        "#,
    )
    .bind(days)
    .execute(pool)
    .await?;
    Ok(res.rows_affected())
}

fn parse_worker_timestamp(s: &str) -> Result<DateTime<Utc>, String> {
    DateTime::parse_from_rfc3339(s.trim())
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| e.to_string())
}

/// `POST /workers/tasks/:id/logs` — append batch for an assigned job.
pub async fn post_worker_task_logs(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    Json(body): Json<Vec<WorkerLogIngestItem>>,
) -> Result<(StatusCode, Json<WorkerLogsAcceptedResponse>), AuthError> {
    let Some(pool) = &state.db else {
        return Err(AuthError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "not_ready",
            "Database is not configured; cannot ingest logs",
        ));
    };

    let job_uuid = Uuid::parse_str(task_id.trim()).map_err(|_| {
        AuthError::new(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "task id must be a UUID",
        )
    })?;

    let mut tx = pool.begin().await.map_err(|e| {
        AuthError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            &format!("database error: {e}"),
        )
    })?;

    type JobRow = (Uuid, Option<String>);
    // Avoid `FOR UPDATE` on `jobs` here: with low pool sizes or certain interleavings, workers
    // could block indefinitely waiting on a row lock while the session poll path holds other work.
    // Line ordering is still protected by UNIQUE (session_id, line_seq) and per-batch sequencing.
    let job: Option<JobRow> = sqlx::query_as(
        r#"
        SELECT session_id, worker_id
        FROM jobs
        WHERE id = $1 AND status = 'assigned'
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

    let Some((session_id, worker_id)) = job else {
        return Err(AuthError::new(
            StatusCode::NOT_FOUND,
            "not_found",
            "Task not found or not assigned",
        ));
    };

    let max_seq: i64 = sqlx::query_scalar(
        r#"SELECT COALESCE(MAX(line_seq), 0)::bigint FROM logs WHERE session_id = $1"#,
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

    let mut seq = max_seq;
    let mut emitted: Vec<LogEntry> = Vec::with_capacity(body.len());
    for item in &body {
        let occurred_at = parse_worker_timestamp(&item.timestamp).map_err(|msg| {
            AuthError::new(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                &format!("invalid log timestamp (RFC 3339): {msg}"),
            )
        })?;
        let level = item.level.trim();
        if level.is_empty() {
            return Err(AuthError::new(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "log level must not be empty",
            ));
        }
        let source = item.source.trim();
        if source.is_empty() {
            return Err(AuthError::new(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "log source must not be empty",
            ));
        }
        seq += 1;
        type InsRow = (
            i64,
            DateTime<Utc>,
            String,
            Uuid,
            Option<Uuid>,
            Option<String>,
            String,
            String,
        );
        let row: InsRow = sqlx::query_as(
            r#"
            INSERT INTO logs (
                session_id, job_id, line_seq, stream, content,
                log_level, log_source, worker_id, occurred_at
            )
            VALUES ($1, $2, $3, 'stdout', $4, $5, $6, $7, $8)
            RETURNING id, occurred_at, log_level, session_id, job_id, worker_id, log_source, content
            "#,
        )
        .bind(session_id)
        .bind(job_uuid)
        .bind(seq)
        .bind(&item.message)
        .bind(level)
        .bind(source)
        .bind(worker_id.as_deref())
        .bind(occurred_at)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| {
            AuthError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                &format!("database error: {e}"),
            )
        })?;
        let (lid, ts, lvl, sess, jid, wid, src, msg) = row;
        emitted.push(LogEntry {
            id: lid.to_string(),
            timestamp: ts,
            level: lvl,
            session_id: sess.to_string(),
            job_id: jid.map(|u| u.to_string()),
            worker_id: wid,
            source: src,
            message: msg,
        });
    }

    tx.commit().await.map_err(|e| {
        AuthError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            &format!("database error: {e}"),
        )
    })?;

    for entry in emitted {
        state.sse.emit_log(session_id, entry);
    }

    Ok((
        StatusCode::ACCEPTED,
        Json(WorkerLogsAcceptedResponse { accepted: true }),
    ))
}
