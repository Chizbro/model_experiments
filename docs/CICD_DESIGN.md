# CI/CD Design

Platform-agnostic design for continuous integration and delivery. **CI platform** and **git host** for this project are not yet chosen; this doc describes *what* should run and *when*, so it can be implemented on any provider (e.g. GitHub Actions, GitLab CI, CircleCI, etc.) once decided.

---

## 1. Triggers

| Trigger | Use |
|--------|-----|
| **Push to default branch** | Run full CI (build, test, lint). Optional: block merge if failing. |
| **Pull / merge request** | Same as above; report status on the PR/MR. |
| **Push to any branch** | Optional: lightweight check (e.g. fmt only) or full CI. |
| **Tag (e.g. `v*`)** | Optional: build artifacts, attach to release, or publish (e.g. crates.io, npm). |

---

## 2. Jobs (design)

### 2.1 Rust (server, worker, cli, api-types)

| Step | Command / intent |
|------|-------------------|
| **Check format** | `cargo fmt --all -- --check` |
| **Lint** | `cargo clippy --all-targets --all-features -- -D warnings` |
| **Build** | `cargo build --all-targets` (or `--release` for release pipeline) |
| **Test** | `cargo test --all-targets` |

Run on a single job (or split build/test if needed for speed). Use a stable Rust toolchain (e.g. `rust-toolchain.toml` or CI image with fixed version).

**Postgres for migration tests:** The GitHub Actions Rust job starts a **PostgreSQL 16** service and sets **`DATABASE_URL`** so `crates/server/tests/migrations_integration.rs` applies embedded SQLx migrations against a real database (see plan task 05). Local `cargo test` without `DATABASE_URL` still passes; that test skips the DB assertions.

### 2.2 Web UI

| Step | Command / intent |
|------|-------------------|
| **Install deps** | `npm ci` (or `pnpm` / `yarn` if standardized) |
| **Lint** | `npm run lint` (ESLint; configure in project) |
| **Type-check** | `npm run typecheck` or `tsc --noEmit` (if applicable) |
| **Build** | `npm run build` (Vite build; ensures it compiles) |
| **Test** | `npm run test` (if tests exist; e.g. Vitest) |

Can be a separate job from Rust, or same workflow with multiple jobs.

### 2.3 Compose E2E smoke (optional / nightly)

| Step | Command / intent |
|------|-------------------|
| **Smoke** | From repo root: [`scripts/compose-smoke.sh`](../scripts/compose-smoke.sh) — `docker compose` (control plane + Postgres + worker + static web), stub agent, `file://` bare repo, identity fixture tokens, one **chat** session until the **job** is **completed** (session may stay `running`), assert logs via API. |
| **Bootstrap variant** | `RH_SMOKE_BOOTSTRAP=1` — starts server **without** `API_KEY`, calls `POST /api-keys/bootstrap`, then starts the worker with the new key. |

**PR vs nightly:** Default **push/PR** pipeline stays fast ([`ci.yml`](../.github/workflows/ci.yml)). Full Compose smoke is **heavy** (image builds); run on **`workflow_dispatch`** or a **schedule** via [`.github/workflows/e2e-compose.yml`](../.github/workflows/e2e-compose.yml).

### 2.4 Optional later (broader)

- **Broader integration tests**: e.g. long-lived staging, multi-worker pools, real agent CLIs.
- **Release**: on tag, build release binaries for targets (e.g. linux-x64, mac-x64, mac-arm), attach to release / publish CLI to a registry.
- **Docker**: build and push images for `server` and `worker` if/when containerized.

---

## 3. Conventions

- **Single pipeline** that runs both Rust and Web jobs (so one “green” covers the repo).
- **Cache**: Rust (e.g. `target/`, cargo registry); Node (`node_modules/` or equivalent). Exact keys depend on platform.
- **Secrets**: no application secrets in CI config; use the provider’s secret store for any API keys or tokens needed for publish/release.
- **Branch protection** (when host chosen): optional “require CI to pass before merge” and “require up-to-date branch.”

---

## 4. Platform (placeholder) & remaining decisions

| Decision | Status |
|---------|--------|
| **Git host** | Still TBD (e.g. GitHub, GitLab, Bitbucket, self-hosted). |
| **CI platform** | **Placeholder:** [GitHub Actions](https://docs.github.com/en/actions) workflow at [`.github/workflows/ci.yml`](../.github/workflows/ci.yml) implements §2 jobs so the default branch can stay green. Re-map to another provider if the git host changes. |

**Local parity:** run [`scripts/ci-local.sh`](../scripts/ci-local.sh) from the repo root (requires Node 22+ and `npm` on `PATH`).

**OpenAPI drift:** `cargo test -p server --test openapi_contract` parses [`crates/server/openapi.yaml`](../crates/server/openapi.yaml) and asserts the set of `operationId` values matches the allowlist in that test. Editing the spec without updating the allowlist (and implementing the route) fails CI.

### Why this matters for UX

A visible **green CI** on every change is part of **release quality**: it catches drift between [API_OVERVIEW.md](API_OVERVIEW.md) and generated OpenAPI/types, broken Web builds, and migration mistakes before users hit them. **Treat “no CI” as a release risk**, not only a developer convenience.

**OpenAPI vs allowlist:** The `openapi_contract` integration test (above) fails when `operationId`s change without updating Rust; optional follow-ups include codegen from the spec or handler-to-path pairing tests—see [API_OVERVIEW.md — Spec delivery](API_OVERVIEW.md).

---

*See also: [Project Kickoff](PROJECT_KICKOFF.md) (CI checklist).*
