# Troubleshooting

**Purpose:** Symptom-indexed **operator** fixes (deploy, env, migrations, workers). Use this when something fails and you want **cause → steps → link** to the canonical doc.

**Not the same as [CLIENT_EXPERIENCE.md](CLIENT_EXPERIENCE.md)**—that spec defines **product behavior** (messages, SSE, credentials UX) for implementers. **Not a substitute for [ARCHITECTURE.md](ARCHITECTURE.md)**—deep behavior (e.g. job reclaim, Git §9a/9b) lives there; this page **points** to it.

---

## 1. Control plane and database

### 1a. CORS errors in the browser

**Symptom:** Browser console shows blocked requests; API calls from the Web UI fail with CORS policy errors.

**Cause:** The UI is served from one origin (e.g. `https://app.example`) and the API from another (e.g. `https://harness.example`). The server only allows origins listed in its CORS configuration.

**Fix:**

1. Set **`CORS_ALLOWED_ORIGINS`** on the server to a comma-separated list of **exact** UI origins (scheme + host + port), e.g. `https://app.example,http://localhost:5173`.
2. Ensure the UI **Settings → Control plane URL** matches how you reach the API (same scheme/host/port as in CORS server-side expectations).
3. After changing CORS env, restart the server.

See [HOSTING.md — Production checklist](HOSTING.md#13-production-and-first-run-checklist).

### 1b. `VersionMismatch` / SQLx migration checksum errors

**Symptom:** Server exits on startup with `VersionMismatch` or similar after pulling new code.

**Cause:** Embedded migration files were changed after they were already applied to your database. SQLx records a checksum per applied migration.

**Fix (local/dev):** Reset the database volume or drop/recreate the DB, then start again (e.g. `docker compose down -v` then `docker compose up --build`).

**Fix (shared/prod):** Never edit applied migrations; add **new** forward-only migrations only. Coordinate deploys with the team.

See [Architecture §2a](ARCHITECTURE.md#2a-schema-migrations).

### 1c. First API key / bootstrap security

If **`POST /api-keys/bootstrap`** is used, treat it like **root access** until a key exists. See [API_OVERVIEW §4c — Bootstrap safety](API_OVERVIEW.md#4c-rest--api-keys-control-plane-auth) and [HOSTING.md — Production checklist](HOSTING.md#13-production-and-first-run-checklist).

### 1d. OAuth redirect loop or “Sign in” does nothing

**Symptom:** GitHub/GitLab OAuth fails or returns to the wrong page.

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

### 2c. Git clone: “too many redirects or authentication replays”

See [GIT_CLONE_SPEC.md](GIT_CLONE_SPEC.md).

---

*See also: [CLIENT_EXPERIENCE.md](CLIENT_EXPERIENCE.md) | [HOSTING.md](HOSTING.md) | [Architecture](ARCHITECTURE.md)*
