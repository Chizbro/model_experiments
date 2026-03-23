# Agent Log Review Report

## Executive Summary

29 agents sequentially built a Rust monorepo (server, worker, CLI, web UI) across 2 days (2026-03-21 to 2026-03-22). The overall execution was **solid** — every feature was marked complete, builds pass, and tests were written. However, there are several patterns of concern.

---

## Issues Found

### 1. TECH_STACK.md Read Redundantly by Nearly Every Agent (HIGH)

**Every single agent** read `docs/TECH_STACK.md` in full — a ~200-line document that doesn't change. It's dumped verbatim in logs 02, 03, 04, 05, 06, 07, 08, 09, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29. That's **28 full reads of the same static file**. The prompt told each agent to read it, but the context dump system should have summarized prior agent outputs to avoid this. Each log contains 100-200 lines of identical TECH_STACK content — massive context waste.

### 2. `plan/design-system.md` — Checked For But Never Exists (LOW)

Multiple agents (01, 03, 07, 12, 17, 26, 28) attempt to read `plan/design-system.md` and find it doesn't exist. The prompt instructs them to read it, but the file was never created. Minor but indicates the orchestration prompt was never updated after discovering this.

### 3. Worker Test Criteria Left Unchecked (MEDIUM)

**Logs 13, 14, 15** (worker foundation, git operations, agent CLI execution) all show test criteria with `[ ]` (unchecked boxes) rather than `[x]`. For example:

- Log 13: "Integration test: start server + worker, verify registration and heartbeats" — noted as "deferred to spec 29"
- Log 14: All test criteria boxes unchecked despite the log claiming tests pass
- Log 15: All test criteria boxes unchecked

The agents marked these features "complete" despite acknowledging tests were deferred. Log 13 explicitly says: *"Integration test (server+worker) — requires running server; the code is structured to support it but a full integration test harness is deferred to spec 29"*. This is acceptable deferral, but the un-checked boxes suggest the agents copied the spec checklist without actually updating it.

### 4. No Integration Tests for Worker Crate (MEDIUM)

The worker crate (`crates/worker/`) has **zero integration tests**. All worker tests are unit tests (57-73 unit tests across logs 13-16). The e2e tests in log 29 test the server side only (via `tower::ServiceExt::oneshot`) — they don't actually run the worker binary. The mock agent script (`tests/mock_agent.sh`) was created but never used in any test.

### 5. Log 11 — Double File Open Bug (LOW)

In `logs/11-log-ingestion-storage.log`, the `write_local_logs` function opens the file **twice**:

```rust
if let Err(e) = tokio::fs::OpenOptions::new()
    .create(true).append(true).open(&file_path).await {
    // error handling
    return;
}
// ...then opens the SAME file again immediately after
match tokio::fs::OpenOptions::new()
    .create(true).append(true).open(&file_path).await {
```

The first open's result is checked for error but the file handle is never used. This is a logic bug — the first open is wasted.

### 6. Excessive Context Drawing in Log 17 (Chat Workflow) (MEDIUM)

Log 17 lists **22 files read** during implementation, including many that were irrelevant to the chat workflow feature (e.g., `sse.rs`, `oauth.rs`, `middleware/auth.rs`, `workers.rs`). The agent also launched an Explore agent that read "all relevant source files" on top of the manual reads. For a feature that only changed 7 files, this is ~3x the necessary context.

### 7. Log 18 (Loop Workflows) — Minimal Actual Work (LOW)

Log 18 acknowledges: *"Feature 18 (Loop Workflows) was mostly already implemented by previous tasks."* The actual change was ~10 lines (server-side sentinel double-check). Yet the agent still read the full TECH_STACK.md (200+ lines), the full spec, explored the full codebase, and produced a 560-line log. The ratio of work done to context consumed is very low.

### 8. Web UI Tests Are Minimal (MEDIUM)

Log 28 (CI Pipeline) reveals the web UI has **exactly 1 test** — a smoke test (`web/src/test/smoke.test.ts`). The spec for task 25 (Workers & Log Viewer) has all test criteria marked `[ ]` unchecked. No React component tests exist for any of the pages (dashboard, session detail, new session, settings, workers, personas, log viewer). Vitest was only added in task 28 (CI Pipeline) as an afterthought.

### 9. Log 29 Found Real Bugs in Pre-Existing Code (NOTABLE)

The e2e integration tests (log 29) discovered 3 real bugs:

1. `GET /sessions/:id` — UUID→String ColumnDecode error (missing `::text` cast)
2. `GET /sessions/:id` — Same issue with jobs query
3. `GET /sessions/:id/logs` — NULL job_id in control plane logs causing decode failure

These bugs would have been caught earlier if prior agents had written proper integration tests. The bugs existed since tasks 09 and 11.

### 10. All Existing Tests Had Wrong DB Password (NOTABLE)

Log 29 reveals: *"All existing test files had wrong DB password (harness vs harness_dev) — they were already failing before this task."* Files fixed: `api_keys_test.rs`, `tasks_test.rs`, `sse_test.rs`, `oauth_test.rs`, `identities_test.rs`, `chat_workflow_test.rs`. This means **agents 05-12 were claiming "all tests pass" but the integration tests couldn't have been running against the dev DB**. They likely ran against a separately configured database, or the tests were only passing because of an existing docker-compose postgres with the old password.

### 11. `detect_sentinel` Function Left Unused (LOW)

Multiple logs (15, 16, 18, 26, 27, 28) mention a pre-existing warning: `function detect_sentinel is never used`. This dead code persisted across 6+ agent runs. It was finally removed in log 28 (CI Pipeline) when clippy was enforced with `-D warnings`.

### 12. Inbox Workflow Never Implemented (MEDIUM)

The `WorkflowType::Inbox` variant exists in the enum and in the database, but no agent implemented the inbox workflow. The engine's `create_jobs_for_session` has an empty branch for Inbox: `WorkflowType::Inbox => { // No immediate jobs for inbox }`. The CLI has `inbox send` and `inbox list` commands stubbed out (log 20/21), but no server routes for inbox message handling exist. This is a feature gap that was never flagged.

---

## Summary Table

| Severity | Issue | Logs Affected |
|----------|-------|---------------|
| HIGH | TECH_STACK.md read 28 times redundantly | All |
| MEDIUM | Worker crate has zero integration tests | 13-16 |
| MEDIUM | Web UI has only 1 smoke test, no component tests | 22-25, 28 |
| MEDIUM | Existing integration tests had wrong DB password | 05-12, fixed in 29 |
| MEDIUM | Excessive context drawing for small changes | 17, 18 |
| MEDIUM | Inbox workflow left unimplemented, never flagged | All |
| LOW | design-system.md repeatedly checked, never exists | 01, 03, 07, 12, 17, 26, 28 |
| LOW | Double file open bug in log shipping | 11 |
| LOW | detect_sentinel dead code persisted 6+ agents | 15-28 |
| LOW | Worker spec test checkboxes never updated | 13, 14, 15 |
| NOTABLE | E2E tests (log 29) found 3 real bugs from tasks 09/11 | 29 |
