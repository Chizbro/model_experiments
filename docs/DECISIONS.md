# Outstanding Decisions Log

Decisions that were marked as open in the docs, resolved one by one. Once decided, the main docs are updated and the decision is recorded here.

---

## 1. Worker discovery method

**Where:** ARCHITECTURE.md §3 — "Discovery options (pick one or support both)"

**Options:**
- **A.** Explicit server URL in env/config (worker has `CONTROL_PLANE_URL=...`, registers on boot).
- **B.** mDNS/DNS-SD (server advertises; workers discover on LAN).
- **C.** Message queue / broker (workers subscribe; server posts work to queue).

Doc stated options; we chose A only.

**Decision:** **A — Explicit server URL.** Workers use `CONTROL_PLANE_URL` (and auth) in env/config; register on boot; periodic heartbeat (see §8e). No mDNS or broker in scope; workers always configure the control plane URL explicitly.

---

## 2. Log retention policy

**Where:** ARCHITECTURE.md §6 — "retention policy TBD"

**Question:** How long to keep logs (by session/job)? Options: fixed days (e.g. 7, 30, 90), size-based cap, or "keep forever" with a future cleanup story.

**Decision:** **Default: 7 days** (configurable in server config). **Override:** sessions/jobs can be marked "retain forever". **Manual delete:** any logs (including retained-forever) can be deleted at any time via CLI or UI.

---

## 3. Control plane HTTP framework (Rust)

**Where:** TECH_STACK.md §1, §7, §8 — "axum or actix-web"

**Options:** **axum** (tokio-native, composable) vs **actix-web** (mature, actor-style).

**Decision:** **axum**

---

## 4. Control plane DB layer (Rust)

**Where:** TECH_STACK.md §1 — "sqlx or diesel"

**Options:** **sqlx** (async, compile-time SQL) vs **diesel** (sync-first, strong typing, migrations).

**Decision:** **sqlx**

---

## 5. Worker Git library (Rust)

**Where:** TECH_STACK.md §2 — "git2 or gitoxide"

**Options:** **git2** (libgit2 bindings, mature) vs **gitoxide** (pure Rust, faster for large repos). Must work with GitHub/GitLab using platform-managed tokens (user signs in via platform; control plane caches/refreshes; workers get credentials per job). Both support HTTPS + token; git2 has more examples for credential callbacks.

**Decision:** **git2**

---

## 6. Task queue: in-process first vs Redis/NATS from day one

**Where:** TECH_STACK.md §1 — "In-process first, then Redis or NATS"

**Question:** Start with in-memory/DB-backed queue and add Redis/NATS when scaling, or introduce Redis (or NATS) from the start?

**Decision:** **PostgreSQL table.** Task queue is implemented as a table in Postgres (same DB as registry/sessions). No Redis or NATS from day one; can add later if needed for scaling.

---

## 7. License

**Where:** README.md — "License: TBD"

**Decision:** **MIT**

---

## 8. CI/CD design and platform

**Where:** PROJECT_KICKOFF.md — "CI (optional): build, test, lint on push"

**Question:** Set up CI in Phase 0 or add later? Which platform?

**Decision:** **CI/CD design documented** in [CICD_DESIGN.md](CICD_DESIGN.md) (triggers, jobs for Rust + Web, conventions). **CI platform** and **git host** for this project are **TBD**; design is platform-agnostic so it can be implemented once chosen.

---

## 7b. Agent execution / licensing (BYOL)

**Where:** PRODUCT.md, TECH_STACK.md, ARCHITECTURE.md

**Decision:** **Bring your own licence (BYOL).** The platform does not call any AI models or APIs directly. Users sign in with a **Claude Code** or **Cursor** subscription; the control plane stores and refreshes that token. Workers run the **Claude Code** or **Cursor** CLI in the cloned repo, authenticated with the user’s token—those CLIs do the actual agent work. No platform-owned model licences.

---

## 8a. Hosting flexibility and power-saving

**Where:** HOSTING.md (new), ARCHITECTURE.md §9, PRODUCT.md O5

**Decision:** **Flexible topologies; optional wake integration.** The control plane (and DB) can run on an always-on server or on a sleepable machine (e.g. user’s Windows desktop or Mac laptop); workers can run on Windows (native or WSL), macOS, Linux. No mandated “always on.” For power-saving setups: optional **wake integration** — UI/CLI can call a **configurable wake URL or script** (deployer-provided) when the backend is unreachable (e.g. “Wake up” button); the harness does not implement WOL or Tailscale. User’s setup (control plane + workers on sleepable Windows/Mac, Tailscale, always-on host for WOL) is supported without locking other deployments into that architecture. See [HOSTING.md](HOSTING.md).

---

## 8d. Control plane auth (B1)

**Where:** TECH_STACK.md §6, API_OVERVIEW, DOC_REVIEW B1

**Decision:** **v1: API key only.** Control plane API (CLI, Web UI, and workers) use API keys. No OIDC and no mTLS in v1; add later if needed. Configuration for where and how to set the API key must be clearly documented (see TECH_STACK §6 and config section).

---

## 8e. Worker heartbeat (B2)

**Where:** PRODUCT F3, ARCHITECTURE §3, API_OVERVIEW, DOC_REVIEW B2

**Decision:** **v1: heartbeat in scope.** Workers call a heartbeat endpoint periodically (e.g. `POST /workers/:id/heartbeat`). Control plane updates last-seen and marks workers stale if heartbeats stop.

---

## 8f. Log persistence and observability (B4)

**Where:** ARCHITECTURE §6, TECH_STACK §5, DOC_REVIEW B4

**Decision:** **Dual-write for observability at all times.** (1) **All logs go to disk:** Control plane writes its own logs to local files on the backend; each worker writes all logs (agent, system, worker) to local files on that worker. Every log line is on disk somewhere. So if streaming or either client breaks, logs are still findable on backend or worker. (2) **Central store (DB):** Control plane stores its own logs and received worker logs in PostgreSQL so the **CLI and Web UI** can both show **all logs from all places** (agent, system, backend, worker) — the whole state of the system from either client. Retention, “retain forever,” and manual delete apply to the central store.

---

## 8g. Log tail protocol (B5)

**Where:** TECH_STACK §5, ARCHITECTURE §6, Summary table, DOC_REVIEW B5

**Decision:** **v1: SSE only.** Log tail and session attach use Server-Sent Events (one-way stream from server). No WebSocket for log tail in v1; add later if needed (e.g. if WebSocket is required for other real-time features).

---

## 8h. Worker task acquisition (B6)

**Where:** TECH_STACK §1, ARCHITECTURE §4, API_OVERVIEW, DOC_REVIEW B6

**Decision:** **v1: poll only.** Workers get tasks by calling a pull endpoint (e.g. `POST /workers/tasks/pull` or long-poll). No DB LISTEN/NOTIFY in v1; add later if we want push-style notification.

---

## 8i. BYOL sign-in mechanism (B7)

**Where:** PRODUCT BYOL, TECH_STACK, DOC_REVIEW B7

**Decision:** **v1: OAuth with Claude Code / Cursor** (when the provider offers it). User signs in via the provider’s OAuth flow. **Applies to both Web UI and CLI:** in the Web UI, user clicks “Sign in with Claude” / “Sign in with Cursor” and completes the redirect; in the CLI, the same auth is used (e.g. CLI opens a browser for the OAuth flow, or uses a device/code flow if the provider supports it). So whether you use the harness from the browser or the CLI, you authenticate to Claude Code/Cursor the same way.

---

## 8j. Control plane URL in Web UI (B8)

**Where:** TECH_STACK §4, DOC_REVIEW B8

**Decision:** **v1: configurable in the Web UI.** The user can set the control plane URL from the Web UI (e.g. in Settings). Stored in the browser (or backend per user/session) so the UI knows where to call. Lets you open the UI from anywhere and point it at a Tailscale URL (or any control plane URL). CLI and worker continue to use env/config for their URL.

---

## 8k. Default log retention (B9)

**Where:** DECISIONS §2, PRODUCT L5, ARCHITECTURE §6, DOC_REVIEW B9

**Decision:** **Default retention: 7 days.** Configurable in server config. Override: "retain forever" per session/job; manual delete anytime via CLI or UI.

---

## 8b. CI platform (when ready)

**Where:** CICD_DESIGN.md §4

**Question:** Which CI platform to use (e.g. GitHub Actions, GitLab CI, CircleCI)?

**Decision:** _TBD_

---

## 8c. Git host (when ready)

**Where:** CICD_DESIGN.md §4

**Question:** Which git host for this repo (e.g. GitHub, GitLab, self-hosted)?

**Decision:** _TBD_

---

## 9. OpenAPI / formal API spec from start

**Where:** PROJECT_KICKOFF.md — "API (optional): OpenAPI/Swagger"; API_OVERVIEW.md — "Refine when you start coding"

**Question:** Maintain an OpenAPI (or similar) spec from the start, or keep API_OVERVIEW as informal until later?

**Decision:** **Yes from start.** Maintain an OpenAPI (or equivalent) spec and keep it in sync with the server; contract-first so UI/CLI can generate or validate against it.

---

## 10. Changelog

**Where:** PROJECT_KICKOFF.md — "Changelog (optional): keep a CHANGELOG.md"

**Question:** Add CHANGELOG.md and maintain it from the first release?

**Decision:** **No for now.** Skip CHANGELOG.md until formal releases; add then if desired.

---

## 11. Out-of-scope list

**Where:** PROJECT_KICKOFF.md — "Out-of-scope list agreed (see PRODUCT.md; adjust if needed)"

**Question:** Is the current out-of-scope list in PRODUCT.md final, or do you want to add/remove items?

**Decision:** **Final as-is.** Multi-tenant SaaS, model training/fine-tuning, and full CI/CD pipeline remain out of scope per PRODUCT.md. No additions or removals.

---

## 12. Success criteria for first milestone

**Where:** PROJECT_KICKOFF.md — "Success criteria for first milestone agreed (see PRODUCT.md)"

**Question:** Are the success criteria in PRODUCT.md (§ Success Criteria) what you want for the first milestone, or change them?

**Decision:** **Approved as-is.** PRODUCT.md success criteria (one control plane + worker, chat → commit/branch; second worker; session attach CLI↔UI; tail logs both; one loop workflow end-to-end) stand as the first milestone.

---

## 13. Worker heartbeat interval and stale threshold

**Where:** ARCHITECTURE §3, DOC_CLARIFICATION_NEEDED §2.2

**Decision:** **Heartbeat interval:** Worker-configured (e.g. env or config; typical 30s). Worker sends `POST /workers/:id/heartbeat` at that interval. **Stale threshold:** Server-configured (e.g. server config; typical 90s or 3× heartbeat interval). If no heartbeat received for that duration, control plane marks the worker stale. Stale workers are not assigned new tasks. Document both in server and worker config/README.

---

## 14. Job granularity for loop workflows

**Where:** ARCHITECTURE §4, DOC_CLARIFICATION_NEEDED §2.3

**Decision:** **One job per loop iteration.** For "loop N times" and "loop until sentinel," the engine creates one **job** per iteration (so N jobs for loop_n, variable for loop_until_sentinel). Each job is one task pulled by a worker; logs and status are per job. Session aggregates jobs. DB and API already reflect this (job_id per task).

---

## 15. Multi-turn chat in v1

**Where:** PRODUCT W1, ARCHITECTURE §4, DOC_CLARIFICATION_NEEDED §2.3

**Decision:** **In scope for v1.** Chat workflow supports multi-turn in one session: user can send follow-up messages via `POST /sessions/:id/input`. Single-turn is a special case (one prompt, one response).

---

## 16. BYOL: OAuth fallback and token refresh

**Where:** PRODUCT BYOL, DOC_CLARIFICATION_NEEDED §1, §2.5

**Decision:** **OAuth first; fallback to pasted token.** v1: Prefer OAuth with Claude Code / Cursor when the provider offers it. If a provider does not offer OAuth (or for dev/testing), the user can paste a token in the Web UI or CLI; control plane stores it and passes it to workers per job. **Token refresh:** Control plane refreshes stored tokens when the provider supports refresh (e.g. OAuth refresh token). Policy: refresh proactively shortly before expiry if we have expiry info; otherwise refresh on use when the worker or API gets a 401 from the provider. Document in PRODUCT or TECH_STACK.

---

## 17. Config precedence (CLI and worker)

**Where:** TECH_STACK §6, DOC_CLARIFICATION_NEEDED §2.6

**Decision:** **CLI and worker:** Precedence is **CLI flag (when applicable) > env var > config file.** So: explicit flag overrides env, env overrides config file. Document in CLI help and worker README (e.g. "API key: flag `--api-key` > env `REMOTE_HARNESS_API_KEY` > config file `api_key`").

---

## 18. Web UI: storage for URL and API key

**Where:** DOC_CLARIFICATION_NEEDED §2.6

**Decision:** **localStorage.** Control plane URL and API key are stored in the browser’s **localStorage** so they persist across tabs and sessions. Logout (or "Clear credentials") clears them. Document in Web UI and README.

---

## 19. Branch naming default and API

**Where:** ARCHITECTURE §9, DOC_CLARIFICATION_NEEDED §1

**Decision:** **Default:** Branch name is derived from **session_id** (e.g. `harness/<short_session_id>` or configurable prefix + short id). **API:** Session create accepts optional `branch_name_prefix` in params (e.g. `harness/refactor-`); if present, branch = prefix + short session or task id. PR/MR mode uses this branch. See API_OVERVIEW §4.

---

## 20. Log storage rule (v1)

**Where:** TECH_STACK §1, DOC_CLARIFICATION_NEEDED E2

**Decision:** **v1: Dual-write only.** (1) **Local files** on each component (control plane writes to disk on backend; each worker writes to disk on that worker). (2) **Central store = PostgreSQL** on the control plane for all logs that reach it (worker logs via POST + control plane’s own logs). No "files vs DB" choice for the central store in v1—it’s always DB. File rotation for local files is implementation-defined (e.g. size or time-based); document in server/worker config.

---

## 21. Phase 1 first workflow

**Where:** PROJECT_KICKOFF §5, §7, DOC_CLARIFICATION_NEEDED E1

**Decision:** **Chat.** The first workflow to implement end-to-end in Phase 1 is **chat** (one session, one run, optional multi-turn). "Run-once" is the same as chat with a single turn.

---

## 22. Wake config: precedence and CLI script

**Where:** HOSTING §4, DOC_CLARIFICATION_NEEDED §1, E3

**Decision:** **Precedence:** If both **WAKE_URL** and **WAKE_SCRIPT** are set, **WAKE_URL wins.** Only the wake URL is invoked (HTTP request). **CLI "run script":** When only WAKE_SCRIPT is set, the CLI runs the script at the given path **on the machine where the CLI is running** (local script). The script is executed by the CLI process (e.g. spawn); no remote execution. Document in HOSTING and CLI help.

---

## 23. Personas (agent identity / pre-configured prompts)

**Where:** Product request; loops and inboxes need distinct agent identities.

**Decision:** **Personas** are user-provided, pre-configured prompts stored in the control plane (name + prompt text). They give agents **separate personas** (e.g. "Refactorer", "Reviewer"). At **every** agent invocation—chat, loop iteration, inbox task, or any other path—the control plane resolves the chosen persona (when specified), combines its prompt with the task-specific information (repo, user message, inbox payload, etc.), and provides that combined context to the worker. The worker invokes the Claude Code or Cursor CLI with it so the agent runs with the right identity and task. **API:** Personas have CRUD (POST/GET /personas, GET /personas/:id; optional PATCH/DELETE). Session create and inbox enqueue accept optional **persona_id**. Task payload to worker includes the resolved prompt (persona + task) so the worker can pass it to the CLI. See [Product W6](PRODUCT.md), [Architecture §4b](ARCHITECTURE.md), [API_OVERVIEW §4, §5a, §8](API_OVERVIEW.md).

---

## 24. Logs interface: full history then stream

**Where:** User requirement — logs view should always be consistent and complete.

**Decision:** Wherever a user views logs (session, job, or any context), the **full history** for that context must be **loaded and rendered first**, then the live stream is attached. So: (1) client fetches existing logs (e.g. `GET /sessions/:id/logs`, paginated until complete or sufficient), (2) client renders them, (3) client subscribes to the stream (`GET /sessions/:id/logs/stream`) so new logs append. The user always sees the complete backlog before any streamed entries. Same behavior in CLI and Web UI. See [Architecture §6](ARCHITECTURE.md), [API_OVERVIEW §6](API_OVERVIEW.md).

---

## 28. Log history: load all, no cap

**Where:** OPEN_POINTS_AND_DECISIONS §2.3 — how much history to load before streaming.

**Decision:** The client loads **all** log history for the context (session or job). **No cap.** Paginate `GET /sessions/:id/logs` until there is no `next_cursor`; then render and attach the stream. See [API_OVERVIEW §6](API_OVERVIEW.md), [Architecture §6](ARCHITECTURE.md).

---

## 29. Platform affinity (v1)

**Where:** OPEN_POINTS_AND_DECISIONS §2.4 — does the session or task ever request a platform?

**Decision:** **v1: no platform affinity.** The engine assigns tasks to **any available worker**. Workers still advertise `platform` in labels for observability (filtering, display in the UI). The control plane does not prefer or require a matching platform when assigning work. Add platform affinity later (e.g. `preferred_platform` on session create) if needed. See [Architecture §4c](ARCHITECTURE.md), [API_OVERVIEW §9](API_OVERVIEW.md) (Register).

---

## 25. Platform-specific workers (CLI invocation)

**Where:** User requirement — CLIs operate differently on Windows, WSL, macOS, Linux; workers must handle these differences.

**Decision:** Workers are **platform-specific** (or have platform-specific modules). We support **Windows** (native and WSL), **macOS**, and **Linux**. The Claude Code and Cursor CLIs behave differently on each platform—invocation, argument passing, process model, output streaming. **Windows in particular** requires dedicated handling (process creation, quoting, console/PTY, stdout/stderr). Each platform must have **specific handling** for: (1) **discovering and invoking the CLI** (where it is, how to spawn it), (2) **passing arguments in** (quoting, stdin vs argv; Windows differs from Unix), (3) **streaming results out** (capturing and sending stdout/stderr to the control plane). Implement one worker binary/variant per platform (Windows native, WSL, macOS, Linux) or a single codebase with clear per-platform modules for CLI interaction. The control plane is unchanged; workers advertise themselves and get tasks regardless of platform. See [Architecture §4c](ARCHITECTURE.md), [Tech Stack §2](TECH_STACK.md#2-workers-rust), [Product F2](PRODUCT.md).

---

## 26. Task payload to worker: two parts (prompt_context + task_input)

**Where:** OPEN_POINTS_AND_DECISIONS §2.1 — how persona + task is represented in the task payload.

**Decision:** The control plane sends **two parts** in the task payload to the worker: **prompt_context** (persona prompt text, or omitted/empty when no persona) and **task_input** (task-specific input: user message, loop prompt, inbox payload, etc.). The worker passes prompt_context as the agent's context (e.g. system prompt) and task_input as the user/task input to the CLI. See [API_OVERVIEW §9](API_OVERVIEW.md) (Pull task).

---

## 27. Session create: default for ref

**Where:** OPEN_POINTS_AND_DECISIONS §2.2 — default when client omits `ref`.

**Decision:** If the client omits **ref** in session create, the server uses **main** as the default (branch to clone). See [API_OVERVIEW §4](API_OVERVIEW.md).
