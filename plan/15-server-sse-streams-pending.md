# 15 — Server: SSE for logs and session events

**Status:** pending  
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
