# 006 — Worker Binary

**Status:** complete

## Summary

Implement the complete worker binary for the Remote Harness project. The worker is a long-running Rust process that registers with the control plane, sends periodic heartbeats, polls for tasks, executes them (clone repo, run agent CLI, commit/push), and reports results.

## Deliverables

- `crates/worker/src/main.rs` — Entry point: load config, register, spawn heartbeat, run task loop
- `crates/worker/src/config.rs` — Config from env vars
- `crates/worker/src/api_client.rs` — HTTP client to control plane (register, heartbeat, pull_task, send_logs, task_complete)
- `crates/worker/src/git_ops.rs` — Git clone/checkout/branch/commit/push following GIT_CLONE_SPEC.md
- `crates/worker/src/agent_runner.rs` — Spawn Claude Code or Cursor CLI as child process
- `crates/worker/src/logger.rs` — Dual-write: local files + buffered POST to server
- `crates/worker/src/task_loop.rs` — Main loop: pull → execute → report
- `crates/api-types/src/lib.rs` — Shared types for worker ↔ control plane contracts
- `Cargo.toml` — Workspace root

## Verification

- `cargo build -p worker` — succeeds
- `cargo clippy -p worker -- -D warnings` — passes (zero warnings)
- `cargo test -p worker` — 13 tests pass (config parsing, URL embedding, username selection, log batching, branch naming, platform detection)
