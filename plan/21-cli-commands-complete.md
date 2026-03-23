# 21 - CLI Commands (Sessions, Logs, Workers, Credentials)

## Goal
Implement all CLI commands that map to the control plane API. Each command should provide a good terminal UX with clear output and error messages.

## What to build

### Session commands (`crates/cli/src/commands/session.rs`)

**session start**
- Collect params from flags: --repo, --workflow, --prompt, --n, --sentinel, --agent-cli, --model, --branch-mode, --persona-id, --identity-id, --retain-forever
- POST /sessions
- Print: `Session created: {session_id}\nStatus: pending\nWeb UI: {web_url}`

**session list**
- GET /sessions with optional --status filter
- Print table: session_id | workflow | status | repo | created_at

**session show**
- GET /sessions/:id
- Print session detail + jobs table (job_id | status | error | PR URL)

**session delete**
- DELETE /sessions/:id
- Confirm before delete (unless --force)
- Print: `Session {id} deleted`

### Attach command (`crates/cli/src/commands/attach.rs`)
- GET /sessions/:id to verify session exists and get status
- Open SSE to GET /sessions/:id/logs/stream
- Also open SSE to GET /sessions/:id/events
- Print log entries as they arrive (formatted: `[timestamp] [level] message`)
- Print session events (e.g. "Job started", "Session completed")
- For chat sessions: accept user input from stdin, POST /sessions/:id/input
- Exit when session reaches terminal state

### Logs commands (`crates/cli/src/commands/logs.rs`)

**logs tail**
- First: GET /sessions/:id/logs (paginate until all loaded, or use --last N)
- Then: open SSE to /sessions/:id/logs/stream
- Print formatted log entries
- Support --job-id and --level filters

**logs delete**
- DELETE /sessions/:id/logs?job_id=X
- Confirm before delete (unless --force)

### Worker commands (`crates/cli/src/commands/workers.rs`)

**workers list**
- GET /workers
- Print table: worker_id | host | platform | status | last_seen

**workers clear**
- DELETE /workers/:id
- Print: `Worker {id} removed`

### Credential commands (`crates/cli/src/commands/credentials.rs`)

**credentials show**
- GET /identities/:id (default "default")
- GET /identities/:id/auth-status
- Print: token status, expiry info

**credentials set**
- PATCH /identities/:id with provided tokens
- Print: `Credentials updated`

### API key commands (`crates/cli/src/commands/api_keys.rs`)

**api-key create**
- POST /api-keys with --label
- Print: `API key created: {key}\nSave this key — it will not be shown again.`

**api-key list**
- GET /api-keys
- Print table: id | label | created_at

**api-key revoke**
- DELETE /api-keys/:id
- Print: `API key {id} revoked`

## Dependencies
- Task 20 (CLI foundation — clap structure, API client)
- All server endpoints must exist (Tasks 05-12)

## Test criteria
- [ ] `session start` creates a session and prints session_id
- [ ] `session list` displays sessions in table format
- [ ] `session show` displays detail with jobs
- [ ] `attach` connects to SSE and streams logs
- [ ] `attach` for chat sessions accepts stdin input
- [ ] `logs tail` loads history then streams
- [ ] `logs tail --last 10` shows only last 10 entries
- [ ] `workers list` shows workers with status
- [ ] `credentials set` updates identity tokens
- [ ] `credentials show` displays token status (never actual tokens)
- [ ] `api-key create` returns and displays the key
- [ ] All error cases show human-readable error messages on stderr
- [ ] All commands exit with code 0 on success, 1 on failure
- [ ] `cargo test -p cli` passes
