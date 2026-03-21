//! SSE: log tail and session events ([`docs/API_OVERVIEW.md`](../../docs/API_OVERVIEW.md) §6–7).

use crate::auth::AuthError;
use crate::logs::session_exists;
use crate::logs::SessionLogsQuery;
use crate::AppState;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
};
use futures_util::stream::StreamExt;
use std::convert::Infallible;
use std::time::Duration;
use tokio_stream::wrappers::BroadcastStream;
use uuid::Uuid;

/// `GET /sessions/:id/logs/stream`
pub async fn stream_session_logs(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<SessionLogsQuery>,
) -> Result<
    Sse<impl futures_util::stream::Stream<Item = Result<Event, Infallible>> + Send>,
    AuthError,
> {
    let Some(pool) = &state.db else {
        return Err(AuthError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "not_ready",
            "Database is not configured; cannot stream logs",
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
        })?
        .map(|u| u.to_string());

    let level_filter = q
        .level
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_lowercase());

    let rx = state.sse.subscribe_logs();
    let stream = BroadcastStream::new(rx).filter_map(move |item| {
        let job_filter = job_filter.clone();
        let level_filter = level_filter.clone();
        async move {
            match item {
                Ok((session_id, entry)) if session_id == sid => {
                    if let Some(ref jf) = job_filter {
                        if entry.job_id.as_deref() != Some(jf.as_str()) {
                            return None;
                        }
                    }
                    if let Some(ref lv) = level_filter {
                        if entry.level.to_lowercase() != *lv {
                            return None;
                        }
                    }
                    let data = serde_json::to_string(&entry).ok()?;
                    Some(Ok(Event::default().event("log").data(data)))
                }
                _ => None,
            }
        }
    });

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(20))
            .text("keep-alive"),
    ))
}

/// `GET /sessions/:id/events`
pub async fn stream_session_events(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<
    Sse<impl futures_util::stream::Stream<Item = Result<Event, Infallible>> + Send>,
    AuthError,
> {
    let Some(pool) = &state.db else {
        return Err(AuthError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "not_ready",
            "Database is not configured; cannot stream session events",
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

    let rx = state.sse.subscribe_session_events();
    let stream = BroadcastStream::new(rx).filter_map(move |item| async move {
        match item {
            Ok((session_id, ev)) if session_id == sid => {
                let data = serde_json::to_string(&ev).ok()?;
                Some(Ok(Event::default().event("session_event").data(data)))
            }
            _ => None,
        }
    });

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(20))
            .text("keep-alive"),
    ))
}
