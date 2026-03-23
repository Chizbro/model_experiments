# 005: Log History, SSE Streaming & Session Events

## Goal
Full log pipeline: history API (paginated), SSE streaming for live logs, SSE for session events, and the delete logs endpoint. CLI and Web UI will consume these — this spec builds the server side.

## Scope
### Server endpoints
- `GET /sessions/:id/logs` — Paginated log history. Supports query params: limit, cursor, job_id, level, last (most recent N). Order by timestamp ASC.
- `GET /sessions/:id/logs/stream` — SSE endpoint. Filter by optional job_id, level. Each event: type `log`, data is JSON log entry. Keep connection open; send events as logs arrive via broadcast channel.
- `DELETE /sessions/:id/logs` — Delete logs. Optional query param job_id (delete only that job's logs). Return 204.
- `GET /sessions/:id/events` — SSE endpoint for session lifecycle events. Event type `session_event`, data is JSON with event name (started, job_started, job_completed, completed, failed) and job_id.

### SSE infrastructure
- `LogBroadcaster`: wraps `tokio::broadcast::Sender<LogEntry>`. When logs are inserted (via worker POST or server-generated), also send to broadcaster. SSE handler subscribes and filters by session_id/job_id/level.
- `EventBroadcaster`: wraps `tokio::broadcast::Sender<SessionEvent>`. Engine sends events on state transitions. SSE handler subscribes and filters by session_id.
- Both use axum's `Sse` response type with `Event::default().event("log").json_data(entry)`.

### Log route file
- `src/routes/logs.rs` — GET history, GET stream, DELETE

## Prerequisites
- Spec 001 (foundation, SSE broadcaster stubs)
- Spec 004 (logs are inserted via task log endpoint)

## Files to create/modify
- `crates/server/src/routes/logs.rs` — New: history, stream, delete endpoints
- `crates/server/src/routes/mod.rs` — Mount log routes
- `crates/server/src/sse.rs` — Complete LogBroadcaster and EventBroadcaster implementations
- `crates/server/src/routes/sessions.rs` — Add events SSE endpoint (or mount under sessions)
- `crates/server/src/routes/workers.rs` — Ensure send_logs handler broadcasts to LogBroadcaster after DB insert

## Acceptance criteria
1. `GET /sessions/:id/logs` → paginated log entries, ordered by timestamp
2. `GET /sessions/:id/logs?job_id=X` → filtered to that job
3. `GET /sessions/:id/logs?level=error` → filtered to errors
4. `GET /sessions/:id/logs?last=50` → most recent 50 entries, no cursor
5. Cursor-based pagination works: first page returns next_cursor, second page uses it
6. `GET /sessions/:id/logs/stream` → SSE connection, receives new log events in real time
7. SSE stream filters by job_id and level when provided
8. `DELETE /sessions/:id/logs` → 204, logs removed from DB
9. `DELETE /sessions/:id/logs?job_id=X` → only that job's logs deleted
10. `GET /sessions/:id/events` → SSE connection, receives session lifecycle events
11. When a job is assigned/completed/failed, corresponding session_event is emitted
12. `cargo test` — at least 4 tests (history pagination, filtering, delete, event emission)
13. `cargo clippy` clean

## Implementation notes
- Cursor for logs: use log.id (UUID) as cursor; query WHERE id > cursor ORDER BY timestamp ASC LIMIT N
- For `last` parameter: query ORDER BY timestamp DESC LIMIT N, then reverse in application code
- SSE connections should have a reasonable buffer (e.g., broadcast channel capacity 1000)
- When broadcasting, clone the entry; subscribers that fall behind get `RecvError::Lagged` and should skip
- Session events should be fired from the engine module (state transitions in spec 004)
