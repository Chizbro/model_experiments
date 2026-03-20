# Client experience (CLI & Web UI)

This document specifies **expected behavior** so operators, implementers, and UX stay aligned: users should rarely hit a “silent failure” or unexplained spinner. It complements [API_OVERVIEW.md](API_OVERVIEW.md) (contracts) and [HOSTING.md](HOSTING.md) (deployment).

---

## 1. Principles

| Principle | Meaning |
|-----------|---------|
| **Same truth everywhere** | CLI and Web UI surface the same session/job state and errors from the API. |
| **Fail loud, classify** | On failure, show a **clear message**, the server **`error.code`** when present, and whether the user can fix it (settings, credentials) vs wait / retry / contact admin. |
| **Streaming resilience** | Log and session **SSE** streams disconnect; clients **reconnect with backoff** and do not pretend the session ended unless the API says so. |
| **No silent CORS or network** | Distinguish **network / CORS / DNS** (browser cannot reach API) from **401** (wrong key) from **5xx** (server error). |

---

## 2. API errors (REST)

All error bodies should follow [API_OVERVIEW §2 — Standard error response](API_OVERVIEW.md#2-standard-error-response):

```json
{
  "error": {
    "code": "string",
    "message": "string",
    "details": {}
  }
}
```

### 2.1 Web UI mapping

| Situation | What to show | Action hints |
|-----------|----------------|--------------|
| **`401` / `unauthorized`** | “Not authorized” + server `message` | Check **Settings → API key**; avoid showing whether the key is “empty” vs “wrong” beyond generic copy. |
| **`400` / `invalid_request`** | Server `message` + field hints if in `details` | Fix form input; link to docs if `details` references a param. |
| **`404` / `not_found`** | Resource missing (session, job, etc.) | Link back to list view. |
| **`409` / conflict** | e.g. session not accepting input | Explain state (e.g. session completed); suggest refresh. |
| **`500` / server error** | “Something went wrong on the server” + optional `message` | Retry; if persistent, show support / logs path. |
| **`503` on OAuth start** | “Sign-in is not configured on this server” | Admin must set OAuth env vars; do not leave a blank redirect. |

### 2.2 CLI mapping

Print **stderr**: HTTP status, `error.code`, and `error.message`. Exit non-zero. **v1:** human-readable output only; **`--json` is not part of the v1 contract** ([API_OVERVIEW.md](API_OVERVIEW.md) — Spec delivery). Add `--json` only in a release that documents it for every affected subcommand.

---

## 3. Browser: network, TLS, and CORS

| Situation | What users see | Implementation note |
|-----------|----------------|----------------------|
| **Failed fetch** (no response, timeout) | “Cannot reach the control plane” + show configured base URL | If [wake URL](HOSTING.md#4-wake-integration-optional-deployer-provided) is configured, offer **Wake up** + “Retry”. |
| **TLS / certificate errors** | “Secure connection failed” | User must fix cert or use the correct URL (common with Tailscale MagicDNS vs raw IP mismatch). |
| **CORS blocked** | “Browser blocked the request (CORS).” | Tell user: admin must add this UI origin to **`CORS_ALLOWED_ORIGINS`**. Link to [TROUBLESHOOTING §1a](TROUBLESHOOTING.md#1a-cors-errors-in-the-browser). |

Do **not** treat CORS as a generic “network error” without mentioning CORS when `TypeError: Failed to fetch` and the console shows a policy block.

---

## 4. SSE (logs and session events)

Contract: [API_OVERVIEW §6–7](API_OVERVIEW.md).

| Behavior | Requirement |
|----------|-------------|
| **Initial load** | Fetch **full** log history for the context first, then open SSE (see API client contract). |
| **Disconnect** | Show **“Reconnecting…”** (or non-alarming equivalent); exponential backoff; cap max interval (e.g. 30s). |
| **Reconnect** | On reconnect, optionally **delta** from last seen timestamp if the API supports it later; v1 may re-open SSE only for new events if history was already loaded (document in UI impl). |
| **Session ended** | Close stream when session/job is **completed** or **failed** per API state, not merely when SSE drops. |

---

## 5. Credentials and BYOL

| Situation | UX |
|-----------|-----|
| **Session create rejected** (missing git or agent token) | Direct user to **Settings → Credentials** (or CLI `credentials set`) with one sentence: both Git and agent tokens are required for this workflow. |
| **Token health** | Use `GET /identities/:id/auth-status` for Git expiry messaging; show “Re-auth” or “Refresh” when `expired_needs_reauth` or `expiring_soon`. |
| **Git OAuth succeeded** | Redirect to Settings with clear success (`credentials=github_ok` etc.); refresh credential status. |
| **Provider API errors** (repo list) | Show `502` / provider message as “GitHub/GitLab temporarily unavailable or token invalid” — not a generic crash. |

Agent (Cursor / Claude Code) tokens in v1 are set via UI/CLI/PATCH, not via a separate OAuth spec in the API—see [PRODUCT.md — BYOL](PRODUCT.md#bring-your-own-licence-byol).

---

## 6. Jobs: failures outside the user’s control

Surface server and worker semantics from [Architecture §3b](ARCHITECTURE.md#3b-worker-death-job-reclaim-and-bounded-retries) and job `error_message`:

| Pattern / code | User-facing gist |
|----------------|------------------|
| **Stale worker / reclaim** | “The worker stopped responding; the job was reassigned.” (Optional: reclaim count if shown in API.) |
| **`[MAX_WORKER_LOSS_RETRIES]`** | “This job failed after several worker interruptions. Start a new session or retry from the dashboard.” |
| **`[JOB_LEASE_EXPIRED]`** | “The job ran too long and was stopped by policy.” |
| **`[AUTH_EXPIRED]`** (or clone auth) | “Git authentication failed—refresh Git credentials.” |

Always show **what happened** and **what to do next** in one short paragraph.

---

## 7. First-time setup (Web UI)

| Step | UX |
|------|-----|
| **Control plane URL** | Required before anything else; validate with `GET /health` (no auth). |
| **API key** | If `GET /health` works but authenticated calls fail with `401`, prompt for an API key. **Bootstrap:** Offer **`POST /api-keys/bootstrap`** **only** when a deliberate probe shows keys are missing (e.g. `401` on `GET /api-keys` or documented “no keys” response)—**never** show a permanent “create key without auth” affordance on every visit. After the first key exists, hide bootstrap entirely. |
| **Credentials** | After API key works, prompt for BYOL/Git setup if user opens “New session” without tokens. |

See [HOSTING.md — Production checklist](HOSTING.md#13-production-and-first-run-checklist) and [HOSTING.md — Web UI threat model](HOSTING.md#14-web-ui-threat-model-api-key-in-browser).

### 7.1 Git “planning” failures (branch/MR text)

When the **short planning step** in [Architecture §9 — Git integration](ARCHITECTURE.md#9-git-integration-workers) (e.g. generating branch or MR title before/after the main agent run) fails, that is a **user-visible job failure** with a clear reason (not a generic “agent failed”). Copy should distinguish **setup/planning** vs **main agent run** vs **Git push** where the API exposes that distinction (e.g. via `error_message`).

---

## 8. Git commit, push, and PR/MR outcomes

Users often expect a **commit**, **push**, or **open PR/MR** after a run. Those outcomes are **conditional** on worker behavior, session params, and provider APIs. **Behavior model:** [Architecture §9a–9b](ARCHITECTURE.md#9a-when-the-worker-attempts-commit-and-push). **Operators:** [TROUBLESHOOTING §2b](TROUBLESHOOTING.md#2b-no-commit-push-or-merge-request).

### 8.1 Mapping API state to user-visible copy

| Situation | Web UI / CLI requirement |
|-----------|---------------------------|
| **Job failed** (`failed`, `error_message` set) | Show **error_message** prominently on the job and session summary; link to **Settings → Credentials** when the message indicates auth or git. |
| **Job completed, no `commit_ref`** | Explain that push/commit did not complete; point to logs and `error_message`. Do not label as “succeeded cleanly” if Git output is missing. |
| **Job completed with commit, no `pull_request_url`** when user chose PR/MR mode (`branch_mode === "pr"`) | Show a **one-line reason**, not silence, chosen from: job not successful; missing branch/title in payload; provider not GitHub/GitLab; provider API error (token/scopes)—see [Architecture §9b](ARCHITECTURE.md#9b-when-the-control-plane-creates-a-prmr). |
| **Agent exited non-zero but commits may exist on the remote** | Explain that **job status** reflects the agent exit, while **Git** may still show commits from the attempt; MR may be skipped when status is not success ([Architecture §9a edge case](ARCHITECTURE.md#9a-when-the-worker-attempts-commit-and-push)). |

### 8.2 Operator signals (logs)

Where feasible, surface or document for support: server messages such as **PR/MR creation failed (task still completed)** or **repo_url not recognized** as a **non-blocking** note or session hint so admins are not blind to partial success.

| Data from API | UI / CLI behavior |
|---------------|-------------------|
| `jobs[].error_message` | Always visible on job row and session detail when set; never hide behind “Failed” alone. |
| `jobs[].status` + `pull_request_url` | If the user expected an MR and `pull_request_url` is null after completion, show an explanation per §8.1—do not imply a silent platform bug. |
| `commit_ref` / branch fields | If missing when a push was expected, point to logs and [TROUBLESHOOTING §2b](TROUBLESHOOTING.md#2b-no-commit-push-or-merge-request). |

---

## 9. Log retention and purge

| Requirement | Detail |
|-------------|--------|
| **Surface defaults** | In **Settings** (or an obvious “About / Data” area), show **default log retention** (e.g. 7 days) and that **retain forever** exists per session/job. |
| **Before data loss** | If the product can know purge time, show relative expiry; if not, show static copy: “Logs older than \[N\] days may be deleted unless marked retain forever.” |
| **Manual delete (CLI + Web)** | **Same API** for both: **`DELETE /sessions/:id/logs`** with optional **`job_id`** ([API_OVERVIEW §6 — Delete session logs](API_OVERVIEW.md#delete-session-logs)). CLI: [TECH_STACK §3 — `logs delete`](TECH_STACK.md#3-cli--rust). Web UI: equivalent on session/job detail; use a **confirm** step for destructive delete. |
| **Recovery** | Link to **dual-write / disk logs** in [Architecture §6](ARCHITECTURE.md#6-logging-architecture) in operator docs—not all users read it; a one-line tooltip is enough (“Older logs may still exist on server/worker disk if enabled”). |

---

## 10. Worker pool heterogeneity warnings

| Requirement | Detail |
|-------------|--------|
| **Detect** | When **two or more** non-stale workers register with **different** `labels.platform` values, or when platforms mix **WSL vs native Windows**, treat the pool as **heterogeneous**. |
| **Warn** | **Workers** list or dashboard banner: explain that **the engine may assign any session to any worker** and that **mixed OS or missing CLIs** cause confusing failures; link [Architecture §4c](ARCHITECTURE.md#4c-platform-specific-workers-cli-invocation). |
| **Session create** | Optional **confirmation** when session `agent_cli` is set but no worker has advertised a compatible environment (v1: heuristic only—full **capability dispatch** is [Product O4](PRODUCT.md)). At minimum, do not imply “any worker will work.” |

---

## 11. Web UI and API key (operator expectations)

The v1 UI stores the control plane API key in **browser storage** (see [TECH_STACK.md §4](TECH_STACK.md)). **Operators should:**

- Deploy the UI with a **strict CSP**, **HTTPS**, and **no untrusted third-party scripts**.
- Treat machines with the UI open as **high trust** for that API key; prefer **CLI** or **separate keys per person** when **shared workstations** are possible.
- **Rotate** API keys if the UI storage might have been exposed.

This is **not** a substitute for network ACLs on the control plane—see [HOSTING.md §14](HOSTING.md#14-web-ui-threat-model-api-key-in-browser).

---

## 12. Long chat sessions

When `task_input.history_truncated` is **`true`** (see [API_OVERVIEW — Pull task](API_OVERVIEW.md#pull-task)), the Web UI and CLI **must** show that **only the latest N turns** are sent to the agent on the next job (N from server config, default 50 per side). Suggested copy: “Long conversation: only the most recent messages are included in the next agent run; full history may appear in logs.”

---

## 13. Compatibility and upgrades

Worker and server **must** implement **`client_version`** on **`POST /workers/register`** ([API_OVERVIEW](API_OVERVIEW.md)). On **`400`** with `error.code === "worker_version_incompatible"`, show: **“Update the worker to match the control plane version (see release notes).”** Do not treat as a generic network error.

---

*See also: [API_OVERVIEW.md](API_OVERVIEW.md) | [TROUBLESHOOTING.md](TROUBLESHOOTING.md) | [HOSTING.md](HOSTING.md)*
