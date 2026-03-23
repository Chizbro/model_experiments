# 10 - Task Dispatch & Completion

## Goal
Implement the server-side task pull (dispatch) and task complete endpoints — the core worker-to-server interaction for executing jobs. Includes job reclaim from stale workers and bounded retries.

## What to build

### Task routes (`crates/server/src/routes/tasks.rs`)

**POST /workers/tasks/pull**
- Request: optional `worker_id`
- Step 1: **Stale reclaim** — in one transaction, reclaim jobs from stale workers:
  ```sql
  UPDATE jobs SET worker_id = NULL, status = 'pending', reclaim_count = reclaim_count + 1
  WHERE status = 'assigned' AND worker_id IN (SELECT id FROM workers WHERE last_seen_at < $stale_cutoff)
  AND reclaim_count < $max_job_reclaims
  ```
- Step 1b: Fail jobs over reclaim cap:
  ```sql
  UPDATE jobs SET status = 'failed', error_message = '[MAX_WORKER_LOSS_RETRIES]'
  WHERE status = 'assigned' AND worker_id IN (SELECT id FROM workers WHERE last_seen_at < $stale_cutoff)
  AND reclaim_count >= $max_job_reclaims
  ```
- Step 1c: **Lease reclaim** (when `job_lease_seconds > 0`): fail jobs in `assigned` state longer than lease:
  ```sql
  UPDATE jobs SET status = 'failed', error_message = '[JOB_LEASE_EXPIRED]'
  WHERE status = 'assigned' AND assigned_at < now() - $job_lease_seconds
  ```
- Step 2: Select one `pending` job, assign to worker (set worker_id, status = 'assigned', assigned_at = now())
- Step 3: Build full pull response: resolve credentials from identity, build task_input, include prompt_context (persona), params, credentials
- Response (task available): `200 { task_id, job_id, session_id, repo_url, ref, workflow, prompt_context, task_input, params, credentials }`
- Response (no work): `204 No Content`

**POST /workers/tasks/:id/complete**
- Validate task is assigned to the reporting worker
- Update job: status, branch, commit_ref, mr_title, mr_description, error_message, output, sentinel_reached, assistant_reply
- Update session status based on job outcomes
- For loop_until_sentinel: if `sentinel_reached` is true, mark session completed; otherwise create next job
- For loop_n: if all jobs completed, mark session completed
- For chat: update session, store assistant_reply for future history assembly
- Trigger PR/MR creation if applicable (defer actual implementation to Task 19)
- Response: `200 { "ok": true }`
- `404` if task unknown or already completed

### PR/MR creation hook
- After successful task complete in PR mode: stub/hook for PR creation (actual provider API call in Task 19)
- Log that PR/MR creation would happen here

## Dependencies
- Task 08 (worker registration — need workers in DB)
- Task 09 (session/job state machine — need sessions and jobs)
- Task 06 (identity credentials — for credential resolution)

## Test criteria
- [ ] Pull with no pending jobs returns `204`
- [ ] Pull assigns a pending job to the requesting worker and returns full payload
- [ ] Credentials (git_token, agent_token) included in pull response
- [ ] Task complete with `success` marks job completed
- [ ] Task complete with `failed` marks job failed with error_message
- [ ] Stale worker's jobs are reclaimed during pull
- [ ] Jobs exceeding max_job_reclaims are failed with `[MAX_WORKER_LOSS_RETRIES]`
- [ ] Lease expiry fails jobs with `[JOB_LEASE_EXPIRED]` when configured
- [ ] Loop until sentinel: `sentinel_reached: true` completes the session
- [ ] Loop until sentinel: `sentinel_reached: false` creates next iteration job
- [ ] Chat: assistant_reply stored for future history
- [ ] Session status updates correctly after job completion
- [ ] Integration tests for pull → complete cycle
- [ ] `cargo test -p server` passes
