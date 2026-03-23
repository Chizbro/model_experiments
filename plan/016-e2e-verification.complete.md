# 016: End-to-End Verification & Integration Tests

## Goal
Verify the entire system works end-to-end: create a session, have a worker pick it up, execute (mock agent), see logs, and confirm final state. Fix any integration issues found. This is a quality gate — nothing ships until this passes.

## Scope
### Integration test suite
Create a test harness that:
1. Starts a Postgres instance (via testcontainers or assumes a running instance)
2. Starts the server programmatically (or as a subprocess)
3. Starts a worker (subprocess or in-process)
4. Uses the CLI (or direct HTTP calls) to:
   a. Check health
   b. Set credentials (PATCH /identities/default)
   c. Create a chat session
   d. Wait for the worker to pull and complete the task
   e. Verify session status = completed
   f. Verify logs exist (GET /sessions/:id/logs)
   g. Verify job has commit_ref or appropriate status

### Mock agent
Since we can't rely on Claude Code or Cursor being installed, create a simple mock:
- A script/binary that reads stdin (prompt), writes "Mock agent output: {prompt}" to stdout, exits 0
- Set CURSOR_AGENT_PATH or CLAUDE_CLI_PATH to point to this mock
- Worker should use it seamlessly

### Success criteria verification (from PRODUCT.md)
- [ ] One control plane + one worker: run a "chat" workflow on a repo and see the session complete
- [ ] Start a session from the CLI; verify logs are visible
- [ ] At least one loop workflow runs end-to-end with logs

### Additional checks
- API error responses match spec (standard error body)
- Pagination works on sessions, workers, logs
- Stale worker detection works (kill worker, verify stale, verify job reclaim)
- Session delete cascades properly
- CORS headers present on responses

### Fix any issues found
This spec is also a "fix it" pass. If integration tests reveal bugs, fix them in this spec. Document what was broken and what was fixed in the log.

## Prerequisites
- All previous specs (001-015) complete

## Files to create/modify
- `tests/` directory at workspace root (or within crates/server/tests/)
- `tests/e2e_chat.rs` — Chat workflow e2e
- `tests/e2e_loop.rs` — Loop N workflow e2e
- `tests/e2e_sentinel.rs` — Loop until sentinel e2e
- `tests/mock_agent.sh` — Mock agent script
- Any bug fixes across the codebase

## Acceptance criteria
1. E2E chat test passes: session created → worker executes → logs exist → session completed
2. E2E loop test passes: session with n=3 → 3 jobs completed → session completed
3. E2E sentinel test passes: session with sentinel "DONE" → mock agent outputs "DONE" → session completed after 1 iteration
4. Stale worker reclaim test: kill worker → job reclaimed → another worker picks it up
5. All PRODUCT.md success criteria checked
6. `cargo test --all-targets` passes (unit + integration)
7. `cargo clippy --all-targets -- -D warnings` clean
8. `npm run build` and `npm run lint` pass for web
9. Docker compose starts and all services are healthy

## Implementation notes
- For the mock agent: a shell script is simplest. `#!/bin/bash\nread prompt\necho "Mock response to: $prompt"\nexit 0`
- E2E tests can use reqwest to call the API directly, without needing the CLI binary.
- For repo cloning in tests: use a small public repo (e.g., a test repo you create, or a known small repo). Or mock git operations for unit tests and only test real clone in a separate integration test.
- Use `tokio::time::timeout` in tests to avoid hanging on poll waits.
