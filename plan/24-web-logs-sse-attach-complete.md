# 24 — Web: logs (history + SSE), attach, chat input

**Status:** complete  
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

## Completed / Notes

- **Web:** `SessionDetailPage` loads paginated log history (`fetchAllSessionLogs`), then `logs/stream` via `useSessionLogsStream` (fetch + `ReadableStream` + `SseLineBuffer`). Exponential backoff to 30s; **“Reconnecting…”** banner; on reconnect **re-fetch + merge by `id`** (`mergeAndSortLogs`). **`events` SSE** via `useSessionEventsStream`. **Delete logs** with `window.confirm`. **PATCH** retain toggles for session and each job. **Chat** `POST /sessions/:id/input` when `workflow === chat` and `status === running`. Streams stop when session is `completed` or `failed`.
- **Tests:** `sseParse.test.ts`, `logMerge.test.ts` (reconnect dedupe), `sseBackoff.test.ts`.
- **API surface:** `GET /sessions/:id` now includes **`retain_forever`** on the session and each **job** so the UI can reflect server state (`api-types`, server `get_session`, OpenAPI, `API_OVERVIEW.md`). **CLI** `sessions get` prints these fields.
- **Docs:** `CLIENT_EXPERIENCE.md` §4 (Web reconnect refetch policy), §7 (implementation pointer to this plan).
