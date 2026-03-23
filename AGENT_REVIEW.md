# Agent Review: Implementation Log Analysis

## 001–003: Workspace, Workers, Sessions
Clean. No issues found. Specs followed precisely.

## 004: Task Pull & Completion
- **Lease expiry untested** — reclaim-on-lease-expiry code path has no test coverage
- **Session "running" transition** — happens on first job assignment in `pull_task`, not on completion. Internally consistent but worth confirming this is intended

## 005: Log Streaming & History
- **Silent event drops** — `BroadcastStream` handles `Lagged` by skipping; clients silently lose events if they fall behind
- **Event ordering race** — `job_completed` emitted before state machine logic, `session completed` emitted after. No ordering guarantee between the two; no tests for ordering
- **Inefficient broadcast** — all events go to all subscribers, filtered client-side

## 007–008: CLI
- **Dead code left in** — `base_url()` on `ApiClient` kept with `#[allow(dead_code)]` for "future use"; violates "no stubs" rule
- **rpassword version coupling** — code uses v5-specific `read_password()` API; no version pin in Cargo.toml
- **Stdin blocking workaround** — dedicated thread + tokio channel for blocking stdin in async attach loop; potential deadlock risk
- **No integration tests** — attach, SSE reconnection, and full CLI workflows are untested
- **Undocumented fallback env vars** — `CONTROL_PLANE_URL` / `API_KEY` accepted as fallbacks but not documented

## 009–010: Web UI
Clean. Build, lint, and typecheck all passed. No issues.

## 011–012: Chat Multi-Turn, Identities, API Keys, Personas
- **Bootstrap endpoint info leak** — `POST /api-keys/bootstrap` is unauthenticated and reveals whether the system has been initialized (intentional per spec, but noted)
- Otherwise clean. 189 tests passing.

## 013–015: OAuth, PR/MR, Docker
- **Silent PR creation failure** — PR/MR created in a spawned `tokio::task` after HTTP response returns; if it fails, no one is notified
- **Expired token fallback** — if OAuth refresh fails, the expired token is still returned with a warning log rather than failing the operation
- **Out-of-scope clippy fixes** — unrelated lint fixes in api_keys.rs and identities.rs bundled into these specs

## 016: E2E Verification
- **No OAuth/PR E2E tests** — OAuth callback flows and PR creation endpoint have zero E2E coverage
- **Dead code annotation** — `#[allow(dead_code)]` on `TestAppState.db` instead of refactoring test helpers
- **Postgres-only tests** — all E2E tests require a real Postgres instance; skip if unavailable

## Top Concerns (by severity)

1. **Silent PR creation failure** — async fire-and-forget with no error reporting back to the user
2. **Silent SSE event drops** — clients can miss events with no indication
3. **No E2E tests for OAuth or PR/MR** — two critical features with zero integration coverage
4. **Expired token fallback** — refresh failure silently returns a stale token
5. **Dead code / `#[allow(dead_code)]`** — appears in CLI and tests; violates project rules
6. **Lease expiry untested** — code path exists but no test exercises it
