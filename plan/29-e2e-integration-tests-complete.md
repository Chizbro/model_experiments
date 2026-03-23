# 29 - End-to-End Integration Tests

## Goal
Build comprehensive integration tests that validate the full system works end-to-end: server + worker + real Git operations + workflow execution. These tests are the final validation that all components work together.

## What to build

### Test infrastructure (`tests/integration/`)

**Test harness**
- Start PostgreSQL (test container or docker-compose)
- Start server with test config (random port, test DB)
- Start worker(s) pointing at test server
- API client for test assertions
- Cleanup: drop test DB, stop processes

**Mock agent CLI**
- Simple script/binary that simulates Claude Code output
- Accepts prompt via args, outputs configurable response to stdout
- Supports: normal success, non-zero exit, output containing sentinel text
- Set via env var so worker finds it on PATH instead of real Claude Code

### Test scenarios

**T1: Worker registration and discovery**
- Start server + worker
- Assert worker appears in GET /workers as "active"
- Stop worker, wait for stale threshold
- Assert worker shows as "stale"

**T2: Chat workflow (single turn)**
- Set credentials on default identity
- Create chat session with repo URL + prompt
- Worker pulls task, clones repo, runs mock agent, commits, completes
- Assert: session completed, job has commit_ref, logs exist

**T3: Chat workflow (multi-turn)**
- Create chat session, first job completes with assistant_reply
- Send input (follow-up message)
- Second job pulled with correct history (session_prompt, history, history_assistant)
- Assert: history assembled correctly, history_truncated correct

**T4: Loop N workflow**
- Create loop_n session with n=3
- Assert 3 jobs created
- All 3 jobs pulled and completed
- Session status transitions: pending -> running -> completed

**T5: Loop until sentinel**
- Create loop_until_sentinel with sentinel "DONE"
- First iteration: mock agent outputs "still working" -> no sentinel
- Second iteration: mock agent outputs "task DONE" -> sentinel reached
- Assert: session completed after 2 iterations

**T6: Job reclaim from stale worker**
- Start 2 workers
- Worker A pulls task, then "dies" (stop it)
- Wait for stale detection
- Worker B pulls -> gets reclaimed task
- Assert: reclaim_count incremented, task completes

**T7: Max reclaim retries**
- Configure max_job_reclaims=1
- Worker pulls task, dies
- Task reclaimed, second worker pulls and dies
- Assert: job failed with [MAX_WORKER_LOSS_RETRIES]

**T8: API key lifecycle**
- Bootstrap first key
- Use key to create another key
- Revoke original key, assert it stops working
- New key still works

**T9: Log streaming**
- Create session, subscribe to SSE log stream
- Worker executes task, sends logs
- Assert: logs appear in SSE stream
- Assert: GET /sessions/:id/logs returns all logs after completion

**T10: PR mode (mocked GitHub API)**
- Create session with branch_mode="pr"
- Mock GitHub API for PR creation
- Worker commits to branch, completes successfully
- Assert: pull_request_url set on job

**T11: Credential validation**
- Try creating session without agent_token on identity
- Assert: 400 error about missing credentials

### Test utilities
- `TestServer` — starts server, provides URL and cleanup
- `TestWorker` — starts worker with configurable mock agent
- `TestClient` — typed HTTP client for assertions
- `assert_eventually!(condition, timeout)` — poll until condition true or timeout

## Dependencies
- All previous tasks (01-28) must be complete

## Test criteria
- [ ] All 11 test scenarios pass
- [ ] Tests can run in CI (no external dependencies beyond Docker)
- [ ] Tests clean up after themselves (no leaked processes, DBs, or files)
- [ ] Tests complete in reasonable time (<5 minutes total)
- [ ] Test failures produce clear, diagnostic output
- [ ] `cargo test --test integration` runs all e2e tests
- [ ] Tests are idempotent (can run multiple times)
