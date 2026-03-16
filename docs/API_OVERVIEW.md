# API Overview

Concrete REST and SSE contracts for the control plane. v1: **SSE only** for log tail and session events (no WebSocket). Formalize in OpenAPI and keep in sync with the server.

---

## 1. Auth and base

- **Base URL:** Control plane root (e.g. `https://harness.example` or Tailscale URL). CLI and Web UI use the configured `control_plane_url`.
- **Auth:** API key only. Send on every request:
  - `Authorization: Bearer <api_key>` **or**
  - `X-API-Key: <api_key>`
- **Missing/invalid key:** `401 Unauthorized` with standard error body (§2).

---

## 2. Standard error response

All errors use the same JSON shape:

```json
{
  "error": {
    "code": "string",
    "message": "string",
    "details": {}
  }
}
```

- **code:** Machine-readable (e.g. `not_found`, `invalid_request`, `unauthorized`).
- **message:** Human-readable.
- **details:** Optional; extra fields (e.g. `field`, `session_id`).

**HTTP status:** 400 (invalid request), 401 (unauthorized), 404 (not found), 409 (conflict), 500 (server error).

---

## 3. Pagination

List endpoints that support pagination use **cursor-based** pagination.

- **Query params:** `limit` (optional, default **20**, max **100**), `cursor` (optional, opaque string from previous response).
- **Response:** JSON body includes:
  - `items`: array of resources
  - `next_cursor`: string or `null`; if present, use as `cursor` for the next page
  - No `prev_cursor` in v1.

---

## 4. REST — Sessions

### Create session (start workflow)

- **`POST /sessions`**
- **Request body:**

```json
{
  "repo_url": "string",
  "ref": "string",
  "workflow": "chat | loop_n | loop_until_sentinel | inbox",
  "params": {}
}
```

- **ref** (optional): Git ref to clone (branch or commit). If omitted, default is **main**.
- **persona_id** (optional): If set, the control plane resolves this persona and combines its prompt with the task-specific info when building the prompt for the agent. Applies to all workflows. See [Personas (§5a)](#5a-rest--personas).
- **params** (workflow-specific):
  - **chat:** `{ "prompt": "string", "agent_cli": "claude_code | cursor" }`. Optional: `"branch_mode": "main | pr"`, `"branch_name_prefix": "string"`.
  - **loop_n:** `{ "prompt": "string", "n": integer, "agent_cli": "claude_code | cursor" }`. Optional branch fields as above.
  - **loop_until_sentinel:** `{ "prompt": "string", "sentinel": "string", "agent_cli": "claude_code | cursor" }`. Optional branch fields. `sentinel` is literal string or regex (v1: literal only).
  - **inbox:** `{ "agent_id": "string", "agent_cli": "claude_code | cursor" }`. Optional branch fields.
- **Response:** `201 Created` + body:

```json
{
  "session_id": "string",
  "status": "pending",
  "web_url": "string"
}
```

- **web_url:** Optional deep link to Web UI for this session (e.g. `https://ui.example/sessions/<session_id>`). Omitted if not configured.

### List sessions

- **`GET /sessions`**
- **Query:** Pagination (§3). Optional: `status` (e.g. `pending`, `running`, `completed`, `failed`), `limit`, `cursor`.
- **Response:** `200 OK` + body:

```json
{
  "items": [
    {
      "session_id": "string",
      "repo_url": "string",
      "ref": "string",
      "workflow": "string",
      "status": "string",
      "created_at": "string"
    }
  ],
  "next_cursor": "string | null"
}
```

- **created_at:** ISO 8601.

### Get session

- **`GET /sessions/:id`**
- **Response:** `200 OK` + body:

```json
{
  "session_id": "string",
  "repo_url": "string",
  "ref": "string",
  "workflow": "string",
  "status": "string",
  "params": {},
  "jobs": [{ "job_id": "string", "status": "string", "created_at": "string" }],
  "created_at": "string",
  "updated_at": "string"
}
```

- **404** if not found.

### Send input (e.g. chat message)

- **`POST /sessions/:id/input`**
- **Request body:**

```json
{
  "message": "string"
}
```

- Used for chat workflow to send a follow-up message in the same session.
- **Response:** `202 Accepted` + body `{ "accepted": true }` or `200 OK` with updated session summary.
- **404** if session not found; **409** if session not in a state that accepts input (e.g. not chat or not running).

---

## 5a. REST — Personas

Personas are user-defined, pre-configured prompts. When starting a session or enqueueing to an inbox, the client can pass **persona_id**; the control plane then combines that persona's prompt with the task-specific information and provides it to the worker when the agent is invoked. See [Architecture §4b](ARCHITECTURE.md) and [Product W6](PRODUCT.md).

### Create persona

- **`POST /personas`**
- **Request body:** `{ "name": "string", "prompt": "string" }`. **name:** Display name (e.g. "Refactorer"). **prompt:** The pre-configured prompt text (system/context for the agent).
- **Response:** `201 Created` + body `{ "persona_id": "string", "name": "string", "prompt": "string" }`.

### List personas

- **`GET /personas`**
- **Query:** Pagination (§3).
- **Response:** `200 OK` + body `{ "items": [ { "persona_id": "string", "name": "string" } ], "next_cursor": "string | null" }`. Prompt text may be omitted in list for brevity.

### Get persona

- **`GET /personas/:id`**
- **Response:** `200 OK` + body `{ "persona_id": "string", "name": "string", "prompt": "string" }`. **404** if not found.

### Update / delete (optional in v1)

- **`PATCH /personas/:id`** — body `{ "name": "string", "prompt": "string" }` (partial). **DELETE /personas/:id** — remove persona. **404** if not found.

---

## 5. REST — Workers (read-only for clients)

Workers register via `POST /workers/register` (see §9). CLI/UI only read.

### List workers

- **`GET /workers`**
- **Query:** Optional pagination (§3).
- **Response:** `200 OK` + body:

```json
{
  "items": [
    {
      "worker_id": "string",
      "host": "string",
      "labels": {},
      "status": "active | stale",
      "last_seen_at": "string"
    }
  ],
  "next_cursor": "string | null"
}
```

### Get worker

- **`GET /workers/:id`**
- **Response:** `200 OK` with same shape as one list item (+ optional `capabilities`). **404** if not found.

---

## 6. REST — Logs

**Client contract (consistent and complete):** Whenever a user opens logs for a context (session or job), the client **must** load the **full** history first, then stream. (1) Call `GET /sessions/:id/logs` (with optional `job_id`) and **paginate until all logs** for that context are loaded (no cap); (2) render those logs; (3) call `GET /sessions/:id/logs/stream` and append new events. This ensures the user always sees the complete backlog before any live entries. Same behavior in CLI and Web UI.

### Get log history (paginated)

- **`GET /sessions/:id/logs`**
- **Query:** `limit`, `cursor` (§3). Optional: `job_id`, `level` (e.g. `info`, `error`).
- **Response:** `200 OK` + body:

```json
{
  "items": [
    {
      "id": "string",
      "timestamp": "string",
      "level": "string",
      "session_id": "string",
      "job_id": "string",
      "worker_id": "string",
      "source": "string",
      "message": "string"
    }
  ],
  "next_cursor": "string | null"
}
```

- **Log entry fields:** `timestamp` (ISO 8601), `level` (e.g. `debug`, `info`, `warn`, `error`), `session_id`, `job_id` (nullable), `worker_id` (nullable), `source` (e.g. `agent`, `worker`, `control_plane`), `message` (string).
- **404** if session not found.

### Stream logs (SSE)

- **`GET /sessions/:id/logs/stream`**
- **Query:** Optional `job_id`, `level`.
- **Response:** `200 OK`, `Content-Type: text/event-stream`. Each event is one log entry:
  - **Event type:** `log`
  - **Data:** JSON string of one log object (same shape as §6 item above).
- Client keeps connection open; server sends events as logs arrive. Reconnect on disconnect if session still active. **Use after loading history** (see client contract above).

---

## 7. REST — Session events (SSE)

- **`GET /sessions/:id/events`**
- **Response:** `200 OK`, `Content-Type: text/event-stream`. Events signal lifecycle:
  - **Event type:** `session_event`
  - **Data:** JSON string, e.g. `{ "event": "started" | "job_started" | "job_completed" | "completed" | "failed", "job_id": "string", "payload": {} }`
- Used by CLI attach and Web UI for live status. Same session_id as logs stream.

---

## 8. REST — Inboxes (P1)

### Enqueue task to agent inbox

- **`POST /agents/:id/inbox`**
- **Request body:**

```json
{
  "payload": {},
  "persona_id": "string"
}
```

- **payload:** Opaque object for the consuming agent (e.g. `{ "prompt": "string", "context": {} }`). Schema is agent-specific; control plane stores and forwards.
- **persona_id** (optional): Persona to use when the agent processes this task. Control plane combines persona prompt + payload when the worker runs the task.
- **Response:** `202 Accepted` + body `{ "task_id": "string" }`. **404** if agent id unknown.

### List / poll inbox (for workers)

- **`GET /agents/:id/inbox`**
- **Query:** `limit`, `cursor`. Used by worker or UI to list pending tasks.
- **Response:** `200 OK` + body:

```json
{
  "items": [
    {
      "task_id": "string",
      "payload": {},
      "enqueued_at": "string"
    }
  ],
  "next_cursor": "string | null"
}
```

- Claim/dequeue is done via worker task pull (§9); this endpoint is for listing only in v1.

---

## 9. Worker ↔ Control plane

Workers authenticate with the same API key (header). Base URL = control plane root.

### Register

- **`POST /workers/register`**
- **Request body:**

```json
{
  "id": "string",
  "host": "string",
  "labels": {},
  "capabilities": []
}
```

- **id:** Unique worker id (e.g. hostname + suffix, or UUID). Server may reject if duplicate.
- **host:** Hostname or identifier for display.
- **labels:** Optional key/value. Include **platform** (e.g. `"platform": "windows" | "wsl" | "macos" | "linux"`) for observability (UI filtering, display). v1: no platform affinity—engine assigns to any available worker. Other labels (e.g. `gpu=true`) for dispatch in P1.
- **capabilities:** Optional list of strings (reserved for future use).
- **Response:** `201 Created` + body `{ "worker_id": "string" }` (echo or server-assigned). **409** if id already registered (e.g. restart with same id).

### Heartbeat

- **`POST /workers/:id/heartbeat`**
- **Request body:**

```json
{
  "status": "idle | busy",
  "current_job_id": "string | null"
}
```

- Workers send this **periodically**. Interval is **worker-configured** (e.g. 30s); server does not mandate. Server updates `last_seen` and marks worker **stale** if no heartbeat for a **server-configured** threshold (e.g. 3× heartbeat interval or 90s); document in server config.
- **Response:** `200 OK` + body `{ "ok": true }`. **404** if worker id unknown.

### Pull task

- **`POST /workers/tasks/pull`**
- **Request body:** Optional `worker_id` (to ensure task is assigned to this worker). Can be omitted if server infers from auth.
- **Response (task available):** `200 OK` + body:

```json
{
  "task_id": "string",
  "job_id": "string",
  "session_id": "string",
  "repo_url": "string",
  "ref": "string",
  "workflow": "chat | loop_n | loop_until_sentinel | inbox",
  "prompt_context": "string",
  "task_input": {},
  "params": {},
  "credentials": {
    "git_token": "string",
    "agent_token": "string"
  }
}
```

- **prompt_context:** When a persona was specified for the session, this is that persona's prompt text (system/context for the agent). Omitted or empty when no persona. The worker passes this as the agent's context (e.g. system prompt).
- **task_input:** The task-specific input for this run: for chat the user message(s) or initial prompt, for loop the iteration prompt, for inbox the payload. Shape matches the workflow (e.g. `{ "prompt": "string" }` or inbox payload). The worker passes this as the user/task input to the CLI.
- **params:** Other workflow params (repo, ref, workflow type, branch_mode, etc.) as in session create; may duplicate fields for convenience. **credentials:** Per-job tokens (git clone/push and agent CLI). Worker uses them only for this task.
- **Response (no work):** `204 No Content` or `200 OK` with `{ "task_id": null }`. Worker should poll again after a delay (e.g. long-poll or backoff).

### Send logs

- **`POST /workers/tasks/:id/logs`**
- **Request body:** **Batch only in v1.** JSON array of log entries:

```json
[
  {
    "timestamp": "string",
    "level": "string",
    "message": "string",
    "source": "string"
  }
]
```

- **timestamp:** ISO 8601. **level:** e.g. `debug`, `info`, `warn`, `error`. **source:** e.g. `agent`, `worker`. Server adds `session_id`, `job_id`, `worker_id` from task context.
- **Response:** `202 Accepted` + body `{ "accepted": true }`. No ordering guarantee across batches; ordering within a batch preserved.

### Task complete

- **`POST /workers/tasks/:id/complete`**
- **Request body:**

```json
{
  "status": "success | failed",
  "branch": "string",
  "commit_ref": "string",
  "error_message": "string"
}
```

- **branch / commit_ref:** Set when push succeeded (main or PR branch). **error_message:** When status is `failed`.
- **Response:** `200 OK` + body `{ "ok": true }`. **404** if task unknown or already completed.

---

## 10. Summary

| Area | Method | Path | Purpose |
|------|--------|------|---------|
| Auth | — | — | `Authorization: Bearer <key>` or `X-API-Key` |
| Sessions | POST | /sessions | Create session (start workflow) |
| Sessions | GET | /sessions | List sessions (paginated) |
| Sessions | GET | /sessions/:id | Get session |
| Sessions | POST | /sessions/:id/input | Send input (e.g. chat) |
| Personas | POST | /personas | Create persona |
| Personas | GET | /personas | List personas |
| Personas | GET | /personas/:id | Get persona |
| Workers | GET | /workers | List workers |
| Workers | GET | /workers/:id | Get worker |
| Logs | GET | /sessions/:id/logs | Log history (paginated) |
| Logs | GET | /sessions/:id/logs/stream | Stream logs (SSE) |
| Events | GET | /sessions/:id/events | Session events (SSE) |
| Inboxes | POST | /agents/:id/inbox | Enqueue task |
| Inboxes | GET | /agents/:id/inbox | List inbox tasks |
| Worker | POST | /workers/register | Register worker |
| Worker | POST | /workers/:id/heartbeat | Heartbeat |
| Worker | POST | /workers/tasks/pull | Pull task |
| Worker | POST | /workers/tasks/:id/logs | Send log batch |
| Worker | POST | /workers/tasks/:id/complete | Mark task complete |

**Formal spec:** Implement these in the server and keep an OpenAPI (or equivalent) spec in sync. This document is the source of truth until the spec file exists.
