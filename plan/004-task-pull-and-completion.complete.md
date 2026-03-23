# 004: Task Pull, Assignment & Completion

## Goal
Workers can pull tasks from the server, execute them, and report completion. The server handles job assignment, stale-worker reclaim, lease expiry, and updates session state on completion.

## Scope
### Server endpoints
- `POST /workers/tasks/pull` — Worker requests work. Server: (1) reclaim jobs from stale workers (bounded by max_job_reclaims), (2) reclaim lease-expired jobs if job_lease_seconds > 0, (3) select oldest pending job, (4) assign to worker (set worker_id, status=assigned, assigned_at). Return task payload with credentials, or 204 if no work.
- `POST /workers/tasks/:id/complete` — Worker reports task done. Update job status, store branch/commit_ref/mr_title/error_message/output/assistant_reply/sentinel_reached. Trigger session state machine (check if session should transition to completed/failed). For loop_until_sentinel: if sentinel not reached, create next job.
- `POST /workers/tasks/:id/logs` — Worker sends log batch. Store in logs table with session_id/job_id/worker_id from job context. Broadcast to SSE listeners.

### Job reclaim logic (in pull_task)
```sql
-- Stale worker reclaim
UPDATE jobs SET worker_id = NULL, status = 'pending', reclaim_count = reclaim_count + 1
WHERE status = 'assigned'
  AND worker_id IN (SELECT id FROM workers WHERE last_seen_at < NOW() - INTERVAL '$stale_seconds seconds')
  AND reclaim_count < $max_job_reclaims;

-- Fail over-reclaimed jobs
UPDATE jobs SET status = 'failed', error_message = '[MAX_WORKER_LOSS_RETRIES]'
WHERE status = 'assigned'
  AND worker_id IN (SELECT id FROM workers WHERE last_seen_at < NOW() - INTERVAL '$stale_seconds seconds')
  AND reclaim_count >= $max_job_reclaims;
```

### Session state machine on task_complete
- Mark job completed/failed
- If workflow is chat: session status = job status
- If workflow is loop_n: check if all jobs done → session completed; if any failed → session failed
- If workflow is loop_until_sentinel: if sentinel_reached → session completed; if failed → session failed; otherwise create next job (pending)
- Update session.updated_at

### Task payload construction (pull response)
Build from session + job + identity:
- Resolve identity tokens (identity first, then session params override)
- Resolve persona prompt if persona_id set
- Build task_input based on workflow type
- Include credentials (git_token, agent_token)

## Prerequisites
- Spec 001 (foundation)
- Spec 002 (workers exist in DB)
- Spec 003 (sessions and jobs exist)

## Files to create/modify
- `crates/server/src/routes/workers.rs` — Add pull_task, task_complete, send_logs handlers
- `crates/server/src/engine/mod.rs` — Add task assignment logic, reclaim logic, session state transitions
- `crates/server/src/sse.rs` — Broadcast log entries to SSE subscribers

## Acceptance criteria
1. Worker pulls task → 200 with full task payload (credentials, prompt, repo_url, etc.)
2. Worker pulls when no work → 204
3. Task pull assigns job to worker (status=assigned, worker_id set)
4. Stale worker's job is reclaimed on next pull by another worker
5. Job exceeding max_job_reclaims is failed with `[MAX_WORKER_LOSS_RETRIES]`
6. Task complete with success → job completed, session state updated
7. Task complete with failed → job failed, session state updated
8. Loop_until_sentinel: task complete without sentinel → new job created
9. Loop_until_sentinel: task complete with sentinel_reached=true → session completed
10. Log batch accepted and stored in DB
11. `cargo test` — at least 6 tests (pull, complete, reclaim, sentinel logic)
12. `cargo clippy` clean

## Implementation notes
- Pull task should be a single DB transaction for reclaim + select + assign
- For chat multi-turn follow-up (POST /sessions/:id/input): defer to spec 010
- The task_input for chat first job is just `{ "prompt": session.params.prompt }`
- Credentials: never log token values, only whether they're present
