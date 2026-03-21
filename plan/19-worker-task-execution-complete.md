# 19 — Worker: end-to-end task execution

**Status:** complete  
**Dependencies:** 16, 17, 18, 11–14 (server)

## Objective

Full **pull → clone → run agent → commit/push (per params) → POST logs → POST complete** loop, including **chat** `assistant_reply`, **loop_until_sentinel** `output` + `sentinel_reached`, and Git failure messages surfaced via `error_message` ([ARCHITECTURE §9a–9b](../docs/ARCHITECTURE.md#9a-when-the-worker-attempts-commit-and-push)).

## Scope

**In scope**

- Work directory lifecycle per job (clean clone or reuse policy—document).
- **`branch_mode`**: main vs PR branch behavior per spec; partial PR creation may be stub if P1 ([PRODUCT O2](../docs/PRODUCT.md#optional--later))—but **must not** silently claim MR exists ([CLIENT_EXPERIENCE §8](../docs/CLIENT_EXPERIENCE.md#8-git-commit-push-and-prmr-outcomes)).
- **Planning step** failures (branch/MR text) as user-visible job errors where Architecture distinguishes them.

**Out of scope**

- Worker-side WebSocket/SSE (logs are POST batches only).

## Spec references

- [API_OVERVIEW — Send logs, Task complete](../docs/API_OVERVIEW.md#send-logs)
- [GIT_CLONE_SPEC](../docs/GIT_CLONE_SPEC.md)

## Acceptance criteria

- **Integration**: with test server + stub agent binary or `echo` shim, one full job succeeds; one failure path sets `error_message`.
- Compose smoke (task 26) runs **chat** on a real repo in dev.

## Testing

| When | What | Retest |
|------|------|--------|
| After implementation | `cargo test -p worker -- --ignored` optional E2E; primary integration in CI | CI + manual dev |

## Completed / Notes

- **Work dir:** Each job uses a **fresh clone** under `<REMOTE_HARNESS_WORK_DIR>/jobs/<job_id>/` (directory removed before clone). Default work root: `{temp}/remote_harness_worker_jobs`.
- **Implementation:** `crates/worker/src/task_execution.rs` (`execute_pulled_task`), `ControlPlaneClient` extended for `heartbeat_busy`, `post_task_logs`, `complete_task`.
- **`branch_mode`:** `main` keeps the checked-out branch when possible; **detached HEAD** → new branch `rh/job-<hex>`; `pr` → `{branch_name_prefix}/job-<hex>` (default prefix `rh/`).
- **`mr_title`:** Set only for `branch_mode: pr` as a **suggested** title (`Harness: <branch>`); control plane still owns real PR/MR creation ([Architecture §9b](../docs/ARCHITECTURE.md#9b-when-the-control-plane-creates-a-prmr)).
- **Stub agent (tests/dev):** `REMOTE_HARNESS_STUB_AGENT=1` (or `true`/`yes`); optional `REMOTE_HARNESS_STUB_AGENT_STDOUT` for scripted output (e.g. sentinel tests).
- **`file://`:** Supported in `git_ops` for local bare remotes (documented in [GIT_CLONE_SPEC](../docs/GIT_CLONE_SPEC.md)); integration test `crates/worker/tests/e2e_file_remote_stub.rs` (Unix + `git` on PATH).
- **Heartbeats:** Background loop sends **busy** + `current_job_id` while executing a task.
