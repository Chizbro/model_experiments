# Design System & Shared Conventions

This document defines architectural patterns and conventions that **every agent** must follow. Read this before implementing any feature spec.

---

## 1. Repository layout

```
remote_harness/
├── crates/
│   ├── api-types/       # Shared request/response types, IDs, enums (serde)
│   │   ├── Cargo.toml
│   │   └── src/lib.rs
│   ├── server/          # Control plane (axum, sqlx, tokio)
│   │   ├── Cargo.toml
│   │   ├── migrations/  # SQLx migrations (forward-only!)
│   │   └── src/
│   │       ├── main.rs
│   │       ├── config.rs
│   │       ├── db.rs           # Pool init, migration runner
│   │       ├── error.rs        # AppError type, IntoResponse
│   │       ├── auth.rs         # API key middleware
│   │       ├── routes/         # One module per resource group
│   │       │   ├── mod.rs
│   │       │   ├── health.rs
│   │       │   ├── sessions.rs
│   │       │   ├── workers.rs
│   │       │   ├── logs.rs
│   │       │   ├── identities.rs
│   │       │   ├── personas.rs
│   │       │   ├── api_keys.rs
│   │       │   └── oauth.rs
│   │       ├── engine/         # Workflow engine, job state machine
│   │       │   ├── mod.rs
│   │       │   └── workflows.rs
│   │       └── sse.rs          # SSE helpers (log stream, session events)
│   ├── worker/          # Worker binary
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs
│   │       ├── config.rs
│   │       ├── api_client.rs   # HTTP client to control plane
│   │       ├── git_ops.rs      # Clone, checkout, commit, push (git2)
│   │       ├── agent_runner.rs # Spawn Claude Code / Cursor CLI
│   │       ├── logger.rs       # Dual-write: local files + POST to server
│   │       └── task_loop.rs    # Main poll → execute → complete loop
│   └── cli/             # CLI binary
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs
│           ├── config.rs
│           ├── api_client.rs
│           ├── commands/
│           │   ├── mod.rs
│           │   ├── health.rs
│           │   ├── session.rs
│           │   ├── logs.rs
│           │   ├── workers.rs
│           │   ├── credentials.rs
│           │   └── api_keys.rs
│           └── sse.rs
├── web/                 # Web UI (Vite + React + TypeScript)
│   ├── package.json
│   ├── tsconfig.json
│   ├── vite.config.ts
│   ├── index.html
│   ├── tailwind.config.js
│   └── src/
│       ├── main.tsx
│       ├── App.tsx
│       ├── api/             # API client + types (mirrors api-types)
│       │   ├── client.ts
│       │   ├── types.ts
│       │   └── sse.ts
│       ├── pages/
│       │   ├── Dashboard.tsx
│       │   ├── SessionDetail.tsx
│       │   ├── SessionCreate.tsx
│       │   ├── Workers.tsx
│       │   └── Settings.tsx
│       └── components/
│           ├── LogViewer.tsx
│           ├── SessionList.tsx
│           └── Layout.tsx
├── Cargo.toml           # Workspace root
├── Cargo.lock
├── Dockerfile
├── docker-compose.yml
├── docs/
├── plan/
└── logs/
```

---

## 2. Rust conventions

### 2.1 Workspace Cargo.toml

```toml
[workspace]
members = ["crates/*"]
resolver = "2"

[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
uuid = { version = "1", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["json", "env-filter"] }
anyhow = "1"
thiserror = "2"
```

### 2.2 api-types crate

All request/response types shared between server, worker, and CLI live here. Use `#[derive(Debug, Clone, Serialize, Deserialize)]` on everything. IDs are `String` (UUIDs serialized as strings). Timestamps are `chrono::DateTime<Utc>` serialized as ISO 8601.

Key types to define:
- `SessionStatus` enum: `Pending`, `Running`, `Completed`, `Failed`
- `JobStatus` enum: `Pending`, `Assigned`, `Running`, `Completed`, `Failed`
- `WorkflowType` enum: `Chat`, `LoopN`, `LoopUntilSentinel`, `Inbox`
- `AgentCli` enum: `ClaudeCode`, `Cursor`
- `BranchMode` enum: `Main`, `Pr`
- Request/response structs for every API endpoint

### 2.3 Error handling (server)

Use a single `AppError` enum that implements `IntoResponse`:

```rust
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("unauthorized")]
    Unauthorized,
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("internal: {0}")]
    Internal(#[from] anyhow::Error),
}
```

All error responses use the standard JSON shape from API_OVERVIEW §2:
```json
{ "error": { "code": "string", "message": "string", "details": {} } }
```

### 2.4 Database

- Use `sqlx` with compile-time checking disabled (use `query_as!` with runtime checking or `query` with manual mapping).
- All migrations in `crates/server/migrations/` with format `YYYYMMDDHHMMSS_description.sql`.
- **NEVER modify an existing migration.** Always add new ones.
- Use `UUID` for primary keys (generated in Rust with `uuid::Uuid::new_v4()`).
- Use `TIMESTAMPTZ` for all timestamps.
- Naming: `snake_case` for tables and columns.

### 2.5 API key auth middleware

Extract from `Authorization: Bearer <key>` or `X-API-Key: <key>` header. Compare against known keys (env `API_KEY`/`API_KEYS` comma-separated, plus DB-issued keys as SHA-256 hashes). Return 401 with standard error body on failure. Health endpoints (`/health`, `/ready`, `/health/idle`) skip auth.

### 2.6 Structured logging

All components use `tracing` with JSON output:
```rust
tracing_subscriber::fmt()
    .json()
    .with_env_filter(EnvFilter::from_default_env())
    .init();
```

Include `session_id`, `job_id`, `worker_id` as span fields where available.

---

## 3. Database schema (core tables)

```sql
-- Workers
CREATE TABLE workers (
    id TEXT PRIMARY KEY,
    host TEXT NOT NULL,
    labels JSONB NOT NULL DEFAULT '{}',
    capabilities JSONB NOT NULL DEFAULT '[]',
    client_version TEXT,
    status TEXT NOT NULL DEFAULT 'active',  -- 'active' or 'stale'
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Identities (BYOL credentials)
CREATE TABLE identities (
    id TEXT PRIMARY KEY,
    agent_token TEXT,           -- encrypted or plain (v1: plain)
    git_token TEXT,
    refresh_token TEXT,
    token_expires_at TIMESTAMPTZ,
    git_provider TEXT,          -- 'manual', 'oauth_github', 'oauth_gitlab'
    git_base_url TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Seed default identity
INSERT INTO identities (id) VALUES ('default');

-- Sessions
CREATE TABLE sessions (
    id TEXT PRIMARY KEY,
    repo_url TEXT NOT NULL,
    ref_name TEXT NOT NULL DEFAULT 'main',
    workflow TEXT NOT NULL,     -- 'chat', 'loop_n', 'loop_until_sentinel', 'inbox'
    status TEXT NOT NULL DEFAULT 'pending',
    params JSONB NOT NULL DEFAULT '{}',
    identity_id TEXT NOT NULL DEFAULT 'default' REFERENCES identities(id),
    persona_id TEXT,
    retain_forever BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Jobs (one per iteration/turn)
CREATE TABLE jobs (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    worker_id TEXT REFERENCES workers(id),
    status TEXT NOT NULL DEFAULT 'pending',  -- pending, assigned, running, completed, failed
    iteration_index INTEGER NOT NULL DEFAULT 0,
    task_input JSONB NOT NULL DEFAULT '{}',
    error_message TEXT,
    branch TEXT,
    commit_ref TEXT,
    mr_title TEXT,
    mr_description TEXT,
    pull_request_url TEXT,
    output TEXT,
    assistant_reply TEXT,
    sentinel_reached BOOLEAN NOT NULL DEFAULT FALSE,
    reclaim_count INTEGER NOT NULL DEFAULT 0,
    assigned_at TIMESTAMPTZ,
    retain_forever BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Logs (central store)
CREATE TABLE logs (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    job_id TEXT,
    worker_id TEXT,
    level TEXT NOT NULL DEFAULT 'info',
    source TEXT NOT NULL DEFAULT 'control_plane',
    message TEXT NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_logs_session_timestamp ON logs(session_id, timestamp);
CREATE INDEX idx_logs_job ON logs(job_id) WHERE job_id IS NOT NULL;

-- API keys (issued, hashed)
CREATE TABLE api_keys (
    id TEXT PRIMARY KEY,
    key_hash TEXT NOT NULL,
    label TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Personas
CREATE TABLE personas (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    prompt TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Inbox tasks (P1, create table now for schema completeness)
CREATE TABLE inbox_tasks (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL,
    payload JSONB NOT NULL DEFAULT '{}',
    persona_id TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    enqueued_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

---

## 4. Server application state

```rust
#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub config: AppConfig,
    pub log_broadcaster: Arc<LogBroadcaster>,      // For SSE log streams
    pub event_broadcaster: Arc<EventBroadcaster>,   // For SSE session events
}
```

Pass `AppState` via axum's `State` extractor to all handlers.

---

## 5. Web UI conventions

- **Vite + React 18 + TypeScript** (strict mode)
- **Tailwind CSS** + **shadcn/ui** components
- **TanStack Query** for server state
- **React Router** for routing
- API client: plain `fetch` wrapper with typed responses; SSE via `EventSource`
- Store control plane URL and API key in `localStorage`
- All API calls go through a single `apiClient` that attaches the API key header
- No server-side rendering — pure SPA

---

## 6. Testing conventions

- **Rust:** `cargo test` for unit tests; integration tests in `tests/` directories within each crate.
- **Server integration tests:** Use `sqlx::test` or spin up a test DB; test routes via `axum::test` helpers (or `reqwest` against a running instance).
- **Web:** Vitest for unit tests; build must succeed (`npm run build`).
- **After each feature:** Run `cargo build --all-targets`, `cargo clippy --all-targets -- -D warnings`, `cargo test`.
- **After web changes:** Run `npm run build` and `npm run lint` in `web/`.

---

## 7. Agent logging requirements

Every agent **must** dump its full working context to `logs/{feature-name}.log` after completing a spec. This is not a summary — it's the complete record of what was done, decisions made, files created/modified, and test results.

---

## 8. Migration safety

- **NEVER** edit an existing migration file.
- **ALWAYS** create a new migration for schema changes.
- Migration files: `crates/server/migrations/YYYYMMDDHHMMSS_description.sql`
- Migrations run on server startup (embedded SQLx migrations).

---

## 9. Key dependencies (pinned)

### Rust (Cargo.toml workspace deps)
```
axum = "0.8"
sqlx = { version = "0.8", features = ["runtime-tokio", "postgres", "uuid", "chrono", "json"] }
reqwest = { version = "0.12", features = ["json"] }
git2 = "0.19"
clap = { version = "4", features = ["derive"] }
tower = "0.5"
tower-http = { version = "0.6", features = ["cors", "trace"] }
sha2 = "0.10"
hex = "0.4"
urlencoding = "2"
rand = "0.8"
```

### Web (package.json)
```
react: ^18
react-dom: ^18
react-router-dom: ^6
@tanstack/react-query: ^5
tailwindcss: ^3
typescript: ^5
vite: ^5
```
