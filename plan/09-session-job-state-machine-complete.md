# 09 - Session & Job State Machine

## Goal
Implement session CRUD and the job state machine. Sessions are user-visible runs; jobs are internal units of work within sessions. The workflow engine creates jobs based on session type.

## What to build

### Session routes (`crates/server/src/routes/sessions.rs`)

**POST /sessions**
- Validate request: repo_url required, workflow must be valid enum, params must match workflow type
- Validate credentials: session's identity must have both agent_token and git_token (400 if not)
- Create session row in DB
- Create initial job(s) based on workflow type:
  - `chat`: 1 job with prompt as task_input
  - `loop_n`: N jobs, each with `{ prompt, iteration_index }`
  - `loop_until_sentinel`: 1 job initially (more created as iterations complete without sentinel)
  - `inbox`: session created, no immediate jobs (jobs come from inbox)
- Response: `201 { "session_id", "status": "pending", "web_url" }`

**GET /sessions**
- Paginated list, optional `status` filter
- Response: `200 { "items": [SessionSummary], "next_cursor" }`

**GET /sessions/:id**
- Full detail including jobs array
- Response: `200 { SessionDetail }` or `404`

**PATCH /sessions/:id**
- Update `retain_forever`
- Response: `204` or `404`

**PATCH /sessions/:id/jobs/:job_id**
- Update job `retain_forever`
- Response: `204` or `404`

**POST /sessions/:id/input**
- Chat workflow only: validate session is chat and running
- Create a new job with task_input containing: session_prompt, message, history (prior user messages), history_assistant (prior assistant replies), history_truncated flag
- History cap: keep last N turns per `CHAT_HISTORY_MAX_TURNS` config
- Response: `202 { "accepted": true }` or `409` if wrong state

**DELETE /sessions/:id**
- Delete session and cascade to jobs and logs
- Response: `204` or `404`

### Job state machine (`crates/server/src/engine/jobs.rs`)
- States: `pending` -> `assigned` -> `running` -> `completed` | `failed`
- Transitions enforced in code (no invalid transitions)
- `assign_to_worker(job_id, worker_id)` — sets assigned, assigned_at
- `mark_running(job_id)` — optional explicit running state
- `complete_job(job_id, result)` — sets completed/failed, stores output fields
- Session status derived from jobs: pending if all pending, running if any assigned/running, completed if all completed, failed if any failed and none running

### Workflow engine (`crates/server/src/engine/mod.rs`)
- Module that handles job creation logic per workflow type
- `create_jobs_for_session(session)` — creates appropriate jobs
- `on_job_completed(job)` — for loop_until_sentinel: check sentinel, create next job if not reached; update session status

## Dependencies
- Task 04 (server foundation)
- Task 05 (API key auth)
- Task 06 (identity credentials — for credential validation on session create)

## Test criteria
- [ ] `POST /sessions` with valid params creates session and initial jobs
- [ ] `POST /sessions` with missing credentials returns `400`
- [ ] `GET /sessions` returns paginated list with cursor
- [ ] `GET /sessions/:id` returns session detail with jobs
- [ ] `POST /sessions/:id/input` creates follow-up job with correct history assembly
- [ ] History truncation works at configured cap (e.g. 50 turns)
- [ ] `history_truncated` is true when cap exceeded
- [ ] `PATCH /sessions/:id` toggles retain_forever
- [ ] `DELETE /sessions/:id` cascades
- [ ] Job state transitions work correctly (no invalid transitions)
- [ ] Session status correctly derived from job statuses
- [ ] Loop N creates exactly N jobs
- [ ] Loop until sentinel creates 1 initial job
- [ ] Integration tests for session lifecycle
- [ ] `cargo test -p server` passes
