# Product Description & Feature List

## Product Vision

A **remote harness** for agentic tasks: a single control plane that manages workflows, loops, and environments tied to Git repositories, with a pool of workers that run on your own devices. You add workers by running a binary and pointing it at the server—no heavy reconfiguration. You drive everything from a **CLI** on any machine or a **Web UI**, with full logging and the ability to attach to any session from either interface.

---

## Product Description (Elevator Pitch)

**Remote Harness** is a **self-hosted, single-tenant** service that runs and orchestrates AI agent workflows over your software repositories. It is not multi-tenant: one deployment serves one organization or team; you run it on your own infrastructure. **Bring your own licence (BYOL):** the platform does not call any AI models directly. Users sign in with a **Claude Code** or **Cursor** subscription; the platform stores and uses that authenticated token to run the **Claude Code** or **Cursor** CLIs on workers—those CLIs do the actual agent work. You define workflows (chat, fixed loops, loop-until-done, or continuous inbox-based agents). Workers clone repos, run the chosen CLI with your token, and optionally commit and push (to main or to a branch for PR/MR). Workers register themselves automatically. You manage sessions, tail logs, and attach to live runs from a CLI or a web dashboard—and you can start a session in the UI and attach to it from the CLI (or the other way around).

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
| F1 | **Control plane server** | Single deployable service: API (REST + WebSocket), task/workflow engine, session store, worker registry, log aggregation. | P0 |
| F2 | **Worker pool** | One or more worker processes that pull (or receive) tasks, clone repos, run agent logic, report status and logs. Workers are **platform-specific**: we support Windows (native and WSL), macOS, and Linux, and the agent CLIs behave differently on each. Each platform has its own worker handling for invoking the CLI, passing arguments in, and streaming results out—Windows in particular needs dedicated handling. See [Architecture §4c](ARCHITECTURE.md). | P0 |
| F3 | **Worker auto-discovery / registration** | Workers register with the control plane on startup and send periodic heartbeats; control plane marks workers stale if heartbeats stop. New workers usable without server reconfiguration. | P0 |
| F4 | **Git integration** | Workers clone a given repo (URL + ref); run tasks in that clone; commit and push to main or to a named branch (PR/MR mode). | P0 |
| F5 | **Repository-scoped tasks** | Every task is associated with a Git repository (and optionally branch/ref). | P0 |

### Workflows

| ID | Feature | Description | Priority |
|----|---------|-------------|----------|
| W1 | **Chat (single / multi-turn)** | One session: user sends messages, agent responds; multi-turn in v1 via `POST /sessions/:id/input`. No fixed loop. | P0 |
| W2 | **Loop N times** | Run the same prompt/workflow exactly N times (e.g. “suggest 5 refactors”). | P0 |
| W3 | **Loop until sentinel** | Run the same prompt until the agent output contains a configured sentinel value (e.g. “DONE” or a regex). | P0 |
| W4 | **Continuous inbox agent** | Long-lived agent that monitors an inbox; processes tasks as they arrive. | P1 |
| W5 | **Spawn task to another agent’s inbox** | From one workflow/agent, enqueue a task to another agent’s inbox (cross-agent tasks). | P1 |
| W6 | **Personas** | User-defined, pre-configured prompts (e.g. Refactorer, Reviewer). When an agent is invoked (chat, loop, inbox, or any path), the chosen persona prompt is provided with task-specific information (repo, user message, inbox payload). Control plane stores personas and resolves at invocation time. See [Architecture §4b](ARCHITECTURE.md). | P1 |

### Interfaces

| ID | Feature | Description | Priority |
|----|---------|-------------|----------|
| I1 | **CLI** | Full management from the command line: start sessions, list workers, tail logs, attach to a session. Works from any client machine. | P0 |
| I2 | **Web UI** | Dashboard: sessions, workers, logs; start sessions; view and attach to sessions; tail logs. | P0 |
| I3 | **Session attach from either interface** | Start a session in the Web UI → attach to it from the CLI (and vice versa). Same session ID, same log stream and state. | P0 |

### Logging & Observability

| ID | Feature | Description | Priority |
|----|---------|-------------|----------|
| L1 | **Structured logging** | All components emit structured logs (e.g. JSON) with session_id, job_id, worker_id, level, message, timestamp. | P0 |
| L2 | **Central log aggregation** | Workers send logs to the control plane; control plane writes its own and ingested worker logs to the central store (DB). All logs go to disk (local files on each component); dual-write so logs are also in the central store for CLI/UI. If streaming or a client breaks, logs are findable on disk. See [Architecture §6](ARCHITECTURE.md#6-logging-architecture). | P0 |
| L3 | **Tail logs from CLI** | e.g. `logs tail --session-id <id>` (and optionally `--job-id`, `--level`). Full history for the context is loaded and rendered first, then logs stream in real time. | P0 |
| L4 | **Tail logs from Web UI** | Session (and job) detail views include a log panel that loads full history for that context first, then streams. Same consistent, complete behavior as CLI. | P0 |
| L5 | **Log retention and search** | Default: 7 days (configurable in server config). Override: mark session/job "retain forever". Manual delete: any logs deletable via CLI or UI at any time. Search/filter in UI and CLI (P1). | P1 |

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

The platform **does not call any AI models or APIs directly**. There is no platform-owned Claude/OpenAI/etc. licence. Instead:

- **Users sign in** with their **Claude Code** or **Cursor** subscription: **OAuth first** when the provider offers it (Web UI: redirect; CLI: opens browser or device/code flow). **Fallback:** If a provider does not offer OAuth (or for dev/testing), the user can paste a token in the Web UI or CLI; the control plane stores and uses it. See [Decisions §16](DECISIONS.md#16-byol-oauth-fallback-and-token-refresh).
- The **control plane** stores and refreshes tokens: when the provider supports refresh (e.g. OAuth refresh token), refresh proactively before expiry if we have expiry info, otherwise refresh on use when a 401 is received from the provider.
- When a task runs, the **worker** receives the user’s token (or a job-scoped credential) and uses it to authenticate to the **Claude Code** or **Cursor** CLI. The worker runs that CLI in the cloned repo; the CLI does the actual agent work.
- So: you bring your own licence; the harness orchestrates workflows and Git, and delegates execution to Claude Code or Cursor.

v1 supports **Claude Code and Cursor** only; no other agent CLIs in scope.

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
The platform does not call any models or APIs; it runs **Claude Code** or **Cursor** CLIs with the user’s own subscription token (BYOL). It therefore does **not** include:

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
| **Sentinel** | Value or pattern in agent output that signals “loop until” should stop. |
| **PR/MR mode** | Worker commits to a branch and (optionally) opens a Pull/Merge Request instead of pushing to main. |
| **BYOL** | Bring your own licence: users sign in with Claude Code or Cursor; the platform runs those CLIs with the user’s token and does not call any model APIs directly. |

---

*Previous: [Tech Stack](TECH_STACK.md) | [Architecture](ARCHITECTURE.md) | [Project Kickoff](PROJECT_KICKOFF.md)*
