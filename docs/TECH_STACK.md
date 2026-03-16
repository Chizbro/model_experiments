# Tech Stack

Recommended technologies for the agentic task manager (control plane, workers, CLI, Web UI). Alternatives are noted where relevant.

---

## 1. Control Plane (Server) — Rust

| Layer | Recommendation | Rationale | Alternatives |
|-------|----------------|-----------|--------------|
| **Language / Runtime** | **Rust** | Single binary, no runtime; same language as workers and CLI for shared types and one toolchain. Strong concurrency (tokio), great for long-lived connections (WebSocket, log streams). | Go, Python (FastAPI) |
| **API** | **REST + WebSocket** | REST for CRUD and one-off calls; WebSocket for log tail, session attach, live updates. | gRPC + streaming for more rigid RPC |
| **Registry / State DB** | **PostgreSQL** or **SQLite** (dev) | Sessions, workers, jobs, inbox metadata, config. PostgreSQL for production scale. | SQLite for single-node / dev only |
| **Task queue / dispatch** | **PostgreSQL table** | Task queue lives in a Postgres table (same DB as registry/sessions). Workers get tasks by **polling** the API (e.g. `POST /workers/tasks/pull` or long-poll). No Redis, NATS, or DB LISTEN/NOTIFY in v1. | — |
| **Log storage** | **v1: Dual-write.** (1) Local files on each component (control plane + workers), with rotation (size or time; implementation-defined). (2) Central store = **PostgreSQL** on the control plane for all logs that reach it. No "files vs DB" choice for central—it’s always DB. See [Decisions §20](DECISIONS.md#20-log-storage-rule-v1). | Structured JSON; index by session_id, job_id, timestamp. Later: ClickHouse/Loki/Elasticsearch for scale. | — |

**Rust control-plane crates (suggested):** `axum` (HTTP + WebSocket), `tokio`, `sqlx` (PostgreSQL), `serde` / `serde_json`, `tracing`, `tower` (middleware).

**Default:** **Rust** + **PostgreSQL** (task queue in DB). WebSocket for real-time.

---

## 2. Workers (Rust)

| Layer | Recommendation | Rationale | Alternatives |
|-------|----------------|-----------|--------------|
| **Language** | **Rust** | Single binary, no runtime; great for long-running workers and resource control. Communicates with control plane via a clear REST/WebSocket contract—no need to share code with the server. | Go, Python |
| **Agent runtime** | **Claude Code or Cursor CLI** (BYOL) | The platform does not call models directly. Users sign in with Claude Code or Cursor subscription; the control plane stores/refreshes that token. The worker runs the **Claude Code** or **Cursor** CLI in the cloned repo, authenticated with the user’s token—those CLIs do the agent work. See [Product: BYOL](PRODUCT.md#bring-your-own-licence-byol). v1 supports these two CLIs only. | — |
| **Git** | **`git2`** (libgit2 bindings) | Clone, checkout, commit, push from worker. Mature; good support for credential callbacks (HTTPS + token). See [Git auth](#git-auth-github--gitlab) below. | Shell-out to `git` CLI |
| **Execution isolation** | **Process per task** (or container later) | One clone + one agent run per process; clear cleanup. Optional later: Docker per job. | Containers (Docker/Podman) from day one if you need strong isolation |
| **Platform-specific CLI integration** | **Per-platform handling required** | The Claude Code and Cursor CLIs behave differently on Windows (native), WSL, macOS, and Linux—invocation, argument passing, output streaming. Workers must have **specific handling per platform** for: (1) discovering and invoking the CLI, (2) passing arguments in (quoting, stdin vs argv; Windows is especially different), (3) streaming results out (stdout/stderr, PTY vs non-PTY, Windows console). Implement one worker variant per platform (Windows native, WSL, macOS, Linux) or clear platform modules in one codebase. See [Architecture §4c](ARCHITECTURE.md), [Decisions §25](DECISIONS.md#25-platform-specific-workers-cli-invocation). | — |

**Rust worker crates (suggested):** `tokio` (async runtime), `reqwest` (HTTP to control plane), `serde` / `serde_json` (contracts), `git2`, `tracing` (structured logs), `anyhow` / `thiserror` (errors). Worker runs Claude Code or Cursor CLI (subprocess) with user’s token; no direct LLM API calls. Optional: `tokio_tungstenite` for WebSocket if workers subscribe to control plane events.

Workers need: **control plane URL** (env/config), **auth token** (API key or mTLS), **optional labels** (e.g. `gpu=true`, `env=staging`).

### Git auth (GitHub / GitLab)

Git operations target **GitHub** and **GitLab**. Auth is platform-managed; workers never perform login.

- **Sign-in:** Users sign in to GitHub/GitLab via the platform (OAuth in Web UI or CLI). The **control plane** stores and refreshes tokens (OAuth refresh or PAT).
- **Credentials per job:** The control plane either includes a job-scoped token in the task payload or provides an endpoint for the worker to request credentials for a repo. Workers use the token only for that job.
- **Worker:** Receives a token and uses it for clone/push. GitHub and GitLab support HTTPS with token-as-password; the worker uses `git2` with credentials supplied by the platform (e.g. `Cred::userpass_plaintext` or credential callback)—no reliance on the host’s global Git config.

---

## 3. CLI — Rust

| Layer | Recommendation | Rationale | Alternatives |
|-------|----------------|-----------|--------------|
| **Language** | **Rust** (`clap`) | One binary, easy to ship; shares types and API client code with control plane and workers in the same repo. | Go (Cobra), Python (Click/Typer) |
| **Config** | **Env vars + config file** (e.g. `~/.config/remote-harness/config.yaml`) | `CONTROL_PLANE_URL`, `API_KEY`; optional profile per environment. | Env only |
| **Real-time** | **WebSocket or SSE** for `logs tail` and `attach` | Same endpoint as Web UI for consistency. | Long-polling (simpler but less efficient) |

**CLI surface (conceptual):**

- `remote-harness login` / `remote-harness config set url <url>`
- `remote-harness session start [--repo URL] [--workflow type] [--params ...]`
- `remote-harness session list` / `remote-harness session show <id>`
- `remote-harness attach <session_id>`
- `remote-harness logs tail [--session-id id] [--job-id id]`
- `remote-harness workers list`
- `remote-harness inbox send <agent_id> <task_spec>`

---

## 4. Web UI

| Layer | Recommendation | Rationale | Alternatives |
|-------|----------------|-----------|--------------|
| **Stack** | **React** (or **Vite + React**) + **TypeScript** | Component model fits dashboards; TypeScript for API types. | Vue, Svelte |
| **State / data** | **TanStack Query (React Query)** + **WebSocket** for live data | REST for sessions, workers, jobs; WebSocket for log tail and session attach. | SWR, Redux + thunk |
| **UI components** | **Tailwind CSS** + **shadcn/ui** or **Radix** | Fast, accessible, good DX. | MUI, Chakra, custom |
| **Real-time** | **SSE** for log stream and session updates | Same as CLI. v1: SSE only for tail/attach. | Polling (fallback) |

**Key views:** Dashboard (sessions, workers), Session detail (log tail, attach, inputs), Worker list, Inbox / agent list, **Settings** (control plane URL configurable here in v1 — e.g. for Tailscale URL when opening the UI from anywhere).

**Client-only API access:** When the UI is hosted publicly (e.g. on an always-on host) while the control plane is only reachable via Tailscale (or a private network), the browser must talk to the control plane directly. The UI must **not** use server-side code in the framework to proxy or fetch from the control plane (no Next.js server components, Nuxt server routes, etc. that call the API). Use a **client-side only** pattern: SPA with browser-based REST and WebSocket to the control plane URL. See [Hosting §4b](HOSTING.md#4b-ui-hosting-public-url-client-only-api-access).

---

## 5. Logging (Unified)

| Layer | Recommendation | Rationale |
|-------|----------------|-----------|
| **Format** | **Structured JSON** (e.g. `{"time":"...","level":"info","session_id":"...","job_id":"...","message":"..."}`) | Easy to parse, filter, index. |
| **Transport from workers** | **HTTPS POST** to control plane log endpoint | Workers send log entries to the control plane; control plane aggregates and stores. No queue. |
| **Tail API** | **SSE** (Server-Sent Events) with filters (`session_id`, `job_id`, `level`) | Same for CLI and Web. v1: SSE only; no WebSocket for log tail. |
| **Persistence** | **Dual-write; all logs go to disk.** (1) **Local files:** Every log line is written to disk on the component that produced it (control plane → files on backend; each worker → files on that worker). (2) **Central store (PostgreSQL)** on the control plane: control plane writes its own logs and ingested worker logs to the DB so the CLI and Web UI can both show all logs from all places. If streaming or either client breaks, logs are still in files on backend or worker. See [Architecture §6](ARCHITECTURE.md#6-logging-architecture). | Retention and “retain forever” / manual delete apply to the central store. |

---

## 6. Security & Auth (control plane only)

This section is about **auth to the Remote Harness control plane** (who can use the API, which workers are trusted). Auth to Git and to the agent CLI (BYOL) are separate; see [Architecture §8](ARCHITECTURE.md#8-three-auth-concerns).

| Area | Recommendation |
|------|----------------|
| **Control plane API** | API key in header (`Authorization: Bearer <key>` or `X-API-Key`) for CLI and Web. |
| **Workers** | API key (machine token) in env or config; sent with every request to the control plane. |
| **TLS** | All connections to the control plane over HTTPS (TLS). |
| **Secrets** | Git credentials in control plane or env; workers receive per-job. Agent execution uses user’s Claude Code / Cursor token (BYOL), stored and refreshed by control plane. |

**v1: API key only.** No OIDC or mTLS in v1.

### Control plane auth: configuration (API key)

Configuration must be clear and consistent across components.

| Component | Where the API key is set | How it's used |
|-----------|---------------------------|----------------|
| **Server** | Server stores issued API keys (e.g. in DB or config). Keys are created via CLI or Web UI (e.g. `remote-harness api-key create` or Settings in UI). Server validates the key on every request. | Validates `Authorization` or `X-API-Key` header. |
| **CLI** | **Env:** `REMOTE_HARNESS_API_KEY` (or `API_KEY`). **Config file:** `~/.config/remote-harness/config.yaml` with `api_key: <key>`. **Precedence:** CLI flag > env > config file. Document in CLI help and README. See [Decisions §17](DECISIONS.md#17-config-precedence-cli-and-worker). | Sent on every request to the control plane URL. |
| **Web UI** | User enters the API key once (e.g. Login or Set API key in UI). Stored in browser **localStorage** (control plane URL and API key); logout clears them. See [Decisions §18](DECISIONS.md#18-web-ui-storage-for-url-and-api-key). | Sent in `Authorization: Bearer <key>` or `X-API-Key` on every API call. |
| **Worker** | **Env:** `REMOTE_HARNESS_API_KEY` or `API_KEY`. **Config file:** e.g. `config.yaml` or `~/.config/remote-harness-worker/config.yaml` with `control_plane_url` and `api_key`. **Precedence:** env > config file (no flags for worker). Document in worker README. See [Decisions §17](DECISIONS.md#17-config-precedence-cli-and-worker). | Sent on every request (register, heartbeat, pull task, send logs). |

**Control plane URL** (for CLI and worker): env `REMOTE_HARNESS_URL` or `CONTROL_PLANE_URL`, or config file `control_plane_url`. Same precedence as api_key (CLI: flag > env > config; worker: env > config). Document alongside `api_key` so both are in one place.

---

## 7. Repo Layout (Rust Monorepo)

Control plane, workers, and CLI are all **Rust** in one workspace; shared types and API contracts live in a common crate.

```
remote_harness/
├── crates/
│   ├── server/              # Control plane (axum, API, engine, sessions, logs)
│   ├── worker/              # Worker (reqwest, git2, runs Claude Code / Cursor CLI with user token)
│   ├── cli/                 # CLI (clap, API client, log tail)
│   └── api-types/           # Shared: request/response types, IDs, contracts (serde)
├── web/                     # Web UI (Vite + React)
│   ├── src/
│   └── package.json
├── docs/
│   ├── ARCHITECTURE.md
│   ├── TECH_STACK.md
│   └── PRODUCT.md
├── Cargo.toml               # Workspace root
├── Cargo.lock
├── Makefile
└── README.md
```

Each binary is built with `cargo build -p server`, `cargo build -p worker`, `cargo build -p cli`. The `api-types` crate keeps REST/WebSocket payloads in sync between server, worker, and CLI.

---

## 8. Summary Table

| Component | Stack |
|-----------|-----------------|
| Control plane | **Rust** (axum, tokio, sqlx), REST + WebSocket, PostgreSQL (task queue in DB) |
| Workers | **Rust** (tokio, reqwest, git2); agent via **Claude Code or Cursor CLI** (BYOL—user’s token) |
| CLI | **Rust** (clap), SSE for tail and attach, shares `api-types` with server |
| Web UI | React + TypeScript, Vite, TanStack Query, SSE for log/session stream |
| Logging | JSON; dual-write (local files + central DB); SSE for tail; retention and manual delete on central store |
| Auth | API key (CLI, Web, workers); TLS for all connections |

---

*Previous: [Architecture](ARCHITECTURE.md) | Next: [Product & Features](PRODUCT.md)*
