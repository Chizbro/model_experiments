# 02 - API Types Crate Design

## Goal
Design and implement all shared request/response types, error shapes, and ID types in `crates/api-types` so server, worker, and CLI all share a single contract. This is deliberate upfront design work — getting these types right prevents rework later.

## What to build
All types derive `Serialize`/`Deserialize` and are grouped by domain:

### Error types
- `ApiError { code: String, message: String, details: Option<serde_json::Value> }` — standard error envelope

### ID types
- `SessionId`, `JobId`, `WorkerId`, `TaskId`, `PersonaId`, `IdentityId`, `ApiKeyId` — newtype wrappers over `String` or `Uuid`

### Session types
- `CreateSessionRequest { repo_url, ref_, workflow, params, persona_id, identity_id, retain_forever }`
- `WorkflowType` enum: `Chat`, `LoopN`, `LoopUntilSentinel`, `Inbox`
- `SessionParams` — workflow-specific params (prompt, n, sentinel, agent_cli, model, branch_mode, branch_name_prefix)
- `SessionStatus` enum: `Pending`, `Running`, `Completed`, `Failed`
- `SessionSummary` (for list), `SessionDetail` (for get, includes jobs)
- `JobSummary { job_id, status, created_at, error_message, pull_request_url }`
- `SendInputRequest { message }`
- `PaginatedResponse<T> { items: Vec<T>, next_cursor: Option<String> }`

### Worker types
- `RegisterWorkerRequest { id, host, labels, capabilities, client_version }`
- `HeartbeatRequest { status: WorkerStatus, current_job_id }`
- `WorkerStatus` enum: `Idle`, `Busy`
- `WorkerSummary { worker_id, host, labels, status, last_seen_at }`
- `PullTaskResponse` — full task payload including credentials, prompt_context, task_input, params
- `TaskInput` — enum/struct for chat (first/followup), loop, inbox variants
- `TaskCompleteRequest { status, worker_id, branch, commit_ref, mr_title, mr_description, error_message, output, sentinel_reached, assistant_reply }`

### Log types
- `LogEntry { id, timestamp, level, session_id, job_id, worker_id, source, message }`
- `LogLevel` enum: `Debug`, `Info`, `Warn`, `Error`
- `SendLogsRequest` — Vec of worker log entries (no session/job — server fills those)
- `WorkerLogEntry { timestamp, level, message, source }`

### Identity types
- `IdentityStatus { has_git_token, has_agent_token }`
- `AuthStatus { git_token_status, git_provider, token_expires_at, message }`
- `UpdateIdentityRequest { agent_token, git_token, refresh_token }`

### API key types
- `CreateApiKeyRequest { label }`, `CreateApiKeyResponse { id, key, label, created_at }`
- `ApiKeySummary { id, label, created_at }`

### Persona types
- `CreatePersonaRequest { name, prompt }`, `PersonaDetail`, `PersonaSummary`

### Health types
- `HealthResponse { status }`, `IdleResponse { idle, pending_or_assigned_jobs }`

## Dependencies
- Task 01 (repo scaffolding)

## Design decisions
- Use `#[serde(rename_all = "snake_case")]` consistently
- Optional fields use `Option<T>` with `#[serde(skip_serializing_if = "Option::is_none")]`
- `BranchMode` enum: `Main`, `Pr` with serde rename to `"main"` / `"pr"`
- `AgentCli` enum: `ClaudeCode`, `Cursor` with serde rename
- Keep types in separate modules by domain (sessions.rs, workers.rs, logs.rs, etc.) with re-exports from lib.rs

## Test criteria
- [ ] `cargo build -p api-types` compiles
- [ ] Unit tests for serde round-trip (serialize then deserialize) for all request/response types
- [ ] Unit tests for enum serialization (e.g. `WorkflowType::Chat` serializes to `"chat"`)
- [ ] `cargo test -p api-types` passes
