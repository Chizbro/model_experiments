# 16 — Worker: HTTP client, register, heartbeat, pull loop

**Status:** completed  
**Dependencies:** 09, 10 (server endpoints stable)

## Objective

Worker process **registers** with **`client_version`**, sends **heartbeats**, and **polls** `POST /workers/tasks/pull` with backoff—structured as a small **`ControlPlaneClient`** module to avoid scattering `reqwest` calls ([ARCHITECTURE §3](../docs/ARCHITECTURE.md#3-worker-discovery--registration)).

## Scope

**In scope**

- Config: `CONTROL_PLANE_URL`, API key, heartbeat interval.
- Idempotent registration behavior on restart (handle **409** per spec).
- **Version string** from build metadata (e.g. `env!("CARGO_PKG_VERSION")`).
- Log structured errors; no secrets in logs.

**Out of scope**

- Executing Git or agent (tasks 17–19).

## Spec references

- [API_OVERVIEW §9](../docs/API_OVERVIEW.md#9-worker--control-plane)
- [TECH_STACK §2](../docs/TECH_STACK.md#2-workers-rust)

## Acceptance criteria

- **Integration test** with mock HTTP server or wiremock verifying request shapes and auth headers.
- Worker binary runs and idles against real server in Compose (smoke in 26).

## Testing

| When | What | Retest |
|------|------|--------|
| After implementation | `cargo test -p worker` | CI |

---

## Completed / Notes

- **`ControlPlaneClient`** (`crates/worker/src/control_plane.rs`): register (201/409), heartbeat (idle/busy), pull (204 / 200 JSON), `post_task_logs`, `complete_task` / `complete_task_failed` (extended in task 19 for full execution).
- **Config** (`crates/worker/src/config.rs`): `CONTROL_PLANE_URL` or `REMOTE_HARNESS_URL`, `API_KEY` or `REMOTE_HARNESS_API_KEY`, optional `WORKER_ID`, `WORKER_HOST`, `WORKER_HEARTBEAT_INTERVAL_SECS` (default 30s).
- **Wiremock** integration tests: `crates/worker/tests/control_plane_client.rs`.
- **Docker:** `Dockerfile.worker`; `docker-compose.yml` **worker** service enabled.
- **Docs:** `crates/worker/README.md`, [TECH_STACK § auth](../docs/TECH_STACK.md), [IMPLEMENTATION_BOUNDARIES](../docs/IMPLEMENTATION_BOUNDARIES.md).
