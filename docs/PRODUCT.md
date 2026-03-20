# Product Description & Feature List

## Product Vision

A **remote harness** for agentic tasks: a single control plane that manages workflows, loops, and environments tied to Git repositories, with a pool of workers that run on your own devices. You add workers by running a binary and pointing it at the server—no heavy reconfiguration. You drive everything from a **CLI** on any machine or a **Web UI**, with full logging and the ability to attach to any session from either interface.

---

## Product Description (Elevator Pitch)

**Remote Harness** is a **self-hosted, single-tenant** service that runs and orchestrates AI agent workflows over your software repositories. It is not multi-tenant: one deployment serves one organization or team; you run it on your own infrastructure. **Bring your own licence (BYOL):** you run **Claude Code** or **Cursor** on workers under *your* subscription; the control plane stores the **agent** and **Git** credentials you provide (OAuth for GitHub/GitLab where configured, plus agent keys/tokens for the CLI—see [BYOL below](#bring-your-own-licence-byol)). The worker invokes those CLIs in the clone; they perform the main agent work. You define workflows (chat, fixed loops, loop-until-done, or continuous inbox-based agents). Workers clone repos, run the chosen CLI, and optionally commit and push (to main or to a branch for PR/MR). Workers register themselves automatically. You manage sessions, tail logs, and attach to live runs from a CLI or a web dashboard—and you can start a session in the UI and attach to it from the CLI (or the other way around).

---

## User Personas

| Persona | Goal |
|---------|------|
| **Developer** | Run one-off or repeated agent tasks on a repo (e.g. “refactor this module” N times or until “DONE”), then review commits or PRs. |
| **DevOps / Platform** | Add or remove worker machines without touching server config; monitor workers and sessions. |
| **Team lead** | Use the Web UI to see active sessions, logs, and agent inboxes; hand off or inspect work. |

---

## Feature List

### Core Platform

| ID | Feature | Description | Priority |
|----|---------|-------------|----------|
| F1 | **Control plane server** | Single deployable service: API (**REST** + **SSE** for logs and session events in v1), task/workflow engine, session store, worker registry, log aggregation. | P0 |
| F2 | **Worker pool** | One or more worker processes that pull (or receive) tasks, clone repos, run agent logic, report status and logs. Workers are **platform-specific**: we support Windows (native and WSL), macOS, and Linux, and the agent CLIs behave differently on each. Each platform has its own worker handling for invoking the CLI, passing arguments in, and streaming results out—Windows in particular needs dedicated handling. See [Architecture §4c](ARCHITECTURE.md). **v1 dispatch does not match tasks to worker OS or installed CLI**—a mixed pool can assign work to a worker that cannot run the session’s `agent_cli`. Operators must run a **homogeneous** pool (same OS family and same CLI available) per control plane, or use separate deployments; the **Web UI and CLI must surface warnings** when registered workers disagree on `platform` or would likely conflict with session `agent_cli`. See [CLIENT_EXPERIENCE.md — Worker pool](CLIENT_EXPERIENCE.md#10-worker-pool-heterogeneity-warnings). | P0 |
| F3 | **Worker auto-discovery / registration** | Workers register with the control plane on startup and send periodic heartbeats; control plane marks workers stale if heartbeats stop. New workers usable without server reconfiguration. | P0 |
| F4 | **Git integration** | Workers clone a given repo (URL + ref); run tasks in that clone; commit and push to main or to a named branch (PR/MR mode). | P0 |
| F5 | **Repository-scoped tasks** | Every task is associated with a Git repository (and optionally branch/ref). | P0 |

### Workflows

| ID | Feature | Description | Priority |
|----|---------|-------------|----------|
| W1 | **Chat (single / multi-turn)** | One session: user sends messages, agent responds; multi-turn in v1 via `POST /sessions/:id/input`. No fixed loop. | P0 |
| W2 | **Loop N times** | Run the same prompt/workflow exactly N times (e.g. “suggest 5 refactors”). | P0 |
| W3 | **Loop until sentinel** | Run the same prompt until the agent output contains a configured **literal substring** sentinel (e.g. `DONE`). **v1:** substring match only (no regex). **Later (P1):** optional regex mode—see [API_OVERVIEW](API_OVERVIEW.md#create-session-start-workflow) and [Glossary — Sentinel](#glossary). | P0 |
| W4 | **Continuous inbox agent** | Long-lived agent that monitors an inbox; processes tasks as they arrive. | P1 |
| W5 | **Spawn task to another agent’s inbox** | From one workflow/agent, enqueue a task to another agent’s inbox (cross-agent tasks). | P1 |
| W6 | **Personas** | User-defined, pre-configured prompts (e.g. Refactorer, Reviewer). When an agent is invoked (chat, loop, inbox, or any path), the chosen persona prompt is provided with task-specific information (repo, user message, inbox payload). Control plane stores personas and resolves at invocation time. See [Architecture §4b](ARCHITECTURE.md). | P1 |

### Interfaces

| ID | Feature | Description | Priority |
|----|---------|-------------|----------|
| I1 | **CLI** | Full management from the command line: start sessions, list workers, tail logs, attach to a session. Works from any client machine. | P0 |
| I2 | **Web UI** | Dashboard: sessions, workers, logs; start sessions; view and attach to sessions; tail logs. | P0 |
| I3 | **Session attach from either interface** | Start a session in the Web UI → attach to it from the CLI (and vice versa). Same session ID, same log stream and state. | P0 |
| I4 | **Clear Git and PR/MR outcomes** | Session and job views explain **why** a commit, push, or PR/MR might be missing—using `error_message`, `status`, `params.branch_mode`, and `pull_request_url`—so users are not left assuming a silent bug. Spec: [CLIENT_EXPERIENCE §8](CLIENT_EXPERIENCE.md#8-git-commit-push-and-prmr-outcomes); behavior model: [Architecture §9a–9b](ARCHITECTURE.md#9a-when-the-worker-attempts-commit-and-push). | P0 |

### Logging & Observability

| ID | Feature | Description | Priority |
|----|---------|-------------|----------|
| L1 | **Structured logging** | All components emit structured logs (e.g. JSON) with session_id, job_id, worker_id, level, message, timestamp. | P0 |
| L2 | **Central log aggregation** | Workers send logs to the control plane; control plane writes its own and ingested worker logs to the central store (DB). All logs go to disk (local files on each component); dual-write so logs are also in the central store for CLI/UI. If streaming or a client breaks, logs are findable on disk. See [Architecture §6](ARCHITECTURE.md#6-logging-architecture). | P0 |
| L3 | **Tail logs from CLI** | e.g. `logs tail --session-id <id>` (and optionally `--job-id`, `--level`). Full history for the context is loaded and rendered first, then logs stream in real time. | P0 |
| L4 | **Tail logs from Web UI** | Session (and job) detail views include a log panel that loads full history for that context first, then streams. Same consistent, complete behavior as CLI. | P0 |
| L5 | **Log retention and search** | Default: **7 days** (configurable in server config). Override: mark session/job **retain forever**. Manual delete: any logs deletable via CLI or UI at any time. Users must be told before logs age out—the **Web UI** shows default retention and remaining time (or expiry) in **Settings** (or equivalent), and session detail copy when logs are subject to purge ([CLIENT_EXPERIENCE.md — Log retention](CLIENT_EXPERIENCE.md#9-log-retention-and-purge)). **Search/filter** in UI and CLI is **P1** (feature can ship after retention/purge UX). | P0 |

### Optional / Later

| ID | Feature | Description | Priority |
|----|---------|-------------|----------|
| O2 | **PR/MR creation** | In PR/MR mode, after push, call GitHub/GitLab API to open a Pull/Merge Request. | P1 |
| O3 | **Secrets management** | Store Git credentials in control plane; inject per job to workers. Agent execution uses user’s Claude Code / Cursor token (BYOL). | P1 |
| O4 | **Labels / capability-based dispatch** | Workers advertise labels (e.g. `gpu=true`); engine assigns jobs to matching workers. | P1 |
| O5 | **Wake integration (power-saving)** | When control plane is unreachable, UI/CLI can show “may be sleeping” and a configurable “Wake up” action (URL or script) so deployers can trigger WOL or similar. See [Hosting](HOSTING.md). | P2 |

---

## Success Criteria (Early Phase)

- [ ] One control plane + one worker: run a “chat” workflow on a repo and see one commit (or branch) created.
- [ ] Second worker joins without changing server config; tasks can be handled by either worker.
- [ ] Start a session from the Web UI; run `remote-harness attach <session_id>` from the CLI and see the same live logs.
- [ ] Tail logs for a session from both CLI and Web UI.
- [ ] At least one loop workflow (N times or until sentinel) runs end-to-end with logs and Git output.

---

## Bring your own licence (BYOL)

The platform **does not replace** your Claude / Cursor subscription: the **worker** runs the **Claude Code** or **Cursor** CLI in the clone; that CLI performs the main agent work. There is no platform-owned model API licence.

**Credentials in v1 (explicit, so setup is predictable):**

| Kind | How users connect |
|------|-------------------|
| **Git (GitHub / GitLab)** | **OAuth** in the Web UI when the server is configured (`/auth/github`, `/auth/gitlab`), or paste **PAT**/token via UI / CLI / `PATCH /identities/:id`. The control plane stores and refreshes OAuth tokens when the provider allows. |
| **Agent (Claude Code / Cursor)** | User provides the **agent token or API key** via Web UI **Settings**, CLI **`credentials set`**, or `PATCH /identities/:id`. The control plane stores it and passes it to the worker per task. **Provider OAuth for the agent CLI** may be added when we standardize on stable browser/CLI flows for both vendors; until then, documented paths are token-based so installs are reproducible. |

The **control plane** refreshes tokens when it has refresh metadata (e.g. Git OAuth); agent keys follow vendor semantics.

When a task runs, the **worker** receives **job-scoped** `git_token` and `agent_token` in the pull payload ([API_OVERVIEW](API_OVERVIEW.md)) and uses them only for that task.

v1 supports **Claude Code and Cursor** only; no other agent CLIs in scope. **Operator + end-user UX** for errors, SSE, and credentials: [CLIENT_EXPERIENCE.md](CLIENT_EXPERIENCE.md).

---

## Tenancy and deployment

**Single-tenant, self-hosted only.** One deployment of Remote Harness serves one organization or team. There is no multi-tenant SaaS offering: you run the control plane and workers on your own infrastructure (or chosen cloud account). Authentication and authorization are for that single tenant (e.g. team members, API keys); no concept of “customers” or “tenants” sharing the same instance.

---

## Out of Scope (for now)

- **Multi-tenant SaaS** — Not in scope. The product is single-tenant and self-hosted only (see above).
- **Built-in agent model training or fine-tuning** — See [elaboration below](#out-of-scope-elaboration).
- **Full CI/CD pipeline** — See [elaboration below](#out-of-scope-elaboration).

### Out-of-scope elaboration

**Model training / fine-tuning**  
The product is not a hosted model provider. **Primary** agent execution is **Claude Code** or **Cursor** CLIs with the user’s credentials (BYOL); it therefore does **not** include:

- Training new models from scratch.
- Fine-tuning a base model on your data.
- Managing training jobs, datasets, or model versions.
- Any “bring your own model API” beyond the supported CLIs (Claude Code, Cursor).

The harness is a workflow runner that delegates agent execution to those CLIs; it is not a model-training or model-hosting platform.

**Full CI/CD pipeline**  
Remote Harness is focused on **agent-driven tasks on Git repos** (clone → run agent → commit/push, optionally open PR/MR). It does **not** aim to replace or orchestrate a full CI/CD system, such as:

- Running arbitrary Jenkins pipelines, GitHub Actions workflows, or GitLab CI jobs.
- Managing build matrices, deploy stages, or release gates.
- Replacing your existing CI/CD tool; it can sit alongside it (e.g. “run this agent task” as one kind of work).

So: the harness runs agent workflows that touch Git; it is not a general-purpose “run any CI job” or “replace Jenkins/GitLab CI” product. You can still trigger the harness from CI (e.g. “on merge, run this agent workflow”) if you want.

---

## Glossary

| Term | Meaning |
|------|---------|
| **Control plane** | Central server: API, workflow engine, registry, sessions, logs. |
| **Worker** | Process that connects to the control plane, receives tasks, clones repos, runs the Claude Code or Cursor CLI (with user’s token), reports logs and status. |
| **Session** | A user-visible run: one chat, one loop, or one continuous agent. Has a stable ID; can be attached from CLI or Web. |
| **Job** | Internal unit of work (e.g. one loop iteration or one inbox task). A session may have one or many jobs. |
| **Inbox** | Per-agent queue of tasks; continuous agents consume from their inbox. |
| **Persona** | A user-defined, pre-configured prompt (name + prompt text) stored in the control plane. When starting a session or enqueueing to an inbox, the user can attach a persona; at invocation time the control plane combines the persona's prompt with the task-specific information and provides that to the agent (so the agent runs with a consistent "identity" or context). |
| **Sentinel** | In **loop_until_sentinel**, a **literal substring** that must appear in agent output for that iteration to stop the loop (**v1**). **Regex / pattern matching** is **not** in v1; treat as a later enhancement aligned with [API_OVERVIEW](API_OVERVIEW.md). |
| **PR/MR mode** | Worker commits to a branch and (optionally) opens a Pull/Merge Request instead of pushing to main. |
| **BYOL** | Bring your own licence: users supply **agent** credentials (token/API key via UI, CLI, or API—see [BYOL](#bring-your-own-licence-byol)); the worker runs Claude Code or Cursor with that credential. The platform does not replace the vendor subscription or call model APIs for the main agent turn. |

---

*Previous: [Tech Stack](TECH_STACK.md) | [Architecture](ARCHITECTURE.md) | [Client experience](CLIENT_EXPERIENCE.md) | [Project Kickoff](PROJECT_KICKOFF.md)*
