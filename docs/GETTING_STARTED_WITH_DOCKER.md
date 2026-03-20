# Getting Started with Docker (macOS)

Run the full stack in Docker with a single `docker compose up`: PostgreSQL, control plane (backend), worker, and Web UI. Use the **CLI on your laptop** against the backend, open the **UI in your browser**, and **view logs** from the UI or CLI as usual.

---

## Prerequisites

- **Docker** and **Docker Compose** (Docker Desktop on Mac includes both)
- **Rust** on your host (only for the CLI; the server and worker run inside containers)

---

## 1. Configuration

The stack uses one shared **API key** for the server, worker, and your CLI/UI. Default is `dev-key-change-in-production` so you can run with no env set. To override:

```bash
export API_KEY="your-secret-key"
```

Then run `docker compose` in the same shell, or create a `.env` file in the repo root:

```
API_KEY=your-secret-key
```

---

## 2. Start everything

From the repo root:

```bash
docker compose up --build
```

First run will build the server and worker (Rust) and the Web UI (Node then nginx), which can take a few minutes. Later runs reuse the images. **If the worker previously failed with "Name or service not known" for host `server`**, rebuild so the worker starts only after the server is healthy: `docker compose build --no-cache server worker && docker compose up -d`.

You should see:

- **postgres** healthy, then **server** listening on 3000
- **worker** registering with the control plane
- **web** serving on 80 (mapped to 5173 on the host)

Stop with `Ctrl+C`. Run in the background with `docker compose up -d --build`.

**Migrations / “server exits (1)” after `git pull`:** If **`docker compose logs server`** shows **`VersionMismatch(…)`**, your Postgres volume has an older migration history than the new server image expects (often after changing migration files). For local dev, reset the DB volume and bring the stack back up:

```bash
docker compose down -v
docker compose up -d --build
```

See [TROUBLESHOOTING.md §1b](TROUBLESHOOTING.md#1b-versionmismatch--sqlx-migration-checksum-errors) for details.

**Sleep-inhibit:** If you run a host-side helper that polls the backend to decide when to allow the OS to sleep, call **`GET http://localhost:3000/health/idle`** (or the published port) from the host. 200 = OK to sleep, 503 = busy. See [HOSTING.md](HOSTING.md) (§4a Idle sleep / sleep-inhibit).

### 2a. Worker and backend lifecycle

- **Restart policy:** All services use **`restart: unless-stopped`** in our compose file. The worker and backend containers auto-restart after a panic or OOM while the host is awake.
- **Sleep/wake:** When the **host** sleeps (e.g. laptop lid), the Docker VM suspends; all containers freeze. On wake, the VM resumes, containers resume, and the worker re-heartbeats (and may re-register if its previous registration was marked stale). No manual step needed. See [HOSTING.md](HOSTING.md) §9 for detailed behavior.
- **Scaling workers:** To run more than one worker: **`docker compose up --scale worker=N`** (e.g. `docker compose up -d --scale worker=2`).
- **Stuck jobs:** If a job stays assigned to a worker that has gone stale, jobs are automatically reclaimed and reassigned. See [TROUBLESHOOTING.md](TROUBLESHOOTING.md) for worker/stale and job-reclaim issues.

---

## 3. Use the UI

1. Open **http://localhost:5173** in your browser (the web container exposes 80 → host 5173).
2. Go to **Settings** and set:
   - **Control plane URL:** `http://localhost:3000`
   - **API key:** `dev-key-change-in-production` (or whatever you set for `API_KEY`)
3. Use the dashboard: create sessions, open session detail, and use the **log panel** (history then live stream) and attach as usual.

---

## 4. Providing git and agent tokens (run the real agent)

The worker runs the real agent (Cursor/Claude Code CLI) only when credentials are configured. You must provide **agent_token** (e.g. Cursor API key) and **git_token** (e.g. GitHub PAT) before creating sessions; otherwise session creation is rejected. With Docker you can do any of the following.

### Option A: UI Settings — Credentials (BYOL)

In the Web UI, open **Settings** and scroll to **Credentials (BYOL)**. Enter your **Git token** and **Agent token**, then click **Save tokens**. They are stored on the server for the default identity. You can also use **Sign in with GitLab** or **Sign in with GitHub** if the server is configured for OAuth (see Option D / Option E).

### Option B: curl (one-off from your laptop)

```bash
curl -s -X PATCH \
  -H "Authorization: Bearer dev-key-change-in-production" \
  -H "Content-Type: application/json" \
  -d '{"agent_token":"YOUR_CURSOR_API_KEY","git_token":"YOUR_GITHUB_PAT"}' \
  http://localhost:3000/identities/default
```

### Option C: CLI — credentials set

From your laptop (with `REMOTE_HARNESS_URL` and `REMOTE_HARNESS_API_KEY` set):

```bash
cargo run -p cli -- credentials set
```

You’ll be prompted for Git token and Agent token (masked). Or pass them via flags or env:

```bash
cargo run -p cli -- credentials set --git-token "ghp_xxx" --agent-token "cursor_xxx"
export GIT_TOKEN="ghp_xxx" AGENT_TOKEN="cursor_xxx"
cargo run -p cli -- credentials set
```

### Option D: Optional — GitLab OAuth (Sign in with GitLab)

To enable **Sign in with GitLab** in the UI, set these in a **`.env`** file in the repo root:

- **GITLAB_CLIENT_ID** — from your [GitLab Application](https://gitlab.com/-/user_settings/applications) (or your self‑hosted GitLab).
- **GITLAB_CLIENT_SECRET** — same application.
- **GITLAB_REDIRECT_URI** — callback URL on the **server** (where the token is exchanged), e.g. `http://localhost:3000/auth/gitlab/callback`. Use the API/server port (e.g. 3000), not the UI port.
- **REDIRECT_AFTER_AUTH** — where to send the user after success, e.g. `http://localhost:5173/settings`.
- **GITLAB_BASE_URL** (optional) — leave unset for gitlab.com. If self‑hosted, set to the GitLab server URL (e.g. `https://gitlab.example.com`). Do not set this to your app URL (e.g. localhost:3000) or the flow will break.

In your GitLab application, add the callback URL under “Redirect URI”. After the user authorizes, the server stores the GitLab access token (plus refresh token and expiry) as the identity’s git token and redirects to Settings with success. The server requests scopes read_repository, write_repository, and api (GitLab exposes `api`, not `read_api`; it is required for listing projects in the repo picker). The OAuth flow uses CSRF nonce validation and PKCE (S256) for security. Access tokens are automatically refreshed when they expire.

### Option E: Optional — GitHub OAuth (Sign in with GitHub)

To enable **Sign in with GitHub** in the UI, set these in **`.env`**:

- **GITHUB_CLIENT_ID** — from [GitHub OAuth App](https://github.com/settings/developers).
- **GITHUB_CLIENT_SECRET** — same OAuth app.
- **GITHUB_REDIRECT_URI** — exact callback URL, e.g. `http://localhost:3000/auth/github/callback`.
- **REDIRECT_AFTER_AUTH** — where to send the user after success, e.g. `http://localhost:5173/settings`.

Add the callback URL in your GitHub OAuth app. After the user authorizes, the server stores the GitHub access token as the identity’s git token and redirects to Settings with success.

---

## 5. Agent CLI in the worker container

The worker image **includes the Cursor Agent CLI** (installed at build time via the official install script). So when you run `docker compose up`, the worker can run tasks that use `agent_cli: "cursor"` without any extra setup.

- **No extra step needed** — create sessions and run workflows; the worker will find `/root/.local/bin/agent` and run it with the credentials you stored in Settings.
- **Override with host binary (optional):** If you want the worker to use the agent binary from your host (e.g. a specific version), mount it and set the env var:
  - **Linux host:** e.g. `-v /usr/local/bin/agent:/usr/local/bin/agent` and `CURSOR_AGENT_PATH: /usr/local/bin/agent` in the worker service.
  - **macOS host:** The Mac `agent` binary is not runnable inside a Linux container. Use the in-image CLI (default) or run the worker natively on your Mac (not in Docker) so it uses your host’s agent.

For Claude Code CLI instead of Cursor, set `CLAUDE_CLI_PATH` to the path of the `claude` binary inside the container (install it in your own image or mount it the same way).

---

## 6. Use the CLI from your laptop

The CLI runs on your machine and talks to the backend at `localhost:3000`:

```bash
export REMOTE_HARNESS_URL="http://localhost:3000"
export REMOTE_HARNESS_API_KEY="dev-key-change-in-production"
cargo run -p cli -- health
cargo run -p cli -- session list
cargo run -p cli -- logs tail --session-id <id>
cargo run -p cli -- attach <session-id>
```

Or use `~/.config/remote-harness/config.yaml` with `control_plane_url` and `api_key`.

---

## 7. Accessing logs

- **UI:** Open a session → log panel loads full history, then streams. Same as non-Docker.
- **CLI:** `remote-harness logs tail --session-id <id>` (and `attach`) — same as non-Docker.
- Logs are stored on the **server** (PostgreSQL) and streamed via the API; the browser and CLI both hit `http://localhost:3000`, so no extra configuration.

---

## 8. What this setup uses

| Piece | Purpose |
|-------|--------|
| **docker-compose.yml** | Defines four services: `postgres`, `server`, `worker`, `web`. Server and worker share one Rust image (different `command`). Postgres has a healthcheck so the server starts after the DB is ready. |
| **Dockerfile** (repo root) | Multi-stage build: compile `server` and `worker` in a Rust image, then copy both into a slim image and install the Cursor Agent CLI (so the worker can run tasks). Single image used for both server and worker containers. |
| **web/Dockerfile** | Builds the Vite app in Node, then serves the built files with nginx. SPA routing is handled by nginx (`try_files` → `index.html`). |
| **CORS on the server** | The Web UI is served from `http://localhost:5173` and the API from `http://localhost:3000`, so the browser makes cross-origin requests. The server adds CORS headers (allowed origins include localhost:5173 and 127.0.0.1:5173; optional env `CORS_ALLOWED_ORIGINS` for custom origins). If you still see CORS errors, see [TROUBLESHOOTING.md §1a](TROUBLESHOOTING.md#1a-cors-errors-in-the-browser). |

---

## 9. Optional: override API key in compose

To force a different key without exporting env:

```bash
API_KEY=my-secret docker compose up --build
```

Or in `.env`:

```
API_KEY=my-secret
```

Then use `my-secret` in the UI Settings and in `REMOTE_HARNESS_API_KEY` for the CLI.

---

*See also: [CLIENT_EXPERIENCE.md](CLIENT_EXPERIENCE.md) | [HOSTING.md §13 — Production checklist](HOSTING.md#13-production-and-first-run-checklist) | [TROUBLESHOOTING.md](TROUBLESHOOTING.md)*
