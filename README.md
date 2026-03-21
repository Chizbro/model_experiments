# Remote Harness

Self-hosted **control plane** (Rust), **workers** that run agent CLIs in Git clones, a **CLI**, and a **Web UI** (Vite + React under `web/`). One **API key** ties server, workers, and clients together; workers register automatically. See [PRODUCT.md](docs/PRODUCT.md) for vision and [PROJECT_KICKOFF.md](docs/PROJECT_KICKOFF.md) for phases.

## Documentation

Specs and runbooks: [`docs/README.md`](docs/README.md). **Local setup:** [GETTING_STARTED.md](docs/GETTING_STARTED.md) — **§1** Docker Compose (recommended), **§2** bare metal (four terminals), **§3** BYOL credentials / OAuth.

## Run with Docker Compose

From the repo root:

```bash
docker compose up --build
```

Services: **Postgres** → **server** on [http://localhost:3000](http://localhost:3000) (`GET /health`), **worker** (points at `http://server:3000`; image bundles **Cursor `agent`**, build-arg **`CURSOR_AGENT_VERSION`**), **web** static UI on [http://localhost:5173](http://localhost:5173). Shared key: `API_KEY` / `.env` (default `dev-key-change-in-production`). In the UI: **Settings** — control plane URL + same API key.

**Automated green path (chat session + logs):**

```bash
./scripts/compose-smoke.sh
```

Stub agent + `file://` fixture repo; see [GETTING_STARTED §1.9](docs/GETTING_STARTED.md#19-automated-compose-smoke-tier-1--tier-2). Optional **`RH_SMOKE_BOOTSTRAP=1`** exercises `POST /api-keys/bootstrap`.

## Run on the host (without full Compose)

### Control plane

```bash
export DATABASE_URL="postgres://…"   # see GETTING_STARTED §2
export API_KEY="dev-key-change-in-production"
cargo run -p server
```

GitLab/GitHub OAuth variables can live in a repo-root **`.env`** file; the server loads that file automatically (shells do not). With **Docker Compose**, OAuth keys must also be listed under `server.environment` in `docker-compose.yml` so they reach the container — copy from `.env.example` and recreate the stack.

### Worker

```bash
export CONTROL_PLANE_URL="http://127.0.0.1:3000"
export API_KEY="dev-key-change-in-production"   # same as server
cargo run -p worker
```

Details: [crates/worker/README.md](crates/worker/README.md).

### CLI

```bash
export REMOTE_HARNESS_URL="http://127.0.0.1:3000"
export REMOTE_HARNESS_API_KEY="dev-key-change-in-production"
cargo build -p cli
cargo run -p cli -- health
cargo run -p cli -- session list
```

Config file option: `~/.config/remote-harness/config.yaml` (`cargo run -p cli -- config show`).

### Web UI (development server)

```bash
cd web && npm install && npm run dev
```

Production-like static build is served by Compose (`web/Dockerfile`). Dev guide: [web/README.md](web/README.md).

## Rust workspace

```bash
cargo build --workspace
```

Crates: `crates/server`, `crates/worker`, `crates/cli`, `crates/api-types`.

## CI

Local parity with GitHub Actions:

```bash
./scripts/ci-local.sh
```

Workflows: [`.github/workflows/ci.yml`](.github/workflows/ci.yml) (push/PR), [`.github/workflows/e2e-compose.yml`](.github/workflows/e2e-compose.yml) (`workflow_dispatch` + nightly Compose smoke).

## Git OAuth (GitHub / GitLab)

To enable **Sign in with GitHub** or **Sign in with GitLab** for identity `git_token` storage ([`docs/API_OVERVIEW.md`](docs/API_OVERVIEW.md) §4b), set on the **control plane**:

| Variable | Purpose |
|----------|---------|
| `REDIRECT_AFTER_AUTH` | Browser redirect after callback (e.g. Web UI `http://localhost:5173/settings`). **Required** for OAuth routes. |
| `GITHUB_CLIENT_ID`, `GITHUB_CLIENT_SECRET`, `GITHUB_REDIRECT_URI` | GitHub OAuth app (callback path must match `/auth/github/callback` on the server). |
| `GITLAB_CLIENT_ID`, `GITLAB_CLIENT_SECRET`, `GITLAB_REDIRECT_URI` | GitLab application (callback `/auth/gitlab/callback`). |
| `GITLAB_BASE_URL` | Optional; default `https://gitlab.com`. Set for self-hosted GitLab. |
| `OAUTH_COOKIE_SECURE` | Optional; set `true` in production HTTPS so `_rh_oauth` gets the `Secure` flag. |
| `WEB_UI_BASE_URL` | Optional; no trailing slash. When set, `POST /sessions` returns `web_url` like `{base}/sessions/{id}` for deep links from CLI/automation. |
| `LOOP_UNTIL_SENTINEL_MAX_ITERATIONS` | Optional; default **500**, minimum **1**. Caps how many jobs the server enqueues for `loop_until_sentinel` when the worker never reports `sentinel_reached`. |

**Test / advanced:** `GITHUB_OAUTH_ACCESS_TOKEN_URL` and `GITHUB_OAUTH_AUTHORIZE_URL` override defaults (used by integration tests with a mock token server). Same pattern for GitLab with `GITLAB_OAUTH_*`.

CLI: `cargo run -p cli -- oauth github` / `oauth gitlab` prints the URL to open. More detail: [`docs/HOSTING.md`](docs/HOSTING.md), [`docs/GETTING_STARTED_WITH_DOCKER.md`](docs/GETTING_STARTED_WITH_DOCKER.md).
