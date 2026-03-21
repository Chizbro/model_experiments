# Implementation boundaries & test strategy

Companion to [TECH_STACK §7](TECH_STACK.md#7-repo-layout-rust-monorepo) and [ARCHITECTURE](ARCHITECTURE.md). **Goal:** keep dependencies and contracts one-directional so the workspace can grow without ad-hoc coupling.

---

## 1. Crate graph

**Workspace binaries and library:**

| Crate | Role | May depend on |
|-------|------|----------------|
| **`api-types`** | Shared serde types, IDs, request/response shapes for REST JSON | `serde`, small std-only helpers only—**no** `axum`, `sqlx`, `reqwest` |
| **`server`** | Control plane: HTTP (REST + SSE), DB, workflows, log aggregation | `api-types`, DB stack (`sqlx`, etc.), `axum`, engine/store modules **inside** this crate |
| **`worker`** | Poll tasks, Git, agent CLI subprocess (`src/agent_cli/` — platform spawn + argv/env + streaming) | `api-types`, HTTP client (`reqwest`), `git2`—**no** server-only crates |
| **`cli`** | User commands, API client, SSE consumers | `api-types`, HTTP + SSE client stack—**no** direct `sqlx` / server internals |

**Dependency rule:** `api-types` is a **DAG leaf** (nothing in `crates/` should depend on `server`). `worker` and `cli` talk to the control plane **only over HTTP** (and shared types from `api-types`), not by linking server code.

**Optional internal layout inside `crates/server`** (single package, modules—not separate crates unless a future task splits them):

- **`engine`** — task/workflow state machine, queue claims, leases
- **`api`** — Axum routers, extractors, error mapping to [standard error JSON](API_OVERVIEW.md#2-standard-error-response)
- **`db`** — queries, migrations path (`crates/server/migrations` per [ARCHITECTURE §2a](ARCHITECTURE.md#2a-schema-migrations))

Keep boundaries as **modules** with `pub(crate)` where possible so handlers do not reach into raw SQL from every file.

---

## 2. Single source of truth for HTTP contracts

- **Canonical artifact:** **OpenAPI 3.x** at [`crates/server/openapi.yaml`](../crates/server/openapi.yaml) ([API_OVERVIEW — Spec delivery](API_OVERVIEW.md#spec-delivery-implementation-requirement)). **This file + [API_OVERVIEW.md](API_OVERVIEW.md) must agree**; CI fails on `operationId` drift via `cargo test -p server --test openapi_contract` ([CICD_DESIGN §4](CICD_DESIGN.md#4-platform-placeholder--remaining-decisions)).
- **Rust types:** Prefer **`api-types`** as the shared serde mirror for payloads used by server, worker, and CLI. If a type is server-only (internal admin), it may live in `server` only—do not duplicate “public” REST shapes in three places.
- **Alternatives (avoid unless justified):** Hand-written duplicates in each crate **require** a pairing test or codegen step so JSON round-trips match OpenAPI examples.

**SSE:** Documented in **[SSE_EVENTS.md](SSE_EVENTS.md)** (companion to OpenAPI); stated in the `openapi.yaml` header and [PROJECT_KICKOFF §6](PROJECT_KICKOFF.md#6-communication--docs).

---

## 3. Configuration (env + file precedence)

Align with [TECH_STACK §3](TECH_STACK.md#3-cli--rust), **Control plane auth** table in [TECH_STACK §6](TECH_STACK.md#6-security--auth-control-plane-only), and implementation notes there.

| Component | Control plane URL | API key / auth | Notes |
|-----------|-------------------|----------------|--------|
| **Server** | `PORT`, `DATABASE_URL`, bind address | `API_KEY` / `API_KEYS`, issued keys in DB, optional config file | See server task specs for full env surface |
| **CLI** | `REMOTE_HARNESS_URL` or `CONTROL_PLANE_URL`; config `control_plane_url` | `REMOTE_HARNESS_API_KEY` / `API_KEY`; config `api_key` | **Precedence:** CLI flag > env > `~/.config/remote-harness/config.yaml` |
| **Worker** | `CONTROL_PLANE_URL` or `REMOTE_HARNESS_URL` | `API_KEY` or `REMOTE_HARNESS_API_KEY` | **v1:** env only (`crates/worker/README.md`). Optional YAML file may be added later; no CLI flags for worker v1 |

**Rule:** Document new env vars in **one** place operators read ([GETTING_STARTED](GETTING_STARTED.md), [HOSTING](HOSTING.md), or TECH_STACK)—and add them to the server/worker/CLI config parsers in the same change.

---

## 4. SSE vs REST (avoid ad-hoc channels)

- **REST:** CRUD, commands, polling endpoints (`POST /workers/tasks/pull`, session create, etc.).
- **SSE:** Long-lived **streams** only—e.g. log tail, session attach/events ([API_OVERVIEW](API_OVERVIEW.md)).

**Implementation pattern (server):** Use a **single broadcast / subscriber abstraction** per stream family (e.g. “log events keyed by session/job”, “session lifecycle events keyed by session id”). Handlers subscribe clients to those buses; **do not** spawn unbounded per-handler `tokio::sync::broadcast` channels without a shared registry, or duplicate fan-out logic in each route.

**Clients (CLI + Web):** Same URLs and query params as documented; reconnect with backoff ([CLIENT_EXPERIENCE](CLIENT_EXPERIENCE.md)).

---

## 5. Test pyramid & named commands

Use the same commands in local dev and CI ([CICD_DESIGN §2](CICD_DESIGN.md#2-jobs-design)).

| Layer | Scope | Command(s) | Notes |
|-------|--------|------------|--------|
| **Unit** | Pure logic (parsers, state transitions, cap math) | `cargo test -p <crate> --lib` | No network; fast |
| **Integration (Rust)** | DB + HTTP with real Postgres | `cargo test --all-targets` with `DATABASE_URL` pointing at a disposable DB | Prefer a dedicated test database; optional **testcontainers** (or Compose service) if the repo adopts it—keep one documented pattern |
| **Web** | Components, hooks, API client | `npm ci` then `npm run lint`, `npm run typecheck` / `tsc --noEmit`, `npm run build`, `npm run test` (Vitest) | Exact scripts follow `web/package.json` when present |
| **E2E (optional)** | Full stack smoke | Documented in plan task **26** (e.g. Compose smoke); **Playwright** only if added by spec—not required for v1 core loop |

**Formatting / lint (Rust):** `cargo fmt --all -- --check` and `cargo clippy --all-targets --all-features -- -D warnings`.

**Contract drift:** Implemented per [CICD_DESIGN §4](CICD_DESIGN.md#4-platform-placeholder--remaining-decisions) (`openapi_contract` test + GitHub Actions workflow).

---

## Related docs

- [ARCHITECTURE §2a — Migrations](ARCHITECTURE.md#2a-schema-migrations)
- [TECH_STACK §7 — Repo layout](TECH_STACK.md#7-repo-layout-rust-monorepo)
- [API_OVERVIEW — Spec delivery](API_OVERVIEW.md#spec-delivery-implementation-requirement)

*Previous: [Docs index](README.md) | Next: follow [plan/README.md](../plan/README.md) build order*
