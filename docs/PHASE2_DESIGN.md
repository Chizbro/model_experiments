# Phase 2 design — Personas, inboxes, PR/MR, log search

**Status:** Approved design backlog (plan task **27**). This document is **design only**; implementation is split into future plan tasks (e.g. `plan/28-*`). It aligns [PRODUCT.md](PRODUCT.md) priorities **W4–W6**, **O2**, and **L5** search with [API_OVERVIEW.md](API_OVERVIEW.md), [ARCHITECTURE.md](ARCHITECTURE.md), and [CLIENT_EXPERIENCE.md](CLIENT_EXPERIENCE.md).

---

## 1. Goals and non-goals

| In scope | Out of scope (later tasks) |
|----------|----------------------------|
| Persona resolution, storage, and client CRUD contract | Implementing `POST /personas` etc. in Rust/OpenAPI |
| Inbox data flow, worker listener model, cross-agent enqueue | Inbox tables and pull routing code |
| PR/MR provider matrix, token expectations, failure surfacing | Git provider API client changes |
| Log search strategy and API shape | Search implementation and indexes |

---

## 2. Personas

### 2.1 Storage model

- **Table** `personas` (conceptual): `persona_id` (UUID/ULID), `name`, `prompt`, `created_at`, `updated_at`.
- **Tenancy:** Single-tenant deployment; no `org_id`. All personas belong to the instance.
- **Limits:** Enforce a **maximum `prompt` length** (recommended **256 KiB**) at API validation to avoid pathological rows and oversized pull payloads.

### 2.2 Resolution order (authoritative)

When the control plane builds **`prompt_context`** for a worker pull ([API_OVERVIEW — Pull task](API_OVERVIEW.md#pull-task)):

1. **Per-task inbox override:** If the work unit originated from **`POST /agents/:id/inbox`** and that request included **`persona_id`**, resolve that persona.
2. **Session default:** Else if the session was created with **`persona_id`**, resolve that persona.
3. **Else:** No persona — `prompt_context` is omitted or empty; only workflow params and task payload apply.

**Rationale:** Lets a continuous inbox session have a default “character,” while individual inbox messages can request a different reviewer/refactor persona for one task.

### 2.3 CRUD and API alignment

- **Create / list / get / patch / delete** follow [API_OVERVIEW §5a](API_OVERVIEW.md#5a-rest--personas). **`PATCH`** is partial update (only sent fields change).
- **Delete semantics:** If a persona is still referenced by **running** sessions or **pending** inbox rows, return **`409 Conflict`** with `error.details` including counts or IDs (implementation choice). Do not silently delete. Completed historical sessions may keep **snapshots** of prompt text in job metadata (optional optimization); if not snapshotted, deleting a persona leaves old jobs readable but future replays cannot resolve the same persona — document for operators.

### 2.4 UI and CLI (parity)

- **Web:** Dedicated **Personas** area (or a Settings subsection): list names, create, edit name + prompt, delete with confirmation.
- **CLI:** Subcommands mirroring REST (`list`, `create`, `get`, `update`, `delete`) when the feature ships — same rule as [AGENTS.md](../AGENTS.md): backend capability appears in **both** CLI and Web.

### 2.5 Open questions

| Topic | Resolution |
|-------|------------|
| Snapshot persona text on job creation? | **Optional P1+** optimization; not required for correctness if operators avoid deleting personas used in active playbooks. |
| Version history of persona edits? | **Deferred** (P2); audit log or `persona_revisions` table if customers ask. |

---

## 3. Inboxes and cross-agent tasks

### 3.1 Agent identity (`agent_id`)

- **`agent_id`** is an opaque string (recommend **UUID** in docs/examples; allow **`[a-zA-Z0-9_-]{1,128}`** for human slugs).
- **Registration vs auto-provision:** [API_OVERVIEW §8](API_OVERVIEW.md#8-rest--inboxes-p1) currently says **`404` if agent id unknown**. For **lower friction P1**, adopt **auto-provision**: the first **`POST /agents/:id/inbox`** (or **`POST /sessions`** with `workflow: inbox` and `params.agent_id`) **creates** a stub `agents` row if missing. Subsequent management can use **`GET /agents`** (new list endpoint in implementation task) for discovery. **Spec delta:** When implementing, update API_OVERVIEW to describe auto-provision and optional explicit **`POST /agents`** for metadata (display name, default `persona_id`) if we add it.

### 3.2 Data model and promotion to jobs

- **Inbox queue:** Persist inbound tasks in an **`inbox_tasks`**-style store: `task_id`, `agent_id`, `payload` (JSON), optional `persona_id`, `enqueued_at`, status (`pending` → `claimed` → `done` / `failed`).
- **Listener claim:** At most **one** active **inbox listener** per `agent_id` (worker that registered intent to consume that inbox). If another worker attempts to claim, **`409 Conflict`**. Claim released on worker **stale**, **DELETE /workers/:id**, or explicit “release inbox” API if added later.
- **Unified execution path:** When a listener **pulls** work, the engine **promotes** the next pending inbox row into a normal **`jobs`** row attached to the **long-lived inbox session** (same session as today’s `workflow: inbox` design). The worker completes it via existing **`POST /workers/tasks/:id/complete`**. **Rationale:** One reclaim/lease/log/SSE path; avoids a parallel “inbox-only” completion API.

### 3.3 Worker pull ordering

- **Priority suggestion:** In `pull_task`, after reclaim/lease logic, consider **inbox-promoted jobs** for workers that hold an **inbox listener** for `agent_id` matching the session **before** generic FIFO, so continuous agents starve less. Exact ordering is implementation-defined but should be **documented in worker-facing runbooks**.

### 3.4 Cross-agent spawn (W5)

- **Mechanism:** From a running job, the worker (or control plane helper) calls **`POST /agents/:target_agent_id/inbox`** with the same **API key** auth as other worker calls. Body: **`payload`** + optional **`persona_id`**.
- **Authorization (single-tenant P1):** Any authenticated client (CLI, Web, worker) may enqueue to any `agent_id`. **Trust model:** One team, one API key surface. **P2 hardening (flagged):** Session param **`allowed_spawn_targets`: string[]** to restrict automated agents.
- **Observability:** Optional **`spawned_from_job_id`** / **`correlation_id`** inside `payload` for UI tracing (convention, not strict schema).

### 3.5 Open questions

| Topic | Resolution |
|-------|------------|
| SSE event when inbox task arrives? | **Desirable:** emit session event `inbox_task_enqueued` for listener UI; specify in [SSE_EVENTS.md](SSE_EVENTS.md) when implementing. |
| Max inbox depth | **Configurable** cap (e.g. 1000 pending per agent); **`429`** or **`503`** when exceeded — prevents unbounded DB growth. |

---

## 4. PR/MR creation (O2)

### 4.1 Provider matrix

| Provider | URL detection (heuristic) | Create API |
|----------|---------------------------|------------|
| **GitHub.com / GHE** | Host `github.com` or configured enterprise host | REST: create pull request for `head` branch |
| **GitLab.com / self-hosted** | Host `gitlab.com` or user GitLab base URL | REST: create merge request on project derived from clone URL |

**Self-hosted GitLab:** Instance base URL must be known from **`repo_url`** or identity metadata so API paths resolve.

### 4.2 Token scopes (operator documentation)

| Provider | Minimum scopes (typical) |
|----------|---------------------------|
| **GitHub (HTTPS)** | Classic: **`repo`** for private repos. Fine-grained: Contents + Pull requests **read/write** on the repository. |
| **GitLab** | **`api`** or project token with **`write_repository`** + merge request create permission per policy. |

OAuth refresh behavior stays as [API_OVERVIEW — Identities](API_OVERVIEW.md#4a-rest--identities-byol-credentials): refresh before PR/MR call when expiry is near.

### 4.3 Control plane vs worker split

- **Worker:** Branch creation, commit, **push** to remote (existing [ARCHITECTURE §9a](ARCHITECTURE.md#9a-when-the-worker-attempts-commit-and-push)).
- **Control plane:** After **`task_complete`** with **success** and **`branch_mode: "pr"`**, non-empty branch + title, recognized host, valid **git** token — invoke provider API. Matches [ARCHITECTURE §9b](ARCHITECTURE.md#9b-when-the-control-plane-creates-a-prmr).

### 4.4 Failure UX (must not be silent)

- Today: job may **complete** without **`pull_request_url`** ([CLIENT_EXPERIENCE §8](CLIENT_EXPERIENCE.md#8-git-commit-push-and-prmr-outcomes)).
- **Phase 2 implementation:** Add optional **`pull_request_error`** (string, nullable) on **job** objects in API/OpenAPI when MR creation was attempted or skipped for a **knowable** reason (scope, rate limit, 404 project). **Do not** fail the job solely for MR API failure unless product later requires strict mode.
- **Logs:** Control plane emits a structured log line with `session_id`, `job_id`, `provider`, and error summary.

### 4.5 Open questions

| Topic | Resolution |
|-------|------------|
| **Bitbucket / other hosts** | **Out of scope** until a dedicated task; keep detection conservative and document “unsupported host → no MR.” |
| Strict mode (fail job if MR fails)? | **Deferred**; default remains **complete with note**. |

---

## 5. Log search (L5 extension)

### 5.1 Requirements recap

- Retention, purge, and **`retain_forever`** are **P0** and already specified.
- **Search/filter** in CLI and Web is **P1** per [PRODUCT — L5](PRODUCT.md#logging--observability).

### 5.2 Index strategy

| Approach | Pros | Cons | Use |
|--------|------|------|-----|
| **A. SQL `ILIKE` / `LIKE '%q%'`** | Zero schema change option | Full scan on large tables | **SQLite dev** or small Postgres |
| **B. Postgres FTS** (`tsvector` + **GIN**) | Good relevance, still one DB | Migration + tuning per language | **Recommended default for Postgres prod** |
| **C. External search (Meilisearch, OpenSearch)** | Best at very large scale | New service, backup, security | **Future** if single-DB search becomes hot |

**Decision:** Ship **(B)** for **Postgres** behind config, e.g. `LOG_SEARCH_MODE=fts|simple`. **SQLite** stays **(A)** only. **(C)** explicitly **not** in Phase 2 unless a follow-up task adds optional sidecar.

### 5.3 API shape

- Extend **`GET /sessions/:id/logs`** with optional query param **`search`** (string). Semantics: match against **`message`** (and optionally `source` / `level` filters combined). Pagination unchanged ([API_OVERVIEW §3](API_OVERVIEW.md#3-pagination)).
- **Cross-session / global search:** **Deferred to P2** (operator dashboard feature).

### 5.4 Client behavior

- **Web:** Search box in log panel; debounce input; show “no results” vs “error”.
- **CLI:** `logs tail` or `logs search` with `--search` — mirror the same query param to REST.

### 5.5 Open questions

| Topic | Resolution |
|-------|------------|
| Regex search? | **Deferred**; literal substring / FTS token match first. |
| Highlighting matches | **Web-only UX** first; CLI plain text. |

---

## 6. Traceability

| Product / doc | This design section |
|---------------|---------------------|
| W4, W5, W6 | §3, §2 |
| O2 | §4 |
| L5 search | §5 |
| [ARCHITECTURE §4b](ARCHITECTURE.md#4b-personas-separate-agent-identities) | §2 |
| [ARCHITECTURE §7](ARCHITECTURE.md#7-agent-inboxes--cross-agent-tasks) | §3 |
| [ARCHITECTURE §9b](ARCHITECTURE.md#9b-when-the-control-plane-creates-a-prmr) | §4 |
| [API_OVERVIEW §5a, §8, §6](API_OVERVIEW.md) | §2, §3, §5 |

---

## 7. Implementation sequencing (informative)

Suggested order for future plan tasks: **(1)** personas storage + resolution + UI/CLI, **(2)** inbox tables + listener + promotion + spawn, **(3)** `pull_request_error` + provider hardening, **(4)** log search param + FTS migration. Exact split is up to the next plan author.

---

*Previous: [PROJECT_KICKOFF](PROJECT_KICKOFF.md) | Next implementation: follow [plan/README.md](../plan/README.md)*
