# SSE events (control plane)

Long-lived **Server-Sent Events** streams are documented here. REST request and response bodies live in [`openapi.yaml`](../crates/server/openapi.yaml) and [`API_OVERVIEW.md`](API_OVERVIEW.md); this file is the **single companion** for SSE payload shapes so OpenAPI can stay focused on `application/json` routes.

## Transport

- **Response:** `200 OK`, `Content-Type: text/event-stream`.
- **Framing:** Standard SSE (`event:` + `data:` lines, blank line between events). Clients should reconnect after disconnect ([`CLIENT_EXPERIENCE.md`](CLIENT_EXPERIENCE.md#4-sse-logs-and-session-events)).
- **Heartbeat:** The server sends periodic **comment** lines (`: keep-alive`) so proxies and browsers do not close idle connections.

## Streams (v1)

| Stream | Method & path | SSE `event` | `data` JSON |
|--------|----------------|---------------|-------------|
| Log tail | `GET /sessions/:id/logs/stream` | `log` | One **LogEntry** — same JSON object as each item in [`GET /sessions/:id/logs`](API_OVERVIEW.md) (§6) |
| Session attach | `GET /sessions/:id/events` | `session_event` | Lifecycle payload (below) |

### Query parameters (logs stream only)

Same semantics as log history ([`API_OVERVIEW.md` §6](API_OVERVIEW.md#stream-logs-sse)):

- **`job_id`** (optional): only forward log lines for that job.
- **`level`** (optional): only forward lines whose level matches (case-insensitive, e.g. `error`).

### `session_event` payload

JSON object:

| Field | Type | Description |
|-------|------|-------------|
| `event` | string | One of: `started`, `job_started`, `job_completed`, `completed`, `failed`, `inbox_task_enqueued` |
| `job_id` | string (optional) | Job UUID when the event refers to a specific job |
| `payload` | object | Extra context; may be empty `{}` |

**Emission rules (v1):**

| `event` | When | Typical `job_id` | `payload` |
|---------|------|------------------|-----------|
| `started` | Session leaves `pending` and becomes `running` (first assignment) | Starting job | `{}` |
| `job_started` | A job transitions from `pending` to `assigned` | That job | `{}` |
| `job_completed` | Worker reports task completion (`success` or `failed`) | Completed job | `{ "worker_reported": "success" \| "failed" }` |
| `inbox_task_enqueued` | **`POST /agents/:id/inbox`** accepted a new queue row | omitted | `{ "task_id": "uuid", "agent_id": "string" }` |
| `completed` | Session status becomes `completed` after a completion | Last completed job (if any) | `{}` |
| `failed` | Session status becomes `failed` | Failing job (if any) | `{}` |

#### Examples

```text
event: log
data: {"id":"42","timestamp":"2025-03-20T12:00:00.000Z","level":"info","session_id":"…","job_id":"…","worker_id":"w-1","source":"worker","message":"hello"}

event: session_event
data: {"event":"job_started","job_id":"550e8400-e29b-41d4-a716-446655440000","payload":{}}

event: session_event
data: {"event":"job_completed","job_id":"550e8400-e29b-41d4-a716-446655440000","payload":{"worker_reported":"success"}}
```

## Implementation note

The server uses in-memory **`tokio::sync::broadcast`** channels (one for logs, one for session events). Subscribers receive only events for the **session id** in the URL. Events are not persisted for replay; clients must load history via REST first, then attach streams ([`API_OVERVIEW.md` — client contract](API_OVERVIEW.md#get-log-history-paginated)).

## Related

- [API_OVERVIEW — §6–7](API_OVERVIEW.md#stream-logs-sse)
- [CLIENT_EXPERIENCE §4](CLIENT_EXPERIENCE.md#4-sse-logs-and-session-events)
- [PROJECT_KICKOFF §6](PROJECT_KICKOFF.md#6-communication--docs)
