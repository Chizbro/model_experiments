# 24 — Web: logs (history + SSE), attach, chat input

**Status:** pending  
**Dependencies:** 23, 14, 15

## Objective

**Client contract** for logs: load **all** history pages first, then **`logs/stream` SSE**; backoff on disconnect ([CLIENT_EXPERIENCE §4](../docs/CLIENT_EXPERIENCE.md#4-sse-logs-and-session-events)). **Attach** uses `GET /sessions/:id/events` similarly. Chat **POST input** from UI.

## Scope

**In scope**

- Reconnect with exponential backoff + max cap; show “Reconnecting…”.
- **Delete logs** with confirm ([CLIENT_EXPERIENCE §9](../docs/CLIENT_EXPERIENCE.md#9-log-retention-and-purge)).
- `PATCH` **retain_forever** on session/job.

**Out of scope**

- Copy for truncation banner (25).

## Spec references

- [API_OVERVIEW §6–7](../docs/API_OVERVIEW.md)

## Acceptance criteria

- Mock SSE in tests or manual checklist; verify no duplicate history loss on reconnect (re-fetch policy documented).

## Testing

| When | What | Retest |
|------|------|--------|
| After implementation | Component/integration tests | CI |
