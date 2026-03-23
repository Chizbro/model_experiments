# Getting Started (local development)

Run the full stack locally: PostgreSQL, control plane, worker, Web UI, and the CLI on your machine. **macOS- or Docker-specific notes** (e.g. Docker Desktop, mounting the agent binary) are called out inline—Linux and Windows follow the same patterns unless noted.

**Two ways:**

1. **Docker Compose (recommended)** — One `docker compose up`; crash restart on laptops; server, worker, DB, and web UI in containers; you only run the **CLI** on the host. See [§1](#1-docker-compose-recommended).
2. **Bare metal** — `cargo run` for server and worker, local Postgres (or container DB only), **npm run dev** for the Web UI. See [§2](#2-bare-metal-four-terminals).

Deep topology and sleep behavior: [HOSTING.md](HOSTING.md).

---

## Prerequisites

**Either path:**

- **Rust** (e.g. `rustup`) — required for the **CLI** from your machine; also required for **bare-metal** server and worker.

**Docker path additionally:**

- **Docker** and **Docker Compose** (e.g. Docker Desktop on macOS/Windows, or Docker Engine + Compose plugin on Linux)

**Bare-metal path additionally:**

- **Node.js** (for the Web UI dev server)
- **PostgreSQL** — Docker one-off container (below) or local install (e.g. `brew install postgresql@16` then `brew services start postgresql@16`)

**SQLite:** Supported for minimal setups per [TECH_STACK.md §1](TECH_STACK.md) and [HOSTING.md §3](HOSTING.md). This guide uses **PostgreSQL**. If you use SQLite, set **`DATABASE_URL`** per server docs.

---

## URLs and consistency

From your **host** machine, the API is usually:

| Setup | Control plane URL | Typical UI URL |
|-------|-------------------|----------------|
| **Docker Compose** (published ports) | `http://localhost:3000` | `http://localhost:5173` (web container → host 5173) |
| **Bare metal** (this guide) | `http://127.0.0.1:3000` | `http://localhost:5173` (Vite) |

Use **one host form** (`localhost` vs `127.0.0.1`) for the API consistently with **OAuth callback URLs**, **cookie scope**, and **CORS**. The examples below use the table above; adjust if you standardize on one form everywhere.

---

## 1. Docker Compose (recommended)

### 1.1 Configuration

One shared **API key** for server, worker, and CLI/UI. Default: `dev-key-change-in-production`. Override:

```bash
export API_KEY="your-secret-key"
```

Run `docker compose` in the same shell, or put `API_KEY=...` in a **`.env`** file in the repo root (see [`.env.example`](../.env.example)).

### 1.2 Start everything

From the repo root:

```bash
docker compose up --build
```

First run builds Rust and the Web image; later runs reuse images. If the worker failed with **"Name or service not known"** for host `server`, rebuild: `docker compose build --no-cache server worker && docker compose up -d`.

You should see: **postgres** healthy → **server** on 3000 → **worker** registered → **web** on host **5173** (container port 80 mapped to 5173).

Stop: `Ctrl+C`. Background: `docker compose up -d --build`.

**Migrations / `VersionMismatch` after `git pull`:** Reset the DB volume and rebuild:

```bash
docker compose down -v
docker compose up -d --build
```

See [TROUBLESHOOTING.md §1b](TROUBLESHOOTING.md#1b-versionmismatch--sqlx-migration-checksum-errors).

**SQLx offline / `cargo sqlx prepare`:** Migrations use `sqlx::migrate!`, which **embeds** the files under `crates/server/migrations` at **compile time**—no live database is required to build. The `cargo sqlx prepare` flow and `.sqlx/` metadata are for **compile-time checked queries** (`query!`, etc.), not for the migration macro; see [TROUBLESHOOTING.md §1b](TROUBLESHOOTING.md#1b-versionmismatch--sqlx-migration-checksum-errors) if you add those macros later.

**Sleep-inhibit:** From the **host**, poll `GET http://localhost:3000/health/idle` (or your published port). **200** = OK to sleep; **503** = busy. See [HOSTING.md §4a](HOSTING.md#4a-idle-sleep--sleep-inhibit).

#### Worker and backend lifecycle

- **Restart:** Services use **`restart: unless-stopped`** — auto-restart after panic/OOM while the host is awake.
- **Sleep/wake:** Host sleep suspends the Docker VM; on wake, containers resume and the worker re-heartbeats. See [HOSTING.md §9](HOSTING.md#9-docker-and-sleep-detailed-behavior).
- **Scale workers:** `docker compose up --scale worker=N` (e.g. `N=2`).
- **Stuck jobs:** Stale workers → reclaim; see [TROUBLESHOOTING.md](TROUBLESHOOTING.md).

### 1.3 Use the UI (Docker)

1. Open **http://localhost:5173**
2. **Settings:** Control plane URL **http://localhost:3000**, API key **matching `API_KEY`**

### 1.4 Agent CLI in the worker container

The **default** `Dockerfile.worker` image installs **Cursor’s `agent`** on `PATH` (`/usr/local/bin/agent` → `/opt/cursor-agent/cursor-agent`) by downloading the official Linux package at image build time. **`CURSOR_AGENT_VERSION`** (build arg / env for Compose) pins the package version; if the download 404s, bump it to match the URL in `curl -fsSL https://cursor.com/install | grep DOWNLOAD_URL`.

- **Claude Code** is **not** bundled: install `claude` in a derived image, mount a **Linux** binary, or set `CLAUDE_CLI_PATH` / `REMOTE_HARNESS_CLAUDE_BIN`.
- **Override Cursor:** set `CURSOR_AGENT_PATH` or `REMOTE_HARNESS_CURSOR_AGENT_BIN`, or mount a different Linux `agent`.
- **macOS host:** Do not bind-mount the Mac `agent` into a Linux worker — the image already ships a Linux build, or run the worker **natively** on the Mac.
- **Smoke / CI:** use `docker-compose.smoke.yml` (or `REMOTE_HARNESS_STUB_AGENT=1`) to skip the real CLI; see [§1.9](#19-automated-compose-smoke-tier-1--tier-2).

### 1.5 CLI from your laptop (Docker)

```bash
export REMOTE_HARNESS_URL="http://localhost:3000"
export REMOTE_HARNESS_API_KEY="dev-key-change-in-production"
cargo run -p cli -- health
cargo run -p cli -- ready
# API keys (see API_OVERVIEW §4c): create/list/revoke need a key; bootstrap is unauthenticated only when no keys exist and the server has no API_KEY/API_KEYS env keys.
cargo run -p cli -- api-key list
cargo run -p cli -- api-key create --label ci
# cargo run -p cli -- api-key bootstrap   # first-run only; prints operator warning
# BYOL identities (API_OVERVIEW §4a): credentials status, auth health, repo picker, token PATCH
cargo run -p cli -- identity get default
cargo run -p cli -- identity auth-status default
cargo run -p cli -- identity repos default --provider github
# RH_AGENT_TOKEN=… RH_GIT_TOKEN=… cargo run -p cli -- identity patch default
```

Or `~/.config/remote-harness/config.yaml` with `control_plane_url` and `api_key`. Run `cargo run -p cli -- config show` to see resolved values and which source won (CLI → env → file).

**End-to-end CLI smoke (local):** with the control plane up, run `./scripts/e2e_cli.sh` from the repo root. It exercises the shipped subcommands in order, uses `api-key bootstrap` when no working key is in the environment (only if the server allows bootstrap), and tears down the session and worker it creates. Override session fields with `E2E_REPO_URL`, `E2E_AGENT_CLI` (`cursor` or `claude_code`), `E2E_PROMPT`, and `E2E_IDENTITY_ID` if needed.

The Web UI **Identities (BYOL)** section calls the same endpoints once you paste an API key.

Sessions and logs use the same API from CLI and Web (`session …`, **Logs** section in the dev UI). The server exposes SSE log tail and session events (`docs/SSE_EVENTS.md`); CLI/Web streaming UX is covered in later plan tasks.

### 1.6 Logs (Docker)

**UI:** Session → log panel (history then stream). **CLI:** `logs list`, `logs delete`, `logs send` (worker batch). Logs are stored in Postgres on the server; scheduled purge honors `retain_forever` on sessions and jobs (`LOG_RETENTION_DAYS_DEFAULT`, default **7** days; `LOG_PURGE_INTERVAL_SECS`).

### 1.7 What this setup uses

| Piece | Purpose |
|-------|--------|
| **docker-compose.yml** | `postgres`, `server`, `worker`, `web`; Postgres healthcheck before server; see [§1.9](#19-automated-compose-smoke-tier-1--tier-2) for smoke overlay. |
| **Dockerfile** (repo root) | Builds **server** binary into the control-plane image. |
| **Dockerfile.worker** | Builds **worker**, **`git`**, and **Cursor `agent`** (pinned `CURSOR_AGENT_VERSION`); **no** Claude Code — see [§1.4](#14-agent-cli-in-the-worker-container). |
| **web/Dockerfile** | Vite production build + nginx; SPA `try_files` → `index.html`. |
| **CORS** | UI origin `http://localhost:5173` (and often `127.0.0.1:5173`) must be allowed; see `CORS_ALLOWED_ORIGINS` and [TROUBLESHOOTING.md §1a](TROUBLESHOOTING.md#1a-cors-errors-in-the-browser). |

### 1.8 Optional: API key without exporting env

```bash
API_KEY=my-secret docker compose up --build
```

Use `my-secret` in UI Settings and `REMOTE_HARNESS_API_KEY`.

### 1.9 Automated Compose smoke (tier 1 / tier 2)

**Tier 1 (default, reproducible, CI-style):** From the repo root, run:

```bash
./scripts/compose-smoke.sh
```

This brings up **Postgres, server, worker, and the static web image** (see root `docker-compose.yml` + `docker-compose.smoke.yml`), enables the worker’s **`REMOTE_HARNESS_STUB_AGENT`** stub, bind-mounts a throwaway **bare Git repo** at `file:///e2e/repo.git`, patches the **default identity** with fixture tokens, creates one **chat** session, waits until the **sole job is `completed`** (the session may stay **`running`**—that is normal for chat so follow-up input is allowed), and checks that **at least one log line** is returned from `GET /sessions/:id/logs`. The stack is torn down afterward unless you set **`RH_SMOKE_KEEP_STACK=1`**.

**Bootstrap path (first-run simulation):** `RH_SMOKE_BOOTSTRAP=1 ./scripts/compose-smoke.sh` — server starts with **no** `API_KEY` env so `POST /api-keys/bootstrap` succeeds, then the worker starts with the issued key.

**Tier 2 (real agent):** Run Compose **without** `docker-compose.smoke.yml`, set real **BYOL** credentials ([§3](#3-credentials-and-oauth-byol)), and use an **`https://` or `http://` remote** the worker can clone. Do **not** set `REMOTE_HARNESS_STUB_AGENT` on the worker.

**Assumptions (green path time):** Roughly **10–25 minutes** first time (Rust + web image builds), **2–5 minutes** when images are warm; requires **Docker**, **git**, **curl**, and **python3** on the host.

**Rust image cache:** The smoke script sets **`RH_DOCKER_SRC_TS`** (timestamp) so Compose rebuilds the `server`/`worker` binaries instead of reusing a stale cached `RUN cargo build` layer. For manual Compose runs, export a new value when you change Rust code, or use `RH_DOCKER_SRC_TS=0` only when you intentionally want layer reuse.

---

## 2. Bare metal (four terminals)

**Crash restart:** Bare-metal `cargo run` does **not** auto-restart. For laptops, prefer [§1](#1-docker-compose-recommended) or [HOSTING.md](HOSTING.md).

### 2.1 Database

**Option A — Postgres in Docker (DB only)**

```bash
docker run -d --name remote-harness-db \
  -e POSTGRES_USER=postgres \
  -e POSTGRES_PASSWORD=postgres \
  -e POSTGRES_DB=remote_harness \
  -p 5432:5432 \
  postgres:16
```

```bash
export DATABASE_URL="postgres://postgres:postgres@127.0.0.1:5432/remote_harness"
```

**Option B — Local PostgreSQL**

```bash
createdb remote_harness
export DATABASE_URL="postgres://localhost/remote_harness"
```

(Adjust user/password if needed.)

### 2.2 Environment

```bash
export DATABASE_URL="postgres://postgres:postgres@127.0.0.1:5432/remote_harness"   # or postgres://localhost/remote_harness
export API_KEY="dev-key-change-in-production"
export CONTROL_PLANE_URL="http://127.0.0.1:3000"
export REMOTE_HARNESS_API_KEY="dev-key-change-in-production"
```

Optional server/worker tuning:

| Variable | Component | Default |
|----------|-----------|--------|
| `BIND_HOST` or `HOST` | server | `0.0.0.0` |
| `PORT` | server | `3000` |
| `DATABASE_URL` | server | unset (no DB / readiness skips ping); use `?sslmode=disable` with local Postgres when using TLS-disabled listeners |
| `CORS_ALLOWED_ORIGINS` | server | comma-separated UI origins; defaults always merge **localhost / 127.0.0.1 / `[::1]`** for Vite **5173** and preview **4173**. **`vite --host` (LAN IP)** on ports **5173 / 4173 / 5174** is allowed without listing each IP. Production hostnames still belong here. |
| `CORS_ORIGINS` | server | optional legacy alias; merged with `CORS_ALLOWED_ORIGINS` if both set |
| `LOG_RETENTION_DAYS_DEFAULT` | server | `7` |
| `LOG_PURGE_INTERVAL_SECS` | server | `3600` |
| `LOG_RETENTION_MAX_BYTES_PER_SESSION_DEFAULT` | server | `52428800` |
| `WORKER_STALE_THRESHOLD_SECS` or `WORKER_STALE_SECONDS` | server | `120` |
| `MAX_JOB_RECLAIMS` | server | `3` — after this many reclaims (stale worker, `DELETE /workers/:id`, or `POST /workers/tasks/pull`), assigned jobs are **failed** with `[MAX_WORKER_LOSS_RETRIES]` |
| `JOB_LEASE_SECONDS` | server | `0` (disabled); e.g. `21600` = 6h lease / `[JOB_LEASE_EXPIRED]` |
| `CHAT_HISTORY_MAX_TURNS` | server | `50` — max prior user turns in `task_input.history` and max prior assistant turns in `task_input.history_assistant` on **chat follow-up** pull payloads; `0` disables capping (not recommended for production). When older turns are dropped, pull sets `history_truncated`: `true` ([API_OVERVIEW — Pull task](API_OVERVIEW.md#pull-task)). |
| `LOOP_UNTIL_SENTINEL_MAX_ITERATIONS` | server | `500` (minimum effective `1`) — max jobs enqueued for **`loop_until_sentinel`** when the worker never sets `sentinel_reached` on complete ([README § Git OAuth env table](../README.md) / [API_OVERVIEW — Create session](API_OVERVIEW.md#4-rest--sessions)). |
| `HEARTBEAT_INTERVAL_SECS` | worker | `30` |

Worker config file: `~/.config/remote-harness-worker/config.yaml` or `REMOTE_HARNESS_CONFIG` (YAML; env overrides file). See [Worker README](../crates/worker/README.md).

### 2.3 Commands

**Terminal 1 — Backend**

```bash
cd /path/to/remote_harness_3
export DATABASE_URL="postgres://postgres:postgres@127.0.0.1:5432/remote_harness"
export API_KEY="dev-key-change-in-production"
cargo run -p server
```

Wait for `listening` on `0.0.0.0:3000`. Migrations run on startup.

**`.env` at the repo root:** The server binary loads that file automatically (and a `.env` in the current directory) so GitLab/GitHub OAuth variables do not need manual `export` when they live in **`remote_harness_3/.env`**. Restart the server after changes. If you still see **`oauth_not_configured`**, see [TROUBLESHOOTING.md §1d](TROUBLESHOOTING.md#1d-oauth-redirect-loop-oauth_not_configured-or-sign-in-does-nothing).

**Terminal 2 — Worker**

```bash
cd /path/to/remote_harness_3
export CONTROL_PLANE_URL="http://127.0.0.1:3000"
export REMOTE_HARNESS_API_KEY="dev-key-change-in-production"
cargo run -p worker
```

**Terminal 3 — Web UI**

```bash
cd /path/to/remote_harness_3/web
npm install
npm run dev
```

Open the URL Vite prints. The home page polls **`/health`**, **`/ready`**, and **`/health/idle`** against the base URL (default `http://127.0.0.1:3000`, or **`VITE_CONTROL_PLANE_URL`**). **Settings (later):** control plane URL, API key **dev-key-change-in-production**.

**Terminal 4 — CLI (optional)**

```bash
cd /path/to/remote_harness_3
export REMOTE_HARNESS_URL="http://127.0.0.1:3000"
export REMOTE_HARNESS_API_KEY="dev-key-change-in-production"
cargo run -p cli -- health
cargo run -p cli -- ready
cargo run -p cli -- idle
```

Or `~/.config/remote-harness/config.yaml`.

### 2.4 Quick check

- `curl -s -H "Authorization: Bearer dev-key-change-in-production" http://127.0.0.1:3000/health` → `{"status":"ok"}`
- `GET /workers` with the same header → includes your worker
- UI: Settings OK, then create a session

### 2.5 One-liner env (bare metal)

```bash
export DATABASE_URL="postgres://postgres:postgres@127.0.0.1:5432/remote_harness" API_KEY="dev-key-change-in-production" CONTROL_PLANE_URL="http://127.0.0.1:3000" REMOTE_HARNESS_API_KEY="dev-key-change-in-production" REMOTE_HARNESS_URL="http://127.0.0.1:3000"
```

---

## 3. Credentials and OAuth (BYOL)

Applies to **both** Docker and bare metal. The worker needs **agent_token** (e.g. Cursor key) and **git_token** (e.g. GitHub PAT) before sessions run; otherwise session creation is rejected.

Use the **control plane URL** from [URLs and consistency](#urls-and-consistency) in the examples below (`localhost:3000` vs `127.0.0.1:3000`).

### 3.1 UI Settings (recommended)

**Settings → Identity & credentials (BYOL)** (`/settings#byol-credentials`): save **Agent CLI token** (Cursor / Claude Code), then **Sign in with GitHub/GitLab** or paste a **Git PAT**. Use **Refresh** to confirm both flags show Git **yes** and Agent **yes** before starting a session ([§3.5](#35-gitlab-oauth-sign-in-with-gitlab), [§3.6](#36-github-oauth-sign-in-with-github)).

### 3.2 PATCH default identity (curl)

```bash
curl -s -X PATCH \
  -H "Authorization: Bearer dev-key-change-in-production" \
  -H "Content-Type: application/json" \
  -d '{"agent_token":"YOUR_CURSOR_API_KEY","git_token":"YOUR_GITHUB_PAT"}' \
  http://localhost:3000/identities/default
```

(Bare metal: use `http://127.0.0.1:3000/identities/default` if that is your API URL.)

Use a Git PAT with **`repo`** (or equivalent). With GitHub/GitLab OAuth, the UI can offer a **repo picker**; you can also paste a clone URL. The CLI accepts **`--repo owner/repo`** (via list-repositories) or **`--repo-url`** with a clone URL.

### 3.3 CLI — `credentials set`

```bash
cargo run -p cli -- credentials set default
```

Use identity id `default` (or your BYOL id). Flags or env `RH_AGENT_TOKEN` / `RH_GIT_TOKEN`, or:

```bash
cargo run -p cli -- credentials set default --git-token "ghp_xxx" --agent-token "sk-ant-…"
export RH_GIT_TOKEN="ghp_xxx" RH_AGENT_TOKEN="…"
cargo run -p cli -- credentials set default
```

Set `REMOTE_HARNESS_URL` and `REMOTE_HARNESS_API_KEY` to your control plane first.

### 3.4 Per-session tokens (optional)

On **`POST /sessions`**, put **`agent_token`**, **`git_token`**, and workflow fields (e.g. **`prompt`**, **`agent_cli`**: `"cursor"` or `"claude_code"`) in **params**. Params merge with the identity; supplied tokens override for that session only without updating the stored identity.

### 3.5 GitLab OAuth (Sign in with GitLab)

Configure the **server** (e.g. repo root **`.env`** for Compose, or env for bare metal):

- **GITLAB_CLIENT_ID** — [GitLab Application](https://gitlab.com/-/user_settings/applications) (or self‑hosted GitLab).
- **GITLAB_CLIENT_SECRET**
- **GITLAB_REDIRECT_URI** — callback on the **API** host, e.g. `http://localhost:3000/auth/gitlab/callback` (not the UI port).
- **REDIRECT_AFTER_AUTH** — e.g. `http://localhost:5173/settings#byol-credentials`.
- **GITLAB_BASE_URL** (optional) — self‑hosted GitLab base URL; omit for gitlab.com. Do not set to your harness app URL.

Register the same callback under the GitLab app’s **Redirect URI**. Scopes include `read_repository`, `write_repository`, `api`. Flow uses CSRF + PKCE; refresh tokens refreshed when supported.

### 3.6 GitHub OAuth (Sign in with GitHub)

Configure the server:

- **GITHUB_CLIENT_ID** — [GitHub OAuth App](https://github.com/settings/developers)
- **GITHUB_CLIENT_SECRET**
- **GITHUB_REDIRECT_URI** — e.g. `http://localhost:3000/auth/github/callback` (must match the OAuth app **Authorization callback URL** exactly).
- **REDIRECT_AFTER_AUTH** — e.g. `http://localhost:5173/settings#byol-credentials`.

After authorization, the server stores the token on the identity and redirects to Settings (e.g. `?oauth_success=github` or `oauth_success=gitlab`).

---

*See also: [CLIENT_EXPERIENCE.md](CLIENT_EXPERIENCE.md) · [HOSTING.md §13 — Production checklist](HOSTING.md#13-production-and-first-run-checklist) · [TROUBLESHOOTING.md](TROUBLESHOOTING.md)*
