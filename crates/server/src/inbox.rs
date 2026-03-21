//! Agent inboxes: enqueue and list pending tasks ([`docs/API_OVERVIEW.md`](../../docs/API_OVERVIEW.md) §8).

use crate::auth::AuthError;
use crate::AppState;
use api_types::{InboxTaskItem, Paginated, PostAgentInboxRequest, PostAgentInboxResponse};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use base64::prelude::{Engine as _, BASE64_URL_SAFE_NO_PAD};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::Deserialize;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct ListInboxQuery {
    pub limit: Option<i64>,
    pub cursor: Option<String>,
}

pub(crate) fn validate_inbox_agent_id(aid: &str) -> Result<(), AuthError> {
    if aid.len() > 128 {
        return Err(AuthError::new(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "agent id is too long (max 128 characters)",
        ));
    }
    if !aid
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(AuthError::new(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "agent id must be ASCII letters, digits, underscore, or hyphen only",
        ));
    }
    Ok(())
}

fn encode_inbox_cursor(enqueued_at: DateTime<Utc>, id: Uuid) -> String {
    let raw = format!(
        "{}|{}",
        enqueued_at.to_rfc3339_opts(SecondsFormat::Millis, true),
        id
    );
    BASE64_URL_SAFE_NO_PAD.encode(raw.as_bytes())
}

fn parse_inbox_cursor(s: &str) -> Result<(DateTime<Utc>, Uuid), String> {
    let bytes = BASE64_URL_SAFE_NO_PAD
        .decode(s.as_bytes())
        .map_err(|e| e.to_string())?;
    let raw = String::from_utf8(bytes).map_err(|e| e.to_string())?;
    let (t, id) = raw
        .split_once('|')
        .ok_or_else(|| "expected '<rfc3339>|<uuid>'".to_string())?;
    let enqueued_at = DateTime::parse_from_rfc3339(t)
        .map_err(|e| e.to_string())?
        .with_timezone(&Utc);
    let id = Uuid::parse_str(id).map_err(|e| e.to_string())?;
    Ok((enqueued_at, id))
}

fn clamp_limit(n: Option<i64>) -> i64 {
    n.unwrap_or(20).clamp(1, 100)
}

/// Extract user-visible message text from POST /agents/:id/inbox body.payload.
pub(crate) fn inbox_payload_message(payload: &Value) -> Result<String, AuthError> {
    let Some(obj) = payload.as_object() else {
        return Err(AuthError::new(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "payload must be a JSON object",
        ));
    };
    if let Some(m) = obj
        .get("message")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return Ok(m.to_string());
    }
    if let Some(p) = obj
        .get("prompt")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return Ok(p.to_string());
    }
    Err(AuthError::new(
        StatusCode::BAD_REQUEST,
        "invalid_request",
        "payload must include a non-empty \"message\" or \"prompt\" string",
    ))
}

/// `POST /agents/:id/inbox`
pub async fn post_agent_inbox(
    State(state): State<AppState>,
    Path(agent_raw): Path<String>,
    Json(body): Json<PostAgentInboxRequest>,
) -> Result<(StatusCode, Json<PostAgentInboxResponse>), AuthError> {
    let Some(pool) = &state.db else {
        return Err(AuthError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "not_ready",
            "Database is not configured",
        ));
    };

    let agent_id = agent_raw.trim();
    validate_inbox_agent_id(agent_id)?;

    inbox_payload_message(&body.payload)?;

    let persona = body
        .persona_id
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    let mut tx = pool.begin().await.map_err(|e| {
        AuthError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            &format!("database error: {e}"),
        )
    })?;

    let agent_exists: bool = sqlx::query_scalar(
        r#"SELECT EXISTS(SELECT 1 FROM agents WHERE id = $1)"#,
    )
    .bind(agent_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        AuthError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            &format!("database error: {e}"),
        )
    })?;

    if !agent_exists {
        return Err(AuthError::new(
            StatusCode::NOT_FOUND,
            "not_found",
            "Agent id is unknown; create an inbox session with this agent_id first",
        ));
    }

    type SRow = (Uuid,);
    let session: Option<SRow> = sqlx::query_as(
        r#"
        SELECT id
        FROM sessions
        WHERE workflow = 'inbox'
          AND status = 'running'
          AND params->>'agent_id' = $1
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .bind(agent_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| {
        AuthError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            &format!("database error: {e}"),
        )
    })?;

    let Some((session_id,)) = session else {
        return Err(AuthError::new(
            StatusCode::CONFLICT,
            "no_inbox_session",
            "No running inbox session for this agent_id; create one with POST /sessions (workflow inbox)",
        ));
    };

    let task_id: (Uuid,) = sqlx::query_as(
        r#"
        INSERT INTO inbox_tasks (agent_id, payload, persona_id)
        VALUES ($1, $2, $3)
        RETURNING id
        "#,
    )
    .bind(agent_id)
    .bind(sqlx::types::Json(body.payload.clone()))
    .bind(persona.as_deref())
    .fetch_one(&mut *tx)
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

    state.sse.emit_session_event(
        session_id,
        crate::sse_hub::SessionEventPayload {
            event: "inbox_task_enqueued".to_string(),
            job_id: None,
            payload: serde_json::json!({ "task_id": task_id.0.to_string(), "agent_id": agent_id }),
        },
    );

    Ok((
        StatusCode::ACCEPTED,
        Json(PostAgentInboxResponse {
            task_id: task_id.0.to_string(),
        }),
    ))
}

/// `GET /agents/:id/inbox`
pub async fn get_agent_inbox(
    State(state): State<AppState>,
    Path(agent_raw): Path<String>,
    Query(q): Query<ListInboxQuery>,
) -> Result<Json<Paginated<InboxTaskItem>>, AuthError> {
    let Some(pool) = &state.db else {
        return Err(AuthError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "not_ready",
            "Database is not configured",
        ));
    };

    let agent_id = agent_raw.trim();
    validate_inbox_agent_id(agent_id)?;

    let exists: bool = sqlx::query_scalar(r#"SELECT EXISTS(SELECT 1 FROM agents WHERE id = $1)"#)
        .bind(agent_id)
        .fetch_one(pool)
        .await
        .map_err(|e| {
            AuthError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                &format!("database error: {e}"),
            )
        })?;

    if !exists {
        return Err(AuthError::new(
            StatusCode::NOT_FOUND,
            "not_found",
            "Agent id is unknown",
        ));
    }

    let limit = clamp_limit(q.limit);
    let after = match q.cursor.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(c) => Some(parse_inbox_cursor(c).map_err(|msg| {
            AuthError::new(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                &format!("invalid cursor: {msg}"),
            )
        })?),
        None => None,
    };

    list_inbox_pending_page(pool, agent_id, limit, after).await
}

async fn list_inbox_pending_page(
    pool: &PgPool,
    agent_id: &str,
    limit: i64,
    after: Option<(DateTime<Utc>, Uuid)>,
) -> Result<Json<Paginated<InboxTaskItem>>, AuthError> {
    type Row = (Uuid, sqlx::types::Json<Value>, DateTime<Utc>);
    let fetch = limit + 1;
    let rows: Vec<Row> = if let Some((ts, id)) = after {
        sqlx::query_as(
            r#"
            SELECT id, payload, enqueued_at
            FROM inbox_tasks
            WHERE agent_id = $1
              AND status = 'pending'
              AND (enqueued_at, id) > ($2, $3)
            ORDER BY enqueued_at ASC, id ASC
            LIMIT $4
            "#,
        )
        .bind(agent_id)
        .bind(ts)
        .bind(id)
        .bind(fetch)
        .fetch_all(pool)
        .await
    } else {
        sqlx::query_as(
            r#"
            SELECT id, payload, enqueued_at
            FROM inbox_tasks
            WHERE agent_id = $1 AND status = 'pending'
            ORDER BY enqueued_at ASC, id ASC
            LIMIT $2
            "#,
        )
        .bind(agent_id)
        .bind(fetch)
        .fetch_all(pool)
        .await
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
            .map(|(id, _, enqueued_at)| encode_inbox_cursor(*enqueued_at, *id))
    } else {
        None
    };
    let items = rows
        .into_iter()
        .map(|(id, payload, enqueued_at)| InboxTaskItem {
            task_id: id.to_string(),
            payload: payload.0,
            enqueued_at,
        })
        .collect();

    Ok(Json(Paginated { items, next_cursor }))
}
