# 15 — Server: SSE for logs and session events

**Status:** complete  
**Dependencies:** 14, 11

## Objective

**SSE endpoints** per [API_OVERVIEW §6–7](../docs/API_OVERVIEW.md): `GET /sessions/:id/logs/stream` (`event: log`) and `GET /sessions/:id/events` (`event: session_event`). Implement a **clean** broadcast/subscription layer (channels, tokio broadcast, or equivalent) rather than ad-hoc per-request state.

## Scope

**In scope**

- Emit events when new logs arrive and when session/job state changes (`started`, `job_completed`, …).
- Correct `Content-Type: text/event-stream` and heartbeat if needed.

**Out of scope**

- CLI/Web reconnection UX (clients—tasks 21, 24) but server should tolerate disconnects.

## Spec references

- [API_OVERVIEW §6–7](../docs/API_OVERVIEW.md)
- [CLIENT_EXPERIENCE §4](../docs/CLIENT_EXPERIENCE.md#4-sse-logs-and-session-events)

## Acceptance criteria

- Integration test: connect SSE, trigger log write, receive `log` event JSON matching REST item shape.
- SSE shapes documented per task 02 decision (OpenAPI or `docs/SSE_EVENTS.md`).

## Testing

| When | What | Retest |
|------|------|--------|
| After implementation | `cargo test` (httptest or hyper client) | CI |

---

## Completed / Notes

- `crates/server/src/sse_hub.rs` — `SseHub` with two `tokio::sync::broadcast` channels (capacity 4096); `SessionEventPayload` for lifecycle JSON.
- `crates/server/src/sse.rs` — `stream_session_logs` and `stream_session_events`; `BroadcastStream` + async `filter_map` by session id; optional `job_id` / `level` on log stream; `KeepAlive` comment lines every 20s.
- `AppState.sse` wired in `lib.rs`; routes under API-key middleware.
- `logs::post_worker_task_logs` — `INSERT … RETURNING` builds `LogEntry` per line; emits after commit.
- `worker_tasks::pull_task` — after commit, emits `started` (when session leaves `pending`) then `job_started`; `complete_task` emits `job_completed` and `completed` / `failed` when session status matches.
- `docs/SSE_EVENTS.md`, `openapi.yaml` (`streamSessionLogs`, `streamSessionEvents`), `ARCHITECTURE.md`, `GETTING_STARTED.md` updated.
- Integration: `sessions_integration::sse_log_stream_receives_ingested_log_matching_rest_shape`.
