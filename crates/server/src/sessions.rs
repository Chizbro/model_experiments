//! Sessions API and job enqueue: chat, `loop_n`, `loop_until_sentinel`, `inbox` ([`docs/API_OVERVIEW.md`](../../docs/API_OVERVIEW.md) §4).

use crate::auth::AuthError;
use crate::identities::session_identity_tokens_sufficient;
use crate::inbox::validate_inbox_agent_id;
use crate::AppState;
use api_types::{
    CreateSessionRequest, CreateSessionResponse, Paginated, PatchSessionRetainRequest,
    SendSessionInputRequest, SendSessionInputResponse, SessionDetailResponse, SessionJobSummary,
    SessionSummary,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use base64::prelude::{Engine as _, BASE64_URL_SAFE_NO_PAD};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct SessionListQuery {
    pub limit: Option<i64>,
    pub cursor: Option<String>,
    pub status: Option<String>,
}

fn encode_cursor(created_at: DateTime<Utc>, id: Uuid) -> String {
    let raw = format!(
        "{}|{}",
        created_at.to_rfc3339_opts(SecondsFormat::Millis, true),
        id
    );
    BASE64_URL_SAFE_NO_PAD.encode(raw.as_bytes())
}

fn parse_cursor(s: &str) -> Result<(DateTime<Utc>, Uuid), String> {
    let bytes = BASE64_URL_SAFE_NO_PAD
        .decode(s.as_bytes())
        .map_err(|e| e.to_string())?;
    let raw = String::from_utf8(bytes).map_err(|e| e.to_string())?;
    let (t, id) = raw
        .split_once('|')
        .ok_or_else(|| "expected '<rfc3339>|<uuid>'".to_string())?;
    let created_at = DateTime::parse_from_rfc3339(t)
        .map_err(|e| e.to_string())?
        .with_timezone(&Utc);
    let id = Uuid::parse_str(id).map_err(|e| e.to_string())?;
    Ok((created_at, id))
}

fn clamp_limit(n: Option<i64>) -> i64 {
    let n = n.unwrap_or(20);
    n.clamp(1, 100)
}

/// `POST /sessions`
pub async fn create_session(
    State(state): State<AppState>,
    Json(body): Json<CreateSessionRequest>,
) -> Result<(StatusCode, Json<CreateSessionResponse>), AuthError> {
    let Some(pool) = &state.db else {
        return Err(AuthError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "not_ready",
            "Database is not configured; cannot create sessions",
        ));
    };

    let wf = body.workflow.trim();
    if wf != "chat" && wf != "loop_n" && wf != "loop_until_sentinel" && wf != "inbox" {
        return Err(AuthError::new(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            r#"workflow must be "chat", "loop_n", "loop_until_sentinel", or "inbox""#,
        ));
    }

    let agent_cli = body
        .params
        .get("agent_cli")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let Some(ac) = agent_cli else {
        return Err(AuthError::new(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "params.agent_cli is required",
        ));
    };
    if ac != "claude_code" && ac != "cursor" {
        return Err(AuthError::new(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            r#"params.agent_cli must be "claude_code" or "cursor""#,
        ));
    }

    let prompt_s: String = if wf == "inbox" {
        let aid = body
            .params
            .get("agent_id")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let Some(aid) = aid else {
            return Err(AuthError::new(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "params.agent_id is required for inbox workflow",
            ));
        };
        validate_inbox_agent_id(aid)?;
        String::new()
    } else {
        let prompt = body
            .params
            .get("prompt")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let Some(p) = prompt else {
            return Err(AuthError::new(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "params.prompt is required for this workflow",
            ));
        };
        p.to_string()
    };

    let repo = body.repo_url.trim();
    if repo.is_empty() {
        return Err(AuthError::new(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "repo_url must not be empty",
        ));
    }

    let identity_id = body
        .identity_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("default")
        .to_string();

    let id_exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM identities WHERE id = $1)")
            .bind(&identity_id)
            .fetch_one(pool)
            .await
            .map_err(|e| {
                AuthError::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "server_error",
                    &format!("database error: {e}"),
                )
            })?;
    if !id_exists {
        return Err(AuthError::new(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "identity_id does not exist",
        ));
    }

    let tokens_ok = session_identity_tokens_sufficient(pool, &identity_id, &body.params)
        .await
        .map_err(|e| {
            AuthError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                &format!("database error: {e}"),
            )
        })?;
    if !tokens_ok {
        return Err(AuthError::new(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "Identity and session params do not provide both agent_token and git_token",
        ));
    }

    let git_ref = body
        .git_ref
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("main")
        .to_string();

    let retain = body.retain_forever.unwrap_or(false);

    /// Cap `loop_n` to avoid accidental huge fan-out.
    const LOOP_N_MAX: i64 = 10_000;

    let job_plan: Vec<(Value, i32)> = if wf == "chat" {
        vec![(json!({ "prompt": prompt_s }), 0)]
    } else if wf == "inbox" {
        vec![]
    } else if wf == "loop_n" {
        let n_raw = body
            .params
            .get("n")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| {
                AuthError::new(
                    StatusCode::BAD_REQUEST,
                    "invalid_request",
                    "params.n must be a positive integer for loop_n workflow",
                )
            })?;
        if !(1..=LOOP_N_MAX).contains(&n_raw) {
            return Err(AuthError::new(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                &format!("params.n must be between 1 and {LOOP_N_MAX} for loop_n workflow"),
            ));
        }
        (1..=n_raw)
            .map(|i| {
                (
                    json!({
                        "prompt": prompt_s,
                        "iteration": i,
                        "iteration_total": n_raw,
                    }),
                    i as i32,
                )
            })
            .collect()
    } else if wf == "loop_until_sentinel" {
        let sentinel = body
            .params
            .get("sentinel")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let Some(sent) = sentinel else {
            return Err(AuthError::new(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "params.sentinel must be a non-empty string for loop_until_sentinel workflow",
            ));
        };
        if sent.len() > 8192 {
            return Err(AuthError::new(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "params.sentinel is too long (max 8192 characters)",
            ));
        }
        vec![(
            json!({
                "prompt": prompt_s,
                "iteration": 1_i64,
            }),
            1,
        )]
    } else {
        return Err(AuthError::new(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "unsupported workflow for job planning",
        ));
    };

    let initial_session_status = if wf == "inbox" { "running" } else { "pending" };

    let mut tx = pool.begin().await.map_err(|e| {
        AuthError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            &format!("database error: {e}"),
        )
    })?;

    if wf == "inbox" {
        let aid = body
            .params
            .get("agent_id")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let Some(aid) = aid else {
            return Err(AuthError::new(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "params.agent_id is required for inbox workflow",
            ));
        };
        sqlx::query(r#"INSERT INTO agents (id) VALUES ($1) ON CONFLICT DO NOTHING"#)
            .bind(aid)
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                AuthError::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "server_error",
                    &format!("database error: {e}"),
                )
            })?;
    }

    let row: (Uuid,) = sqlx::query_as(
        r#"
        INSERT INTO sessions (
            identity_id, repo_url, git_ref, workflow, status, params, persona_id, retain_forever
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        RETURNING id
        "#,
    )
    .bind(&identity_id)
    .bind(repo)
    .bind(&git_ref)
    .bind(wf)
    .bind(initial_session_status)
    .bind(sqlx::types::Json(body.params.clone()))
    .bind(
        body.persona_id
            .as_ref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty()),
    )
    .bind(retain)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        AuthError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            &format!("database error: {e}"),
        )
    })?;

    let sid = row.0;

    for (task_input, queue_ordinal) in job_plan {
        sqlx::query(
            r#"
            INSERT INTO jobs (session_id, status, task_input, retain_forever, queue_ordinal)
            VALUES ($1, 'pending', $2, $3, $4)
            "#,
        )
        .bind(sid)
        .bind(sqlx::types::Json(task_input))
        .bind(retain)
        .bind(queue_ordinal)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            AuthError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                &format!("database error: {e}"),
            )
        })?;
    }

    tx.commit().await.map_err(|e| {
        AuthError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            &format!("database error: {e}"),
        )
    })?;

    let web_url = state
        .config
        .web_ui_base_url
        .as_ref()
        .map(|base| format!("{}/sessions/{}", base.trim_end_matches('/'), sid));

    Ok((
        StatusCode::CREATED,
        Json(CreateSessionResponse {
            session_id: sid.to_string(),
            status: initial_session_status.to_string(),
            web_url,
        }),
    ))
}

/// `GET /sessions`
pub async fn list_sessions(
    State(state): State<AppState>,
    Query(q): Query<SessionListQuery>,
) -> Result<Json<Paginated<SessionSummary>>, AuthError> {
    let Some(pool) = &state.db else {
        return Err(AuthError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "not_ready",
            "Database is not configured; cannot list sessions",
        ));
    };

    let limit = clamp_limit(q.limit);
    let fetch = limit + 1;
    let cursor = q
        .cursor
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(parse_cursor)
        .transpose()
        .map_err(|msg| {
            AuthError::new(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                &format!("invalid cursor: {msg}"),
            )
        })?;

    let st_filter = q
        .status
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    type Row = (Uuid, String, String, String, String, DateTime<Utc>);
    let rows: Vec<Row> = if let Some((c_at, c_id)) = cursor {
        if let Some(ref st) = st_filter {
            sqlx::query_as(
                r#"
                SELECT id, repo_url, git_ref, workflow, status, created_at
                FROM sessions
                WHERE (created_at, id) < ($1::timestamptz, $2::uuid)
                  AND status = $3
                ORDER BY created_at DESC, id DESC
                LIMIT $4
                "#,
            )
            .bind(c_at)
            .bind(c_id)
            .bind(st)
            .bind(fetch)
            .fetch_all(pool)
            .await
        } else {
            sqlx::query_as(
                r#"
                SELECT id, repo_url, git_ref, workflow, status, created_at
                FROM sessions
                WHERE (created_at, id) < ($1::timestamptz, $2::uuid)
                ORDER BY created_at DESC, id DESC
                LIMIT $3
                "#,
            )
            .bind(c_at)
            .bind(c_id)
            .bind(fetch)
            .fetch_all(pool)
            .await
        }
    } else if let Some(ref st) = st_filter {
        sqlx::query_as(
            r#"
            SELECT id, repo_url, git_ref, workflow, status, created_at
            FROM sessions
            WHERE status = $1
            ORDER BY created_at DESC, id DESC
            LIMIT $2
            "#,
        )
        .bind(st)
        .bind(fetch)
        .fetch_all(pool)
        .await
    } else {
        sqlx::query_as(
            r#"
            SELECT id, repo_url, git_ref, workflow, status, created_at
            FROM sessions
            ORDER BY created_at DESC, id DESC
            LIMIT $1
            "#,
        )
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
            .map(|(id, _, _, _, _, created_at)| encode_cursor(*created_at, *id))
    } else {
        None
    };

    let items = rows
        .into_iter()
        .map(
            |(id, repo_url, git_ref, workflow, status, created_at)| SessionSummary {
                session_id: id.to_string(),
                repo_url,
                git_ref,
                workflow,
                status,
                created_at,
            },
        )
        .collect();

    Ok(Json(Paginated { items, next_cursor }))
}

/// `GET /sessions/:id`
pub async fn get_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<SessionDetailResponse>, AuthError> {
    let Some(pool) = &state.db else {
        return Err(AuthError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "not_ready",
            "Database is not configured; cannot load sessions",
        ));
    };

    let sid = Uuid::parse_str(id.trim()).map_err(|_| {
        AuthError::new(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "session id must be a UUID",
        )
    })?;

    type SRow = (
        Uuid,
        String,
        String,
        String,
        String,
        sqlx::types::Json<Value>,
        DateTime<Utc>,
        DateTime<Utc>,
        bool,
    );
    let session: Option<SRow> = sqlx::query_as(
        r#"
        SELECT id, repo_url, git_ref, workflow, status, params, created_at, updated_at, retain_forever
        FROM sessions
        WHERE id = $1
        "#,
    )
    .bind(sid)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        AuthError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            &format!("database error: {e}"),
        )
    })?;

    let Some((
        id,
        repo_url,
        git_ref,
        workflow,
        status,
        params,
        created_at,
        updated_at,
        session_retain_forever,
    )) = session
    else {
        return Err(AuthError::new(
            StatusCode::NOT_FOUND,
            "not_found",
            "Session not found",
        ));
    };

    type JRow = (
        Uuid,
        String,
        DateTime<Utc>,
        Option<String>,
        Option<String>,
        Option<String>,
        bool,
    );
    let job_rows: Vec<JRow> = sqlx::query_as(
        r#"
        SELECT id, status, created_at, error_message, pull_request_url, commit_ref, retain_forever
        FROM jobs
        WHERE session_id = $1
        ORDER BY created_at ASC
        "#,
    )
    .bind(sid)
    .fetch_all(pool)
    .await
    .map_err(|e| {
        AuthError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            &format!("database error: {e}"),
        )
    })?;

    let jobs = job_rows
        .into_iter()
        .map(
            |(jid, jstatus, jcreated, err, pr, cref, jretain)| SessionJobSummary {
                job_id: jid.to_string(),
                status: jstatus,
                created_at: jcreated,
                error_message: err,
                pull_request_url: pr,
                commit_ref: cref,
                retain_forever: jretain,
            },
        )
        .collect();

    let (chat_history_truncated, chat_history_max_turns) = chat_session_history_flags_for_detail(
        pool,
        workflow.as_str(),
        id,
        state.config.chat_history_max_turns,
    )
    .await
    .map_err(|e| {
        AuthError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            &format!("database error: {e}"),
        )
    })?;

    Ok(Json(SessionDetailResponse {
        session_id: id.to_string(),
        repo_url,
        git_ref,
        workflow,
        status,
        params: params.0,
        jobs,
        created_at,
        updated_at,
        retain_forever: session_retain_forever,
        chat_history_truncated,
        chat_history_max_turns,
    }))
}

/// `DELETE /sessions/:id`
pub async fn delete_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, AuthError> {
    let Some(pool) = &state.db else {
        return Err(AuthError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "not_ready",
            "Database is not configured; cannot delete sessions",
        ));
    };

    let sid = Uuid::parse_str(id.trim()).map_err(|_| {
        AuthError::new(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "session id must be a UUID",
        )
    })?;

    let res = sqlx::query("DELETE FROM sessions WHERE id = $1")
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

    if res.rows_affected() == 0 {
        return Err(AuthError::new(
            StatusCode::NOT_FOUND,
            "not_found",
            "Session not found",
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}

/// `PATCH /sessions/:id`
pub async fn patch_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<PatchSessionRetainRequest>,
) -> Result<StatusCode, AuthError> {
    let Some(pool) = &state.db else {
        return Err(AuthError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "not_ready",
            "Database is not configured; cannot update sessions",
        ));
    };

    let sid = Uuid::parse_str(id.trim()).map_err(|_| {
        AuthError::new(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "session id must be a UUID",
        )
    })?;

    let res = sqlx::query(
        r#"
        UPDATE sessions
        SET retain_forever = $2, updated_at = now()
        WHERE id = $1
        "#,
    )
    .bind(sid)
    .bind(body.retain_forever)
    .execute(pool)
    .await
    .map_err(|e| {
        AuthError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            &format!("database error: {e}"),
        )
    })?;

    if res.rows_affected() == 0 {
        return Err(AuthError::new(
            StatusCode::NOT_FOUND,
            "not_found",
            "Session not found",
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}

/// `PATCH /sessions/:id/jobs/:job_id`
pub async fn patch_session_job(
    State(state): State<AppState>,
    Path((session_id, job_id)): Path<(String, String)>,
    Json(body): Json<PatchSessionRetainRequest>,
) -> Result<StatusCode, AuthError> {
    let Some(pool) = &state.db else {
        return Err(AuthError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "not_ready",
            "Database is not configured; cannot update jobs",
        ));
    };

    let sid = Uuid::parse_str(session_id.trim()).map_err(|_| {
        AuthError::new(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "session id must be a UUID",
        )
    })?;
    let jid = Uuid::parse_str(job_id.trim()).map_err(|_| {
        AuthError::new(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "job id must be a UUID",
        )
    })?;

    let res = sqlx::query(
        r#"
        UPDATE jobs
        SET retain_forever = $3, updated_at = now()
        WHERE id = $2 AND session_id = $1
        "#,
    )
    .bind(sid)
    .bind(jid)
    .bind(body.retain_forever)
    .execute(pool)
    .await
    .map_err(|e| {
        AuthError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            &format!("database error: {e}"),
        )
    })?;

    if res.rows_affected() == 0 {
        return Err(AuthError::new(
            StatusCode::NOT_FOUND,
            "not_found",
            "Session or job not found",
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}

pub(crate) async fn chat_histories_for_session(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    session_id: Uuid,
) -> Result<(Vec<String>, Vec<String>), sqlx::Error> {
    type HRow = (Option<sqlx::types::Json<Value>>, Option<String>);
    let rows: Vec<HRow> = sqlx::query_as(
        r#"
        SELECT task_input, assistant_reply
        FROM jobs
        WHERE session_id = $1 AND status = 'completed'
        ORDER BY created_at ASC
        "#,
    )
    .bind(session_id)
    .fetch_all(&mut **tx)
    .await?;

    let mut history: Vec<String> = Vec::new();
    let mut history_assistant: Vec<String> = Vec::new();

    for (ti, ar) in rows {
        if let Some(j) = ti {
            if let Some(m) = j.0.get("message").and_then(|v| v.as_str()) {
                let m = m.trim();
                if !m.is_empty() {
                    history.push(m.to_string());
                }
            }
        }
        if let Some(a) = ar {
            let t = a.trim();
            if !t.is_empty() {
                history_assistant.push(t.to_string());
            }
        }
    }

    Ok((history, history_assistant))
}

/// For `GET /sessions/:id`: whether the next chat pull would cap history (same rule as worker pull).
async fn chat_session_history_flags_for_detail(
    pool: &sqlx::PgPool,
    workflow: &str,
    session_id: Uuid,
    max_turns: u32,
) -> Result<(bool, Option<u32>), sqlx::Error> {
    if workflow != "chat" {
        return Ok((false, None));
    }
    if max_turns == 0 {
        return Ok((false, Some(0)));
    }
    let mut tx = pool.begin().await?;
    let (h, ha) = chat_histories_for_session(&mut tx, session_id).await?;
    tx.commit().await?;
    let truncated = h.len() > max_turns as usize || ha.len() > max_turns as usize;
    Ok((truncated, Some(max_turns)))
}

/// `POST /sessions/:id/input`
pub async fn post_session_input(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<SendSessionInputRequest>,
) -> Result<(StatusCode, Json<SendSessionInputResponse>), AuthError> {
    let Some(pool) = &state.db else {
        return Err(AuthError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "not_ready",
            "Database is not configured; cannot accept input",
        ));
    };

    let sid = Uuid::parse_str(id.trim()).map_err(|_| {
        AuthError::new(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "session id must be a UUID",
        )
    })?;

    let msg = body.message.trim();
    if msg.is_empty() {
        return Err(AuthError::new(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "message must not be empty",
        ));
    }

    let mut tx = pool.begin().await.map_err(|e| {
        AuthError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            &format!("database error: {e}"),
        )
    })?;

    type LRow = (String, String, sqlx::types::Json<Value>, bool);
    let session: Option<LRow> = sqlx::query_as(
        r#"
        SELECT workflow, status, params, retain_forever
        FROM sessions
        WHERE id = $1
        FOR UPDATE
        "#,
    )
    .bind(sid)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| {
        AuthError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            &format!("database error: {e}"),
        )
    })?;

    let Some((workflow, session_status, params, session_retain)) = session else {
        return Err(AuthError::new(
            StatusCode::NOT_FOUND,
            "not_found",
            "Session not found",
        ));
    };

    if workflow != "chat" {
        return Err(AuthError::new(
            StatusCode::CONFLICT,
            "conflict",
            "Session workflow does not accept chat input (use POST /agents/:id/inbox for inbox workflow)",
        ));
    }

    if session_status != "running" {
        return Err(AuthError::new(
            StatusCode::CONFLICT,
            "conflict",
            "Session is not accepting input (must be running)",
        ));
    }

    let active: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM jobs
        WHERE session_id = $1 AND status IN ('pending', 'assigned')
        "#,
    )
    .bind(sid)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        AuthError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            &format!("database error: {e}"),
        )
    })?;

    if active > 0 {
        return Err(AuthError::new(
            StatusCode::CONFLICT,
            "conflict",
            "A job is already pending or in progress for this session",
        ));
    }

    let session_prompt = params
        .0
        .get("prompt")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let (history, history_assistant) =
        chat_histories_for_session(&mut tx, sid)
            .await
            .map_err(|e| {
                AuthError::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "server_error",
                    &format!("database error: {e}"),
                )
            })?;

    let task_input = json!({
        "session_prompt": session_prompt,
        "message": msg,
        "history": history,
        "history_assistant": history_assistant,
        "history_truncated": false
    });

    sqlx::query(
        r#"
        INSERT INTO jobs (session_id, status, task_input, retain_forever, queue_ordinal)
        SELECT $1, 'pending', $2, $3, COALESCE(MAX(queue_ordinal), 0) + 1
        FROM jobs WHERE session_id = $1
        "#,
    )
    .bind(sid)
    .bind(sqlx::types::Json(task_input))
    .bind(session_retain)
    .execute(&mut *tx)
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

    Ok((
        StatusCode::ACCEPTED,
        Json(SendSessionInputResponse { accepted: true }),
    ))
}
