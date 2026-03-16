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

### 2.2 Web UI

| Step | Command / intent |
|------|-------------------|
| **Install deps** | `npm ci` (or `pnpm` / `yarn` if standardized) |
| **Lint** | `npm run lint` (ESLint; configure in project) |
| **Type-check** | `npm run typecheck` or `tsc --noEmit` (if applicable) |
| **Build** | `npm run build` (Vite build; ensures it compiles) |
| **Test** | `npm run test` (if tests exist; e.g. Vitest) |

Can be a separate job from Rust, or same workflow with multiple jobs.

### 2.3 Optional later

- **Integration tests**: e.g. start server + worker, run a single task, assert outcome (DB, test container, or mock).
- **Release**: on tag, build release binaries for targets (e.g. linux-x64, mac-x64, mac-arm), attach to release / publish CLI to a registry.
- **Docker**: build and push images for `server` and `worker` if/when containerized.

---

## 3. Conventions

- **Single pipeline** that runs both Rust and Web jobs (so one “green” covers the repo).
- **Cache**: Rust (e.g. `target/`, cargo registry); Node (`node_modules/` or equivalent). Exact keys depend on platform.
- **Secrets**: no application secrets in CI config; use the provider’s secret store for any API keys or tokens needed for publish/release.
- **Branch protection** (when host chosen): optional “require CI to pass before merge” and “require up-to-date branch.”

---

## 4. Not yet decided

| Decision | Status |
|---------|--------|
| **Git host** | TBD (e.g. GitHub, GitLab, Bitbucket, self-hosted). |
| **CI platform** | TBD (e.g. GitHub Actions, GitLab CI, CircleCI, Jenkins). |

Once chosen, add a short “Platform” section to this doc (e.g. “We use GitHub and GitHub Actions; see `.github/workflows/`”) and implement the jobs above in that platform’s format. The design (triggers, jobs, steps) remains as above.

---

*See also: [Project Kickoff](PROJECT_KICKOFF.md) (CI checklist), [Decisions](DECISIONS.md) (CI platform & git host recorded when decided).*
