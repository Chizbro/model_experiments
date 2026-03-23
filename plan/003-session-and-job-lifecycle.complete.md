# 003: Session & Job Lifecycle

## Goal
Users can create sessions (workflows), list/get/delete them. The engine creates jobs for each session. This implements the session state machine and job creation for the chat workflow (simplest case — one job per session create).

## Scope
### Server endpoints
- `POST /sessions` — Create session. Validate required fields (repo_url, workflow, params with prompt and agent_cli). Check identity has credentials (agent_token + git_token on identity or in params). Create session row + first job row. Return 201 with session_id, status, web_url.
- `GET /sessions` — List sessions (paginated, optional status filter).
- `GET /sessions/:id` — Get session with jobs array.
- `DELETE /sessions/:id` — Delete session (cascades to jobs and logs). Return 204.
- `PATCH /sessions/:id` — Update retain_forever. Return 204.
- `PATCH /sessions/:id/jobs/:job_id` — Update job retain_forever. Return 204.

### Session state machine
- Session starts as `pending`
- When first job is assigned to a worker → `running`
- When all jobs complete successfully → `completed`
- If any job fails and no more pending jobs → `failed`

### Job creation rules (by workflow type)
- **chat:** 1 job created at session start
- **loop_n:** N jobs created at session start (iteration_index 0..N-1)
- **loop_until_sentinel:** 1 job created at session start; more created on task_complete if sentinel not reached
- **inbox:** no initial job; jobs created when inbox tasks arrive (P1, stub only)

### Server route file
- `src/routes/sessions.rs` — All session endpoints
- `src/engine/mod.rs` — Session creation logic, job creation, state machine transitions
- `src/engine/workflows.rs` — Workflow-specific job creation logic

## Prerequisites
- Spec 001 complete (workspace, DB, server skeleton)
- Spec 002 helpful but not required (sessions don't need workers to exist)

## Files to create/modify
- `crates/server/src/routes/sessions.rs` — New
- `crates/server/src/engine/mod.rs` — New
- `crates/server/src/engine/workflows.rs` — New
- `crates/server/src/routes/mod.rs` — Mount session routes

## Acceptance criteria
1. `POST /sessions` with valid chat params → 201, session + 1 job in DB
2. `POST /sessions` with loop_n, n=3 → 201, session + 3 jobs in DB
3. `POST /sessions` with loop_until_sentinel → 201, session + 1 job
4. `POST /sessions` without required fields → 400 with descriptive error
5. `POST /sessions` when identity missing credentials → 400 with "both Git and agent tokens are required"
6. `GET /sessions` → paginated list
7. `GET /sessions?status=pending` → filtered list
8. `GET /sessions/:id` → session with jobs array
9. `DELETE /sessions/:id` → 204, session + jobs + logs deleted
10. `PATCH /sessions/:id` with retain_forever=true → 204, updated
11. Session status transitions work correctly (tested via direct DB manipulation or engine calls)
12. `cargo test` — at least 5 tests covering CRUD and validation
13. `cargo clippy` clean

## Implementation notes
- `web_url` in create response: if config has `WEB_UI_URL`, format as `{WEB_UI_URL}/sessions/{id}`; otherwise omit.
- Identity credential check: query identities table, merge with params tokens. Both agent_token and git_token must be present (from either source).
- For job creation in loop_n: use a transaction to insert all N jobs atomically.
