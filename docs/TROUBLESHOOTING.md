# Troubleshooting

**Purpose:** Symptom-indexed **operator** fixes (deploy, env, migrations, workers). Use this when something fails and you want **cause → steps → link** to the canonical doc.

**Not the same as [CLIENT_EXPERIENCE.md](CLIENT_EXPERIENCE.md)**—that spec defines **product behavior** (messages, SSE, credentials UX) for implementers. **Not a substitute for [ARCHITECTURE.md](ARCHITECTURE.md)**—deep behavior (e.g. job reclaim, Git §9a/9b) lives there; this page **points** to it.

---

## 1. Control plane and database

### 1a. CORS errors in the browser

**Symptom:** Browser console shows blocked requests; API calls from the Web UI fail with CORS policy errors.

**Cause:** The UI is served from one origin (e.g. `https://app.example`) and the API from another (e.g. `https://harness.example`). The server must respond with matching **`Access-Control-Allow-Origin`** (and, for some Chrome preflights, **`Access-Control-Allow-Private-Network`**).

**Built-in dev allowances (no env required):** In addition to **`CORS_ALLOWED_ORIGINS`**, the server allows **`http`/`https` origins on ports 5173, 4173, or 5174** when the host is **`localhost`**, **`127.0.0.1`**, **`::1`**, or a **private IPv4** address (e.g. **`http://192.168.1.10:5173`** from `vite --host`). Default merged origins also include **Vite preview** on **`localhost` / `127.0.0.1` / `[::1]`** for ports **4173** and **5173**. Chrome **Private Network Access** preflights get **`Access-Control-Allow-Private-Network: true`** from the CORS layer.

**Fix:**

1. For **production** UI hostnames, set **`CORS_ALLOWED_ORIGINS`** to a comma-separated list of **exact** UI origins (scheme + host + port), e.g. `https://app.example`.
2. Ensure the UI **Settings → Control plane URL** matches how you reach the API (same scheme/host/port as in CORS server-side expectations).
3. After changing CORS env, restart the server.

**If DevTools also shows `404` on `GET /health`:** The response is probably **not** from this control plane (wrong port, static/nginx “health”, or **two listeners** on port 3000 — e.g. **Docker Compose** and **`cargo run -p server`** on the same machine). Stop one stack or change **`PORT`**. Verify with:

`curl -sS -D - -H "Origin: http://localhost:5173" "http://127.0.0.1:3000/health"`

You should see **`200`**, JSON with **`"status":"ok"`**, **`access-control-allow-origin`**, and **`x-remote-harness-control-plane: 1`**.

See [HOSTING.md — Production checklist](HOSTING.md#13-production-and-first-run-checklist).

### 1b. `VersionMismatch` / SQLx migration checksum errors

**Symptom:** Server exits on startup with `VersionMismatch` or similar after pulling new code.

**Cause:** Embedded migration files were changed after they were already applied to your database. SQLx records a checksum per applied migration.

**Fix (local/dev):** Reset the database volume or drop/recreate the DB, then start again (e.g. `docker compose down -v` then `docker compose up --build`).

**Fix (shared/prod):** Never edit applied migrations; add **new** forward-only migrations only. Coordinate deploys with the team.

See [Architecture §2a](ARCHITECTURE.md#2a-schema-migrations).

### 1c. First API key / bootstrap security

If **`POST /api-keys/bootstrap`** is used, treat it like **root access** until a key exists. See [API_OVERVIEW §4c — Bootstrap safety](API_OVERVIEW.md#4c-rest--api-keys-control-plane-auth) and [HOSTING.md — Production checklist](HOSTING.md#13-production-and-first-run-checklist).

### 1e. `scripts/compose-smoke.sh` fails (job not completed, clone, or permissions)

**Symptom:** The Compose smoke script exits non-zero after starting the stack.

**Common causes:**

1. **Worker cannot push to the bind-mounted bare repo** — The smoke overlay runs the worker as **root** (`user: "0:0"` in `docker-compose.smoke.yml`) so pushes to the host-created temp directory succeed. If you modified that overlay, restore it or fix volume permissions.
2. **`RH_E2E_REPO_BARE_PATH` unset** — The script sets this automatically; do not run `docker compose` with the smoke file alone without the variable.
3. **Chat session stays `running` after the job completes** — Expected: **`workflow: chat`** leaves the session **`running`** for follow-up input; the smoke script waits on **`jobs[0].status === "completed"`**, not session `completed`. If **`jobs[0]`** never leaves **`assigned`**, see worker logs (often **`git clone`** on **`file://`**).
4. **Worker hang on `git clone`** — On **macOS + Docker Desktop**, bind mounts outside the project (e.g. **`/var/folders/...`**, sometimes **`/tmp`**) may not be visible in the Linux VM. The smoke script keeps the fixture under **`.compose-smoke-fixture/`** in the repo (ignored by git) so the mount is on a shared path.
5. **Port conflicts** — Another process is using **3000** (API) or **5173** (web). Stop it or adjust published ports in `docker-compose.yml`.
6. **Stale DB after schema changes** — `docker compose down -v` then re-run (see §1b).

See [GETTING_STARTED.md §1.9](GETTING_STARTED.md#19-automated-compose-smoke-tier-1--tier-2).

### 1d. OAuth redirect loop, `oauth_not_configured`, or “Sign in” does nothing

**Symptom A — JSON error `oauth_not_configured`:** (“GitLab/GitHub OAuth is not configured … set `GITLAB_CLIENT_ID` …”)

**Cause:** The server process does not see the three provider variables (`GITLAB_CLIENT_ID`, `GITLAB_CLIENT_SECRET`, `GITLAB_REDIRECT_URI`, or the GitHub equivalents). Common cases:

1. **Bare `cargo run -p server`:** A repo-root **`.env`** file is **not** read by the shell. The server loads **`.env` from the current working directory** and, when built with Cargo, from the **workspace root** `../../.env` relative to `crates/server`. Put OAuth vars in **`/path/to/remote_harness_3/.env`**, then restart the server. Alternatively `export` the variables in the same terminal before `cargo run`.
2. **`docker compose`:** Compose only injects variables that appear under the **`server` → `environment:`** block (or `env_file`). Root **`.env`** is used for **interpolation** (e.g. `${GITLAB_CLIENT_ID}`), but values must be forwarded into the container — see root **`docker-compose.yml`** (OAuth keys are listed there). After editing **`.env`**, run **`docker compose up --build`** (or recreate the server container).

**Symptom B — Wrong redirect or loop after the provider login page**

**Fix:** **`REDIRECT_AFTER_AUTH`**, **`GITHUB_REDIRECT_URI`** / **`GITLAB_REDIRECT_URI`**, and the OAuth app’s registered callback URL must match **exactly** (including `http` vs `https` and port). The callback URL is always on the **control plane** host (API port), not the UI dev port.

See [GETTING_STARTED.md §3.5–3.6](GETTING_STARTED.md#35-gitlab-oauth-sign-in-with-gitlab) (GitLab/GitHub OAuth) and [HOSTING.md](HOSTING.md).

---

## 2. Workers and jobs

### 2a. Worker registration or tasks fail after sleep or network blip

Workers may need to re-register; the control plane may mark workers **stale** and **reclaim** jobs. See [Architecture §3b — Worker death, job reclaim](ARCHITECTURE.md#3b-worker-death-job-reclaim-and-bounded-retries).

### 2b. No commit, push, or merge request

**Symptom:** Job finished (or failed) but there is no commit on the remote, no push, or no PR/MR when you expected one.

**Understand the pipeline:**

1. **Worker** — When commit/push is attempted and failure modes: [Architecture §9a](ARCHITECTURE.md#9a-when-the-worker-attempts-commit-and-push).
2. **Control plane** — When a PR/MR is created: [Architecture §9b](ARCHITECTURE.md#9b-when-the-control-plane-creates-a-prmr).

**Checklist:**

1. **Job record** (`GET /sessions/:id` or UI): `status` (**completed** vs **failed**), **`error_message`**, **`pull_request_url`**, and any **branch / commit_ref** fields exposed by the API.
2. **Session params:** For an MR/PR, **`branch_mode`** must be **`"pr"`** (exact string).
3. **Worker logs** for the task/session: agent run failed → often no commit; **clone/checkout** / **Git message generation** / **create branch** / **commit or push** failures each map to different messages—see §9a table.
4. **Server logs:** MR API failures often log while the job still **completes**; **repo URL not recognized** means no MR for that host.

**User-facing behavior** when explaining missing Git/MR: [CLIENT_EXPERIENCE §8](CLIENT_EXPERIENCE.md#8-git-commit-push-and-prmr-outcomes).

### 2c. Job fails: `failed to spawn agent CLI` / `No such file or directory`

**Symptom:** Job `error_message` mentions spawning `agent` or `claude`, or **errno 2** (file not found).

**Cause:** The worker process cannot execute the vendor CLI (not installed, wrong `PATH`, or wrong architecture — e.g. macOS binary inside a Linux container).

**Fix:**

1. On the **same machine/container** as the worker, ensure **Cursor**’s `agent` or **Claude Code**’s `claude` exists, or set **`CURSOR_AGENT_PATH`** / **`CLAUDE_CLI_PATH`** (aliases: **`REMOTE_HARNESS_CURSOR_AGENT_BIN`**, **`REMOTE_HARNESS_CLAUDE_BIN`**).
2. **Docker:** **`Dockerfile.worker`** bundles **Cursor** at build time; if **`agent`** is still missing, rebuild with a current **`CURSOR_AGENT_VERSION`** (see `docker-compose.yml` / [GETTING_STARTED §1.4](GETTING_STARTED.md#14-agent-cli-in-the-worker-container)). **Claude** is not bundled — extend the image or set **`CLAUDE_CLI_PATH`**. For smoke without a vendor CLI, set **`REMOTE_HARNESS_STUB_AGENT=1`** (see `docker-compose.smoke.yml`).

See [GETTING_STARTED.md §1.4](GETTING_STARTED.md#14-agent-cli-in-the-worker-container) and [CLIENT_EXPERIENCE §6](CLIENT_EXPERIENCE.md#6-jobs-failures-outside-the-users-control).

### 2d. Git clone: “too many redirects or authentication replays”

See [GIT_CLONE_SPEC.md](GIT_CLONE_SPEC.md).

### 2e. Cursor: “invalid API key” / “invalid token” in job logs

**Symptom:** Worker/agent output mentions an invalid **Cursor** API key, token, or `CURSOR_API_KEY`.

**Common causes:**

1. **Wrong credential** — The **agent** (BYOL) value must be a **Cursor User API key** from [Cursor Dashboard → Cloud Agents](https://cursor.com/dashboard/cloud-agents) ([CLI auth docs](https://cursor.com/docs/cli/reference/authentication)). It is **not** your GitHub/GitLab PAT, not an OpenAI/Anthropic key, and not the Remote Harness **control plane API key**.
2. **Whitespace** — Accidental newline or space when pasting breaks auth. **Re-save** the token in **Settings → Identity & credentials** (or `PATCH /identities/:id` / CLI `identity patch`); the server trims stored tokens on write, and the worker trims when merging credentials.
3. **Network / DNS** — Cursor’s CLI may surface “invalid API key” when it **cannot reach** Cursor’s servers (firewall, VPN, offline). From the worker host: confirm HTTPS to the public internet works; see [Cursor forum reports](https://forum.cursor.com/) for DNS-related false “invalid key” messages.
4. **Plan / product limits** — Some agent flows require a **Cursor** subscription tier or disallow certain API-key-only usage; the vendor’s error text is authoritative.

**Quick check:** On any machine with `agent` installed, run `export CURSOR_API_KEY='…'` then `agent status` (or a trivial `agent run -f --print "ping"`). If that fails, fix the key or network before debugging Remote Harness.

See [CLIENT_EXPERIENCE §5](CLIENT_EXPERIENCE.md#5-credentials-and-byol).

---

*See also: [CLIENT_EXPERIENCE.md](CLIENT_EXPERIENCE.md) | [HOSTING.md](HOSTING.md) | [Architecture](ARCHITECTURE.md)*
