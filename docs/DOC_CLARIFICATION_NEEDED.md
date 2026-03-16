# Doc Review: Tentative Language & Underspecified Areas

This document captures **tentative language**, **underspecified behavior**, and **ambiguities** that could block a well-defined implementation plan. Use it to decide and then update the main docs.

**Clarifications applied (Decisions §13–§22):** Worker heartbeat/stale (§13), job granularity (§14), multi-turn chat (§15), BYOL fallback and token refresh (§16), config precedence (§17), Web UI storage (§18), branch naming (§19), log storage rule (§20), Phase 1 workflow = chat (§21), wake precedence and CLI script (§22). Main docs (ARCHITECTURE, PRODUCT, TECH_STACK, HOSTING, PROJECT_KICKOFF, API_OVERVIEW) updated accordingly. Sections below marked **addressed** where resolved.

---

## 1. Tentative or vague language

| Location | Wording | Issue |
|----------|---------|--------|
| **PRODUCT.md** (BYOL) | "when the provider offers it" (OAuth) | Unclear what happens if Claude Code or Cursor don’t offer OAuth; no fallback specified. |
| **PRODUCT.md** (BYOL) | "as appropriate" (token refresh) | When and how the control plane refreshes tokens is not defined. |
| **ARCHITECTURE.md** §3 | "periodic heartbeats (e.g. …)" | Heartbeat **interval** and **who configures it** (server vs worker) not specified. |
| **ARCHITECTURE.md** §4 | "each run can be one 'task' or one loop iteration logged as sub-steps" | Not pinned: is one job per loop iteration or one job per whole loop? Affects DB and API. |
| **ARCHITECTURE.md** §9 | "Branch naming can be derived from: task id, session id, or a user-provided prefix" | Default and API shape (which field, optional/required) not specified. |
| **TECH_STACK.md** §1 | "Files + rotation or DB table for small scale" | Rotation policy (size/time) and when to use “files” vs “DB” not defined. |
| **TECH_STACK.md** §6 | "Document precedence (e.g. config overrides env)" | Precedence is never actually stated (env vs config file vs CLI flag). |
| **API_OVERVIEW.md** | "stream or batch" (worker logs) | Worker log upload: stream vs batch and payload format not specified. |
| **DECISIONS.md** §8i | "e.g. CLI opens a browser … or device/code flow if the provider supports it" | Which mechanism for Claude Code vs Cursor in v1 is not pinned. |
| **HOSTING.md** §4 | "WAKE_URL" or "WAKE_SCRIPT" | Precedence if both are set not specified; for CLI “run a script” (local path?) not defined. |

---

## 2. Underspecified: no enough detail to implement

### 2.1 API contracts (API_OVERVIEW → OpenAPI) — **addressed**

- **Request/response shapes**: Defined in [API_OVERVIEW.md](API_OVERVIEW.md) for sessions, workers, logs, inboxes, and worker register/heartbeat/pull/logs/complete.
- **Errors**: Standard error body and status codes in API_OVERVIEW §2.
- **Pagination**: Cursor-based, `limit` (default 20, max 100), `next_cursor` in API_OVERVIEW §3.
- **Real-time**: v1 uses **SSE** only (no WebSocket for clients). Streams: `GET /sessions/:id/logs/stream`, `GET /sessions/:id/events`; event shapes in API_OVERVIEW §6–7.
- **Worker task payload**: Full task and per-workflow `params` in API_OVERVIEW §9 (Pull task).

### 2.2 Worker and control plane behavior

- **Stale worker**: After how many missed heartbeats (or after what timeout) is a worker marked stale? Configurable?
- **Heartbeat interval**: Value (e.g. 30s) and whether it’s server-configured, worker-configured, or fixed.
- **Session start params**: What top-level and workflow-specific parameters does `session start` accept? (e.g. for “loop N”, is it `n`, `prompt`, `repo`, etc.?)

### 2.3 Workflows and jobs

- **Chat “optional multi-turn”**: Is multi-turn in scope for v1 or explicitly out?
- **Loop N / loop until**: One **job** per iteration vs one job per entire loop — need a single decision for DB and APIs.
- **Inbox / spawn**: Exact schema of “task” or “payload” for `spawn_task(agent_id, payload)` and for inbox dequeue.

### 2.4 Logging — **addressed**

- **Log entry schema**: Fields and types in [API_OVERVIEW](API_OVERVIEW.md) §6 (GET logs, stream, and worker POST logs).
- **Worker → control plane**: Batch JSON array only in v1. [API_OVERVIEW](API_OVERVIEW.md) §9 (Send logs).

### 2.5 BYOL and auth

- **Which CLI per run**: How does the system choose “Claude Code” vs “Cursor” for a session or job? User choice at session start? Account-level default?
- **Token refresh**: When does the control plane refresh (e.g. on use, cron, on 401)? Where is it documented?

### 2.6 Config and precedence

- **CLI / Worker config**: Env vs config file vs CLI flag — explicit precedence (e.g. “CLI flag > env > config file”).
- **Web UI**: Control plane URL and API key — where stored (localStorage vs sessionStorage) and behavior on logout.

---

## 3. Either/or or “optional” still ambiguous

| ID | Where | Choice | Needed for implementation |
|----|--------|--------|----------------------------|
| **E1** | Phase 1 workflow | “chat” vs “run-once” | PROJECT_KICKOFF says “e.g. chat or run-once” — pick one for “first workflow”. |
| **E2** | Log storage (scale) | “Files + rotation” vs “DB table” | TECH_STACK allows both; need rule (e.g. “v1: DB only” or “files for workers, DB on control plane”). |
| **E3** | Wake config | WAKE_URL vs WAKE_SCRIPT | Precedence when both set; and for CLI, meaning of “run script” (local path, who runs it). |

(Note: DECISIONS.md already resolves B1–B9 and §12; the list above is for items that either weren’t in DOC_REVIEW or need one more level of detail.)

---

## 4. Suggested order to clarify

1. **API and data shapes**  
   - Session create/start request and response.  
   - Task payload to worker (and per-workflow params).  
   - Log entry schema and worker log upload (stream vs batch, format).  
   - Error response format and pagination.

2. **Worker lifecycle**  
   - Heartbeat interval and source of truth.  
   - Stale threshold (missed heartbeats or timeout) and configurability.

3. **Workflows and jobs**  
   - Job granularity for “loop N” and “loop until sentinel”.  
   - Multi-turn chat in/out for v1.  
   - Inbox/spawn task payload schema.

4. **BYOL and CLI choice**  
   - How “Claude Code vs Cursor” is chosen per session/job.  
   - OAuth vs fallback (e.g. paste token) if provider has no OAuth.  
   - Token refresh policy.

5. **Config and precedence**  
   - Env / config file / CLI flag precedence for CLI and worker.  
   - Web UI storage for URL and API key.

6. **Operational details**  
   - Default branch naming rule and API.  
   - Log rotation (if files used) and when to use files vs DB.  
   - Wake: WAKE_URL vs WAKE_SCRIPT precedence and CLI script semantics.

---

## 5. Next step

Go through **Sections 1–3** item by item; for each:

- **Decide** the intended behavior or choice.  
- **Update** the corresponding doc (ARCHITECTURE, PRODUCT, TECH_STACK, API_OVERVIEW, HOSTING, etc.).  
- **Record** in DECISIONS.md if it’s a design decision.  
- **Add** to OpenAPI (or API spec) once endpoints and payloads are defined.

After that, the docs should be enough to derive a single, well-defined implementation plan.
