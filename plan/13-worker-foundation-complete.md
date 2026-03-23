# 13 - Worker Foundation

## Goal
Build the worker binary skeleton: configuration, control plane API client, registration, heartbeat loop, and task polling loop. The worker should be able to register, heartbeat, and poll for tasks (returning 204/no work) without executing anything yet.

## What to build

### Configuration (`crates/worker/src/config.rs`)
- Load from env or YAML config file (`~/.config/remote-harness-worker/config.yaml`)
- Precedence: env > config file
- Fields:
  - `CONTROL_PLANE_URL` / `REMOTE_HARNESS_URL` (required)
  - `API_KEY` / `REMOTE_HARNESS_API_KEY` (required)
  - `WORKER_ID` (optional — auto-generate from hostname + random suffix if not set)
  - `WORKER_HOST` (optional — defaults to hostname)
  - `WORKER_LABELS` (optional — JSON or comma-separated k=v pairs, must include `platform`)
  - `HEARTBEAT_INTERVAL_SECS` (default 30)
  - `POLL_INTERVAL_SECS` (default 5)
  - `LOG_DIR` (default `./logs/`)
  - `REMOTE_HARNESS_CONFIG` (path to config file, default `~/.config/remote-harness-worker/config.yaml`)

### API client (`crates/worker/src/api_client.rs`)
- HTTP client (reqwest) with base URL and API key header
- Methods:
  - `register(request) -> Result<RegisterResponse>`
  - `heartbeat(worker_id, request) -> Result<()>`
  - `pull_task(worker_id) -> Result<Option<PullTaskResponse>>`
  - `send_logs(task_id, entries) -> Result<()>`
  - `complete_task(task_id, request) -> Result<()>`
- Retry on 5xx with backoff (reqwest-retry or manual)
- Handle 404 on heartbeat (worker unknown — need to re-register)

### Main loop (`crates/worker/src/main.rs`)
1. Load config
2. Init tracing (structured JSON to stdout + local file)
3. Auto-detect platform label (macos/linux/windows/wsl)
4. Register with control plane (retry on failure)
5. Spawn heartbeat task (periodic POST /workers/:id/heartbeat)
6. Enter poll loop:
   - `POST /workers/tasks/pull`
   - If 204/no task: wait `POLL_INTERVAL_SECS`, retry
   - If 200/task: hand off to task executor (stub for now — just log and complete as success)
   - On heartbeat 404: re-register

### Platform detection
- Detect OS at build time (`cfg!(target_os)`) and runtime for WSL (`/proc/version` check)
- Set `platform` label automatically

### Graceful shutdown
- Handle SIGINT/SIGTERM
- Complete current task if running, then exit
- Cancel heartbeat and poll loops

## Dependencies
- Task 02 (api-types — shared request/response types)
- Task 08 (server worker registration — endpoint must exist)

## Test criteria
- [ ] Worker starts and registers with control plane
- [ ] Heartbeat sends periodically (visible in server logs / DB)
- [ ] Worker shows as "active" in `GET /workers`
- [ ] Poll returns 204 when no tasks, worker retries after interval
- [ ] Worker re-registers after heartbeat 404
- [ ] Platform label auto-detected correctly (at least for current OS)
- [ ] Graceful shutdown on SIGINT
- [ ] Config loads from env vars
- [ ] Config loads from YAML file when env not set
- [ ] `cargo test -p worker` passes
- [ ] Integration test: start server + worker, verify registration and heartbeats
