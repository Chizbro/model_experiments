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

Run `docker compose` in the same shell, or put `API_KEY=...` in a **`.env`** file in the repo root.

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

The worker image includes the **Cursor Agent CLI**. Sessions with `agent_cli: "cursor"` work without extra setup (`/root/.local/bin/agent`).

- **Override (Linux host):** Mount the binary and set e.g. `CURSOR_AGENT_PATH`.
- **macOS host:** Do not mount the Mac `agent` binary into the Linux worker — use the image CLI or run the worker **bare metal** on the Mac.

For **Claude Code**, set `CLAUDE_CLI_PATH` inside the image or mount a Linux `claude` binary.

### 1.5 CLI from your laptop (Docker)

```bash
export REMOTE_HARNESS_URL="http://localhost:3000"
export REMOTE_HARNESS_API_KEY="dev-key-change-in-production"
cargo run -p cli -- health
cargo run -p cli -- session list
cargo run -p cli -- logs tail --session-id <id>
cargo run -p cli -- attach <session-id>
```

Or `~/.config/remote-harness/config.yaml` with `control_plane_url` and `api_key`.

### 1.6 Logs (Docker)

**UI:** Session → log panel (history then stream). **CLI:** `logs tail` / `attach`. Logs live in Postgres on the server; browser and CLI call the same API URL.

### 1.7 What this setup uses

| Piece | Purpose |
|-------|--------|
| **docker-compose.yml** | `postgres`, `server`, `worker`, `web`; server/worker share one image; Postgres healthcheck before server. |
| **Dockerfile** (repo root) | Builds server + worker; worker image includes Cursor CLI. |
| **web/Dockerfile** | Vite build + nginx; SPA `try_files` → `index.html`. |
| **CORS** | UI origin `http://localhost:5173` (and often `127.0.0.1:5173`) must be allowed; see `CORS_ALLOWED_ORIGINS` and [TROUBLESHOOTING.md §1a](TROUBLESHOOTING.md#1a-cors-errors-in-the-browser). |

### 1.8 Optional: API key without exporting env

```bash
API_KEY=my-secret docker compose up --build
```

Use `my-secret` in UI Settings and `REMOTE_HARNESS_API_KEY`.

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
| `HOST` | server | `0.0.0.0` |
| `PORT` | server | `3000` |
| `WORKER_STALE_SECONDS` | server | `90` |
| `MAX_JOB_RECLAIMS` | server | `3` |
| `JOB_LEASE_SECONDS` | server | `0` (disabled); e.g. `21600` = 6h lease / `[JOB_LEASE_EXPIRED]` |
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

Open the URL Vite prints. **Settings:** Control plane **http://127.0.0.1:3000**, API key **dev-key-change-in-production**.

**Terminal 4 — CLI (optional)**

```bash
cd /path/to/remote_harness_3
export REMOTE_HARNESS_URL="http://127.0.0.1:3000"
export REMOTE_HARNESS_API_KEY="dev-key-change-in-production"
cargo run -p cli -- health
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

**Settings → Credentials (BYOL):** Git token and Agent token → **Save**. Optional **Sign in with GitHub** or **Sign in with GitLab** if the server is configured ([§3.5](#35-gitlab-oauth-sign-in-with-gitlab), [§3.6](#36-github-oauth-sign-in-with-github)).

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
cargo run -p cli -- credentials set
```

Masked prompts, or:

```bash
cargo run -p cli -- credentials set --git-token "ghp_xxx" --agent-token "cursor_xxx"
export GIT_TOKEN="ghp_xxx" AGENT_TOKEN="cursor_xxx"
cargo run -p cli -- credentials set
```

Set `REMOTE_HARNESS_URL` and `REMOTE_HARNESS_API_KEY` to your control plane first.

### 3.4 Per-session tokens (optional)

On **`POST /sessions`**, put **`agent_token`**, **`git_token`**, and workflow fields (e.g. **`prompt`**, **`agent_cli`**: `"cursor"` or `"claude_code"`) in **params**. Params merge with the identity; supplied tokens override for that session only without updating the stored identity.

### 3.5 GitLab OAuth (Sign in with GitLab)

Configure the **server** (e.g. repo root **`.env`** for Compose, or env for bare metal):

- **GITLAB_CLIENT_ID** — [GitLab Application](https://gitlab.com/-/user_settings/applications) (or self‑hosted GitLab).
- **GITLAB_CLIENT_SECRET**
- **GITLAB_REDIRECT_URI** — callback on the **API** host, e.g. `http://localhost:3000/auth/gitlab/callback` (not the UI port).
- **REDIRECT_AFTER_AUTH** — e.g. `http://localhost:5173/settings`.
- **GITLAB_BASE_URL** (optional) — self‑hosted GitLab base URL; omit for gitlab.com. Do not set to your harness app URL.

Register the same callback under the GitLab app’s **Redirect URI**. Scopes include `read_repository`, `write_repository`, `api`. Flow uses CSRF + PKCE; refresh tokens refreshed when supported.

### 3.6 GitHub OAuth (Sign in with GitHub)

Configure the server:

- **GITHUB_CLIENT_ID** — [GitHub OAuth App](https://github.com/settings/developers)
- **GITHUB_CLIENT_SECRET**
- **GITHUB_REDIRECT_URI** — e.g. `http://localhost:3000/auth/github/callback` (must match the OAuth app **Authorization callback URL** exactly).
- **REDIRECT_AFTER_AUTH** — e.g. `http://localhost:5173/settings`.

After authorization, the server stores the token on the identity and redirects to Settings (e.g. `?credentials=github_ok`).

---

*See also: [CLIENT_EXPERIENCE.md](CLIENT_EXPERIENCE.md) · [HOSTING.md §13 — Production checklist](HOSTING.md#13-production-and-first-run-checklist) · [TROUBLESHOOTING.md](TROUBLESHOOTING.md)*
