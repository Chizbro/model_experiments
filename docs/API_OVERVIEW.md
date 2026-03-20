# API Overview

Concrete REST and SSE contracts for the control plane. v1: **SSE only** for log tail and session events (no WebSocket). **[CLIENT_EXPERIENCE.md](CLIENT_EXPERIENCE.md)** covers UX around these endpoints (errors, reconnect, credentials—not new server capabilities). **CLI and Web** must both expose the same operations defined here ([AGENTS.md](../AGENTS.md)); see **Spec delivery** below.

### Spec delivery (implementation requirement)

- **OpenAPI 3.x** (or equivalent) **checked into the repo** (path TBD alongside server crate, e.g. `crates/server/openapi.yaml`) is the **contract artifact** for REST shapes, security schemes, and tags. **This markdown doc and OpenAPI must stay in sync**; CI should fail if generated types or a schema-diff step diverges (see [CICD_DESIGN.md](CICD_DESIGN.md)). Each REST operation SHOULD have a stable **`operationId`** in OpenAPI for reviews and codegen; until OpenAPI exists, **HTTP method + path** in this document is the interim identifier.
- **SSE event shapes** for logs and session events must be documented in that OpenAPI document (e.g. `text/event-stream` description + example events) **or** in a short companion `docs/SSE_EVENTS.md` linked from OpenAPI—pick one approach per repo and document it in [PROJECT_KICKOFF.md](PROJECT_KICKOFF.md).
- **CLI v1:** Output is **human-readable stderr** for errors (HTTP status, `error.code`, `error.message`). **`--json` is out of scope for v1** unless added explicitly to this doc and implemented for **all** relevant subcommands in the same release.
- **One contract, two clients:** Do not describe **different** server behavior in CLI docs vs Web docs. [TECH_STACK.md](TECH_STACK.md) names stacks and commands/views that **map** to this API; it must not redefine parameters or status codes (link here instead). [CLIENT_EXPERIENCE.md](CLIENT_EXPERIENCE.md) is for presentation and safety (confirm delete, SSE backoff, bootstrap copy)—not for adding endpoints or changing payload meaning without an update here (and OpenAPI when present).
- **Ship rule:** Any new or changed control-plane operation requires **server + CLI + Web** in the same delivery ([AGENTS.md](../AGENTS.md)).

---

## 1. Auth and base

- **Base URL:** Control plane root (e.g. `https://harness.example` or Tailscale URL). CLI and Web UI use the configured `control_plane_url`.
- **Auth:** API key only. Send on every request:
  - `Authorization: Bearer <api_key>` **or**
  - `X-API-Key: <api_key>`
- **Missing/invalid key:** `401 Unauthorized` with standard error body (§2).

### Health (no auth)

- **`GET /health`** — Liveness. **Response:** `200 OK` + body `{ "status": "ok" }`. No API key required. Used by CLI and load balancers.
- **`GET /ready`** — Readiness. **Response:** `200 OK` + body `{ "status": "ok" }`. No API key required. Use for Kubernetes or similar readiness probes.
- **`GET /health/idle`** — Idle check for sleep-inhibit (see [HOSTING.md](HOSTING.md)). No API key. **Response:** `200 OK` when there are no pending or assigned jobs (OK for OS to idle-sleep), body e.g. `{ "idle": true }`. **Response:** `503 Service Unavailable` when there is work (hold sleep inhibit), body e.g. `{ "idle": false, "pending_or_assigned_jobs": N }`. Host-side helpers (or future in-process code) can poll this to decide when to allow the machine to sleep.

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
  "params": {},
  "persona_id": "string",
  "identity_id": "string",
  "retain_forever": false
}
```

- **ref** (optional): Git ref to clone (branch or commit). If omitted, default is **main**.
- **persona_id** (optional): If set, the control plane resolves this persona and combines its prompt with the task-specific info when building the prompt for the agent. Applies to all workflows. See [Personas (§5a)](#5a-rest--personas).
- **identity_id** (optional): Session identity for BYOL credentials. Omit for `"default"`. See [Identities (§4a)](#4a-rest--identities-byol-credentials).
- **retain_forever** (optional): If `true`, the session’s logs are exempt from retention purge. Default `false`. Can also be set later via PATCH /sessions/:id.
- **params** (workflow-specific):
  - **chat:** `{ "prompt": "string", "agent_cli": "claude_code | cursor" }`. Optional: `"model": "string"` (e.g. `"auto"` for Cursor; CLI default if omitted), `"branch_mode": "main | pr"`, `"branch_name_prefix": "string"`.
  - **loop_n:** `{ "prompt": "string", "n": integer, "agent_cli": "claude_code | cursor" }`. Optional: `"model"`, branch fields as above.
  - **loop_until_sentinel:** `{ "prompt": "string", "sentinel": "string", "agent_cli": "claude_code | cursor" }`. Optional: `"model"`, branch fields. **`sentinel` (v1):** **literal substring** match only—the worker treats a match as “sentinel found” if the configured string appears **anywhere** in the captured agent output for that iteration (case sensitivity: **implementation-defined**; document in server config, default **case-sensitive**). **Regex and other pattern modes are not supported in v1** (future: separate param e.g. `sentinel_mode: "literal" | "regex"`).
  - **inbox:** `{ "agent_id": "string", "agent_cli": "claude_code | cursor" }`. Optional: `"model"` (model for this inbox agent), branch fields.
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
  "jobs": [{ "job_id": "string", "status": "string", "created_at": "string", "error_message": "string|null", "pull_request_url": "string|null" }],
  "created_at": "string",
  "updated_at": "string"
}
```

- **jobs[].error_message:** Populated when the worker or engine records a failure reason; **CLI and Web UI must show this** on job/session detail (see [CLIENT_EXPERIENCE.md — Git outcomes](CLIENT_EXPERIENCE.md#8-git-commit-push-and-prmr-outcomes)).
- **jobs[].pull_request_url:** Set when the server successfully created a PR/MR; if `null` while the user expected one, the client must explain **why** using session `params` (e.g. `branch_mode`), job `status`, and [Architecture §9b](ARCHITECTURE.md#9b-when-the-control-plane-creates-a-prmr)—do not imply a silent platform bug. Copy rules: [CLIENT_EXPERIENCE §8](CLIENT_EXPERIENCE.md#8-git-commit-push-and-prmr-outcomes).
- **404** if not found.

### Send input (e.g. chat message)

- **`POST /sessions/:id/input`**
- **Request body:**

```json
{
  "message": "string"
}
```

- Used for chat workflow to send a follow-up message in the same session. Follow-up jobs pulled by the worker include **`session_prompt`** (original goal), **`message`**, **`history`** (prior user follow-ups), and **`history_assistant`** (prior assistant replies, no thinking/tool calls) so each agent run is multi-turn aware (see Pull task · **task_input**).
- **Response:** `202 Accepted` + body `{ "accepted": true }` or `200 OK` with updated session summary.
- **404** if session not found; **409** if session not in a state that accepts input (e.g. not chat or not running).

### Update session (retain_forever)

- **`PATCH /sessions/:id`**
- **Request body:** `{ "retain_forever": true | false }`. When set, the session’s logs are exempt from retention purge (see §6 and log retention).
- **Response:** `204 No Content` on success. **404** if session not found.

### Update job (retain_forever)

- **`PATCH /sessions/:id/jobs/:job_id`**
- **Request body:** `{ "retain_forever": true | false }`. When set, that job’s logs are exempt from retention purge.
- **Response:** `204 No Content` on success. **404** if session or job not found.

---

## 4a. REST — Identities (BYOL credentials)

Sessions use an **identity** to resolve **agent_token** (Cursor/Claude API key) and **git_token** (e.g. GitHub PAT). The worker runs the real agent CLI only when the pulled task has all of: `repo_url`, `git_token`, `agent_cli` (in params), and `agent_token`. Credentials come from the session’s identity first, then from session **params** (params can override or supply tokens).

- **identity_id** (optional on create): Session identity. Omit for `"default"`. The migration seeds an identity with id `"default"`.
- **params**: In addition to workflow fields, you may pass `"agent_token"` and/or `"git_token"` per session. These are merged with the identity’s tokens (identity first; params override or fill in).

### Get identity credentials status

- **`GET /identities/:id`**
- **Response:** `200 OK` with body indicating whether tokens are configured; token values are never returned. Example: `{ "has_git_token": true, "has_agent_token": true }`. **404** if identity not found. Used by the Web UI Settings to show credential status.

### Get identity auth status (token health)

- **`GET /identities/:id/auth-status`**
- **Response:** `200 OK` with token health info (no token values). Used by the Web UI Settings to show token expiry status and prompt re-authentication. Example:

```json
{
  "git_token_status": "healthy",
  "git_provider": "oauth_gitlab",
  "token_expires_at": "2026-03-18T13:00:00Z",
  "message": "Token valid for ~55 minutes."
}
```

- **`git_token_status`** values: `healthy`, `expiring_soon`, `expired_refreshable`, `expired_needs_reauth`, `unknown`, `not_configured`.
- `expired_needs_reauth` means the token is expired and no refresh_token exists; the user must re-authenticate via OAuth.
- `expired_refreshable` means the token is expired but will be auto-refreshed on the next task pull.

### List identity repositories (repo picker)

- **`GET /identities/:id/repositories`**
- **Query (optional):** `provider=github` or `provider=gitlab` — for manual PAT, hint which provider API to call.
- **Response:** `200 OK` with `{ "items": [ { "full_name": "owner/repo", "clone_url": "https://..." } ], "provider": "github" | "gitlab" }`. Used by the Web UI and CLI repo picker when creating a session. Server resolves the identity, refreshes GitLab token if needed, then calls GitHub or GitLab API. **400** if provider unknown (no OAuth and no `?provider=`). **401**/**502** if provider API rejects the token.

### Update identity tokens

- **`PATCH /identities/:id`**
- **Request body:** Any subset of (only provided fields are updated; tokens are never returned in API responses):

```json
{
  "agent_token": "string",
  "git_token": "string",
  "refresh_token": "string"
}
```

- **Response:** `204 No Content` on success. **404** if identity not found.

Example: set tokens for the default identity so all sessions using it get credentials:

```bash
curl -s -X PATCH -H "Authorization: Bearer YOUR_API_KEY" -H "Content-Type: application/json" \
  -d '{"agent_token":"your-cursor-api-key","git_token":"your-github-pat"}' \
  http://localhost:3000/identities/default
```

After this, create sessions as usual (e.g. from the UI with repo URL, workflow, and `agent_cli` in params). The worker will receive the tokens when it pulls a task and will invoke the Cursor (or Claude Code) CLI. Session creation is rejected with 400 if the identity does not have both agent_token and git_token set.

---

## 4b. OAuth — Git provider sign-in (identity credentials)

The control plane exposes **browser-based OAuth** flows so users can sign in with GitHub or GitLab and have the **git_token** stored on an identity. These endpoints are **not** protected by API key; they are used by the Web UI (or direct browser navigation) during sign-in. Server must be configured with the provider’s client ID and redirect URI (see [HOSTING.md](HOSTING.md) or server docs).

**Security measures:**

- **CSRF protection:** A random nonce is stored in an `HttpOnly; SameSite=Lax` cookie (`_rh_oauth`) and included in the OAuth `state` parameter. On callback, the server validates that the nonce matches.
- **PKCE (S256):** A `code_verifier` is generated and stored alongside the nonce in the cookie. The corresponding `code_challenge` (SHA-256, base64url) is sent in the authorization URL. On callback, the `code_verifier` is included in the token exchange.
- **Refresh tokens:** When the provider returns a `refresh_token` and `expires_in`, both are stored on the identity. Before serving a `git_token` to a worker or using it for PR/MR creation, the server proactively refreshes the access token if it is expired or will expire within 5 minutes.
- **Provider metadata:** The identity records `git_provider` (`oauth_github`, `oauth_gitlab`, or `manual`) and `git_base_url` (for self-hosted GitLab) so refresh and API calls use the correct endpoints.

- **`GET /auth/github`**
  - **Query:** Optional `identity_id` (default `"default"`). Identity on which to store the resulting git token.
  - **Response:** Redirects to GitHub OAuth authorization with PKCE `code_challenge` (S256). Sets `_rh_oauth` HttpOnly cookie for CSRF + PKCE validation. Requires env: `GITHUB_CLIENT_ID`, `GITHUB_REDIRECT_URI`. If not configured, returns 503.
- **`GET /auth/github/callback`**
  - **Query:** `code` (from GitHub), `state` (contains CSRF nonce and `identity_id`).
  - **Response:** Validates CSRF nonce against cookie, exchanges code for access token (with PKCE `code_verifier`), stores `git_token`, `refresh_token`, `token_expires_at`, and `git_provider` on the identity, clears the OAuth cookie, redirects to `REDIRECT_AFTER_AUTH` (e.g. Web UI Settings or a “credentials saved” page).
- **`GET /auth/gitlab`**
  - **Query:** Optional `identity_id` (default `"default"`).
  - **Response:** Redirects to GitLab OAuth with PKCE `code_challenge` (S256). Sets `_rh_oauth` HttpOnly cookie. Requires env: `GITLAB_CLIENT_ID`, `GITLAB_REDIRECT_URI`.
- **`GET /auth/gitlab/callback`**
  - **Query:** `code`, `state` (contains CSRF nonce and `identity_id`).
  - **Response:** Same as GitHub callback; additionally stores `git_base_url` (from `GITLAB_BASE_URL`) for self-hosted GitLab API calls.

After a successful callback, the identity has `git_token` (and optionally `refresh_token`, `token_expires_at`, `git_provider`, `git_base_url`) set. The Web UI can show credential status via `GET /identities/:id`. Agent token (Cursor/Claude) is still set via PATCH /identities/:id or UI.

---

## 4c. REST — API keys (control plane auth)

The **API key** is used to **log in to the Remote Harness service** (control plane). It is distinct from Git and agent tokens (identities/BYOL). Keys can be created via CLI (`remote-harness api-key create`), Web UI (Settings → Create API key), or **bootstrap** when no keys exist (see below). The server stores only a hash; the plain key is returned **once** at creation.

### Bootstrap (create first key, no auth)

- **`POST /api-keys/bootstrap`** — **No API key required.** Creates the first API key when the server has no keys (neither from env/config nor in the database). Same request/response shape as `POST /api-keys`. **Response:** `201 Created` + body with `id`, `key`, `label`, `created_at`. **Response:** `403 Forbidden` when any key already exists (use an existing key or set `API_KEY` in the server environment and restart). Use this from the Web UI when you have the control plane URL but no key yet.

#### Bootstrap safety (operators must read this)

Until the first key exists, **`POST /api-keys/bootstrap` is unauthenticated root-equivalent access** to issuing API keys. **Do not** expose the control plane’s HTTP port to the public internet in that state.

**Recommended patterns:**

1. **Bind to loopback or VPN-only** until bootstrap completes (e.g. first setup over SSH, Tailscale, or `127.0.0.1`).
2. **Firewall** the API port from the wide internet; open only after at least one key exists and bootstrap returns `403`.
3. Prefer **`API_KEY` in server env** (single known key at first boot) for unattended installs in trusted networks, and use DB-issued keys for humans/machines afterward.

Document this in runbooks; the Web UI should only show bootstrap **after** `GET /health` succeeds and a **401** on an authenticated probe indicates “no key configured yet,” not on every visit. See [HOSTING.md §13](HOSTING.md#13-production-and-first-run-checklist) and [CLIENT_EXPERIENCE.md §7](CLIENT_EXPERIENCE.md#7-first-time-setup-web-ui).

### Create API key

- **`POST /api-keys`**
- **Request body:** `{ "label": "string" }` (optional). **label:** Optional label (e.g. "CI", "worker-1").
- **Response:** `201 Created` + body `{ "id": "string", "key": "string", "label": "string | null", "created_at": "string" }`. **key** is the plain secret; store it (env, config, or UI); it is not stored on the server and will not be shown again.

### List API keys

- **`GET /api-keys`**
- **Query:** Pagination (§3): `limit`, `cursor`.
- **Response:** `200 OK` + body `{ "items": [ { "id": "string", "label": "string | null", "created_at": "string" } ], "next_cursor": "string | null" }`. No secret returned.

### Revoke API key

- **`DELETE /api-keys/:id`**
- **Response:** `204 No Content` on success. **404** if not found. The key stops working immediately.

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

## 5. REST — Workers

Workers register via `POST /workers/register` (see §9). CLI and Web UI can list and get workers; **delete** is available for operational use (e.g. removing stale workers from the registry).

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

### Delete worker

- **`DELETE /workers/:id`**
- **Response:** `204 No Content` on success. **404** if worker not found. Removes the worker from the registry and unlinks inbox_listeners and jobs; use to clear stale or decommissioned workers. CLI: `remote-harness workers clear <worker_id>`.

---

## 6. REST — Logs

**Client contract (consistent and complete):** Whenever a user opens logs for a context (session or job), the client **must** load the **full** history first, then stream. (1) Call `GET /sessions/:id/logs` (with optional `job_id`) and **paginate until all logs** for that context are loaded (no cap); (2) render those logs; (3) call `GET /sessions/:id/logs/stream` and append new events. This ensures the user always sees the complete backlog before any live entries. Same behavior in CLI and Web UI.

### Get log history (paginated)

- **`GET /sessions/:id/logs`**
- **Query:** `limit`, `cursor` (§3). Optional: `job_id`, `level` (e.g. `info`, `error`). **`last`:** if set (e.g. `last=50`), return only the N most recent entries in chronological order; no cursor, one page. Use for tail mode (Web UI or CLI `logs tail --last N`).
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

### Delete session logs

- **`DELETE /sessions/:id/logs`**
- **Query:** Optional **`job_id`**. If set, delete log entries **for that job only** within the session; if omitted, delete **all** log entries for the session in the central store (see [PRODUCT.md — L5](PRODUCT.md#logging--observability)).
- **Response:** `204 No Content` on success. **404** if session not found (or job not found when `job_id` is specified).
- **CLI:** `remote-harness logs delete` — see [TECH_STACK §3](TECH_STACK.md#3-cli--rust). **Web UI:** same contract; behavior (e.g. confirm before delete): [CLIENT_EXPERIENCE §9](CLIENT_EXPERIENCE.md#9-log-retention-and-purge).

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
  "capabilities": [],
  "client_version": "string"
}
```

- **id:** Unique worker id (e.g. hostname + suffix, or UUID). Server may reject if duplicate.
- **host:** Hostname or identifier for display.
- **labels:** Optional key/value. Include **platform** (e.g. `"platform": "windows" | "wsl" | "macos" | "linux"`) for observability (UI filtering, display). v1: no platform affinity—engine assigns to any available worker. Other labels (e.g. `gpu=true`) for dispatch in P1.
- **capabilities:** Optional list of strings (reserved for future use).
- **client_version:** **Required for v1 implementations.** Semver string of the worker binary (e.g. `0.4.1`), same **major.minor** family as the control plane release. The server **MUST** reject incompatible workers with **`400 Bad Request`** and error body `code: "worker_version_incompatible"` and `message` describing required range (see [CLIENT_EXPERIENCE.md §13](CLIENT_EXPERIENCE.md#13-compatibility-and-upgrades)). If omitted during a transitional period, server **MAY** accept but **SHOULD** log a warning; new code should always send this field.
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
- **task_input:** The task-specific input for this run. The worker passes this as the user/task input to the CLI. See also **Send input** (§4) for sending chat follow-up messages via `POST /sessions/:id/input`.
  - **Chat — first job:** `{ "prompt": "string", ... }` (same shape as session `params` for the workflow), i.e. the initial user message from session create.
  - **Chat — follow-up jobs** (after `POST /sessions/:id/input`): `{ "session_prompt": "string", "message": "string", "history": ["string", ...], "history_assistant": ["string", ...], "history_truncated": false }`. **`session_prompt`** is the original create-session prompt. **`history`** lists prior user follow-ups. **`history_assistant`** lists prior assistant reply text (user/assistant messages only; no thinking or tool calls). **`message`** is the current follow-up. **Long sessions:** The server **MUST** cap how much history is included so `task_input` stays bounded. **Defaults (v1):** keep at most the last **50** user turns in `history` and the last **50** assistant turns in `history_assistant` (server config: e.g. `CHAT_HISTORY_MAX_TURNS`, default `50`). Older turns are dropped from the payload (full transcript may still appear in logs/UI). When any drops occur, set **`history_truncated`: `true`**; clients **MUST** show [CLIENT_EXPERIENCE.md — Long chat](CLIENT_EXPERIENCE.md#12-long-chat-sessions).
  - **Loop workflows:** `{ "prompt": "string", "iteration_index": number, ... }`.
  - **Inbox:** listener flag or payload as documented for inbox.
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
  "worker_id": "string",
  "branch": "string",
  "commit_ref": "string",
  "mr_title": "string",
  "mr_description": "string",
  "error_message": "string",
  "output": "string",
  "sentinel_reached": true,
  "assistant_reply": "string"
}
```

- **status:** Required. `success` or `failed`.
- **worker_id:** Optional but recommended. Server uses it to validate the task is assigned to this worker.
- **branch / commit_ref:** Set when push succeeded (main or PR branch). Shown in UI.
- **mr_title / mr_description:** Optional. For PR/MR mode, suggested title and description (e.g. agent-generated); used when creating a pull/merge request (if that feature is enabled).
- **error_message:** When status is `failed`, optional human-readable error.
- **output:** Optional. Agent output snippet (e.g. last N chars); used for sentinel detection in **loop_until_sentinel** (**literal substring** search per session `params.sentinel`, v1).
- **sentinel_reached:** Optional boolean. When `true`, worker detected the sentinel substring in output; server may mark the session completed for loop_until_sentinel workflow.
- **assistant_reply:** Optional. For **chat** workflow only: assistant reply text from this job (user and assistant messages only; no thinking, tool calls, or system events). Server appends it to the session so the next follow-up job receives it in **task_input.history_assistant**.
- **Response:** `200 OK` + body `{ "ok": true }`. **404** if task unknown or already completed.

---

## 10. Summary

| Area | Method | Path | Purpose |
|------|--------|------|---------|
| Auth | — | — | `Authorization: Bearer <key>` or `X-API-Key` |
| Health | GET | /health | Liveness (no auth) |
| Health | GET | /health/idle | Idle for sleep-inhibit (no auth); 200 = OK to sleep, 503 = busy |
| Health | GET | /ready | Readiness (no auth) |
| OAuth | GET | /auth/github | Redirect to GitHub OAuth (optional identity_id) |
| OAuth | GET | /auth/github/callback | Exchange code, store git_token on identity |
| OAuth | GET | /auth/gitlab | Redirect to GitLab OAuth (optional identity_id) |
| OAuth | GET | /auth/gitlab/callback | Exchange code, store git_token on identity |
| Sessions | POST | /sessions | Create session (start workflow) |
| Sessions | GET | /sessions | List sessions (paginated) |
| Sessions | GET | /sessions/:id | Get session |
| Sessions | PATCH | /sessions/:id | Update session (e.g. retain_forever) |
| Sessions | PATCH | /sessions/:id/jobs/:job_id | Update job (retain_forever) |
| Sessions | POST | /sessions/:id/input | Send input (e.g. chat) |
| Sessions | DELETE | /sessions/:id | Delete session |
| Identities | GET | /identities/:id | Get credentials status (no token values) |
| Identities | GET | /identities/:id/auth-status | Get token health (expiry, refresh capability) |
| Identities | GET | /identities/:id/repositories | List repos (repo picker) |
| Identities | PATCH | /identities/:id | Update identity tokens |
| API keys | POST | /api-keys | Create API key (plain key returned once) |
| API keys | GET | /api-keys | List API keys (no secret) |
| API keys | DELETE | /api-keys/:id | Revoke API key |
| Personas | POST | /personas | Create persona |
| Personas | GET | /personas | List personas |
| Personas | GET | /personas/:id | Get persona |
| Workers | GET | /workers | List workers |
| Workers | GET | /workers/:id | Get worker |
| Workers | DELETE | /workers/:id | Remove worker from registry |
| Logs | GET | /sessions/:id/logs | Log history (paginated) |
| Logs | GET | /sessions/:id/logs/stream | Stream logs (SSE) |
| Logs | DELETE | /sessions/:id/logs | Delete session logs (optional job_id) |
| Events | GET | /sessions/:id/events | Session events (SSE) |
| Inboxes | POST | /agents/:id/inbox | Enqueue task |
| Inboxes | GET | /agents/:id/inbox | List inbox tasks |
| Worker | POST | /workers/register | Register worker |
| Worker | POST | /workers/:id/heartbeat | Heartbeat |
| Worker | POST | /workers/tasks/pull | Pull task |
| Worker | POST | /workers/tasks/:id/logs | Send log batch |
| Worker | POST | /workers/tasks/:id/complete | Mark task complete |

**Formal spec:** Implement these in the server; **OpenAPI in-repo** (see top of this document) is the machine-readable contract; **this document is the normative human spec**—if they disagree during a transition, update both in the same change. **PROJECT_KICKOFF.md** lists ordered implementation checkpoints derived from these docs.
