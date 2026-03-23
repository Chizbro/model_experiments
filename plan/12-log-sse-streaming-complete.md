# 12 - Log & Session Event SSE Streaming

## Goal
Implement Server-Sent Events (SSE) for real-time log streaming and session lifecycle events. Both CLI and Web UI will consume these streams.

## What to build

### SSE infrastructure (`crates/server/src/sse.rs`)
- Use `tokio::sync::broadcast` channels for fan-out
- Per-session broadcast channel for log events
- Per-session broadcast channel for session lifecycle events
- Channel management: create on first subscriber, clean up when session completes

### Log stream endpoint (`crates/server/src/routes/logs.rs`)

**GET /sessions/:id/logs/stream**
- Query: optional `job_id`, `level` filter
- Response: `200`, `Content-Type: text/event-stream`
- Each event:
  ```
  event: log
  data: {"id":"...","timestamp":"...","level":"info","session_id":"...","message":"..."}
  ```
- Keep connection open; server sends events as logs arrive (from worker POST or control plane self-logging)
- When session reaches terminal state (completed/failed), send a final event and close
- Handle client disconnect gracefully

### Session events endpoint (`crates/server/src/routes/sessions.rs`)

**GET /sessions/:id/events**
- Response: `200`, `Content-Type: text/event-stream`
- Events:
  ```
  event: session_event
  data: {"event":"started|job_started|job_completed|completed|failed","job_id":"...","payload":{}}
  ```
- Emitted when: session starts, job assigned, job completed, session completed/failed
- Close stream on terminal session state

### Publishing events
- When log batch is received (POST /workers/tasks/:id/logs), publish each entry to the session's log broadcast channel
- When job/session state changes, publish to session event broadcast channel
- Control plane self-logs also published to the appropriate session channel

### SSE helpers
- `axum::response::Sse` with `tokio_stream`
- Heartbeat/keepalive: send `:keepalive\n\n` comment every 15s to prevent proxy timeouts
- Handle `Last-Event-ID` header for potential reconnect (v1: not required but good to have)

## Dependencies
- Task 11 (log ingestion — logs must be stored before streaming)
- Task 09 (session state machine — for session event triggers)

## Test criteria
- [ ] `GET /sessions/:id/logs/stream` returns `Content-Type: text/event-stream`
- [ ] Log events appear in stream when worker sends logs via POST
- [ ] Stream filters by `job_id` and `level` when specified
- [ ] Stream closes when session reaches terminal state
- [ ] `GET /sessions/:id/events` streams lifecycle events
- [ ] Session event fires on job_started, job_completed, session completed/failed
- [ ] Keepalive comments sent on idle connections
- [ ] Multiple concurrent subscribers to same session receive events
- [ ] Client disconnect doesn't crash the server
- [ ] Integration test: create session, subscribe to stream, post logs, verify events received
- [ ] `cargo test -p server` passes
