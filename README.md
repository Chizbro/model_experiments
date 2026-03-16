# Remote Harness

A **self-hosted, single-tenant** service that manages **agentic tasks, loops, workflows, and environments** for software projects (Git repositories). **Bring your own licence (BYOL):** users sign in with Claude Code or Cursor; workers run the Claude Code or Cursor CLI with that token—no platform-owned model APIs. Workers clone repos, run workflows, and optionally commit and push (main or PR/MR branch). Control everything from a **CLI** or **Web UI**, with full logging and the ability to attach to any session from either interface.

## Status

**Early stage.** This repo contains project kickoff documentation; implementation is in progress.

## Documentation

| Document | Description |
|----------|-------------|
| [**docs/ARCHITECTURE.md**](docs/ARCHITECTURE.md) | System architecture, component diagram, worker discovery, task flow, session attach, logging, agent inboxes. |
| [**docs/TECH_STACK.md**](docs/TECH_STACK.md) | Recommended tech stack: control plane, workers, CLI, Web UI, logging, auth. |
| [**docs/PRODUCT.md**](docs/PRODUCT.md) | Product vision, description, feature list (with priorities), success criteria, glossary. |
| [**docs/PROJECT_KICKOFF.md**](docs/PROJECT_KICKOFF.md) | Kickoff checklist, risks, phases/milestones, next actions. |
| [**docs/API_OVERVIEW.md**](docs/API_OVERVIEW.md) | REST and WebSocket API sketch (to refine when implementing). |
| [**docs/CICD_DESIGN.md**](docs/CICD_DESIGN.md) | CI/CD design (triggers, jobs); CI platform and git host TBD. |
| [**docs/HOSTING.md**](docs/HOSTING.md) | Hosting flexibility: always-on vs sleepable topologies, optional wake integration (e.g. WOL), platform support (Windows/WSL, macOS, Linux). |
| [**docs/DOC_REVIEW.md**](docs/DOC_REVIEW.md) | Review of uncertain/undecided items in the docs; use to lock the design before building. |

## Quick idea summary

- **Control plane** (Rust): API (REST + WebSocket), task/workflow engine, session store, worker registry, log aggregation.
- **Workers** (Rust): Auto-discover and register; clone repo, run agent, commit/push; report logs and status.
- **Workflows**: Chat, loop N times, loop until sentinel, continuous inbox agents, spawn tasks to other agents’ inboxes.
- **Interfaces**: CLI (any machine) and Web UI; attach to the same session from either; tail logs from both.
- **Logging**: Structured, centralized; tail from CLI and UI.

## How to run (when implemented)

*(To be filled in as the stack is built.)*

- **Control plane**: `cargo run -p server` with DB and optional Redis.
- **Worker**: `cargo run -p worker` with `CONTROL_PLANE_URL` and auth.
- **CLI**: `cargo run -p cli` (or install `remote-harness`) — config via env or `~/.config/remote-harness/`.
- **Web UI**: `cd web && npm run dev` for development.

## License

MIT.
