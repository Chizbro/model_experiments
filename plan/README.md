# Implementation plan

Tasks are **ordered**, **batchable** (related work grouped to reduce context switching), and each file lists **how to verify** the work immediately and how to **re-run** checks later.

## How to use

1. Work through tasks in numeric order unless a task’s **Dependencies** section says otherwise.
2. After completing a task, rename the file: replace `-pending` with `-done` (e.g. `05-database-schema-and-migrations-done.md`) and optionally add a short “Completed / Notes” section at the bottom.
3. If scope changes, update the task file and the **spec** in `docs/` per [AGENTS.md](../AGENTS.md).

## Canonical specs

| Topic | Document |
|--------|-----------|
| Features & priorities | [docs/PRODUCT.md](../docs/PRODUCT.md) |
| REST + SSE + worker pull | [docs/API_OVERVIEW.md](../docs/API_OVERVIEW.md) |
| UX (errors, SSE, bootstrap, retention) | [docs/CLIENT_EXPERIENCE.md](../docs/CLIENT_EXPERIENCE.md) |
| Workers, reclaim, Git §9, logging | [docs/ARCHITECTURE.md](../docs/ARCHITECTURE.md) |
| Worker HTTPS Git / libgit2 | [docs/GIT_CLONE_SPEC.md](../docs/GIT_CLONE_SPEC.md) |
| CI expectations | [docs/CICD_DESIGN.md](../docs/CICD_DESIGN.md) |
| Stack & repo layout | [docs/TECH_STACK.md](../docs/TECH_STACK.md) |
| Crate boundaries & tests | [docs/IMPLEMENTATION_BOUNDARIES.md](../docs/IMPLEMENTATION_BOUNDARIES.md) |
| Phases & §6a checkpoints | [docs/PROJECT_KICKOFF.md](../docs/PROJECT_KICKOFF.md) |

## Task index

| # | File | Summary |
|---|------|---------|
| 00 | [00-upfront-design-boundaries-complete.md](00-upfront-design-boundaries-complete.md) | Crate boundaries, config, test layers |
| 01 | [01-monorepo-docker-tooling-complete.md](01-monorepo-docker-tooling-complete.md) | Workspace, Compose, images |
| 02 | [02-api-types-openapi-baseline-complete.md](02-api-types-openapi-baseline-complete.md) | Shared types + OpenAPI artifact |
| 03 | [03-ci-rust-web-openapi-complete.md](03-ci-rust-web-openapi-complete.md) | Pipeline + drift checks |
| 04 | [04-server-health-config-complete.md](04-server-health-config-complete.md) | Health/ready/idle, config |
| 05 | [05-database-migrations-complete.md](05-database-migrations-complete.md) | Schema, SQLx migrations |
| 06 | [06-server-api-keys-bootstrap-complete.md](06-server-api-keys-bootstrap-complete.md) | Auth + bootstrap |
| 07 | [07-server-identities-byol-complete.md](07-server-identities-byol-complete.md) | Identities CRUD/status/repos |
| 08 | [08-server-oauth-git-complete.md](08-server-oauth-git-complete.md) | GitHub/GitLab OAuth |
| 09 | [09-server-worker-registry-versioning-complete.md](09-server-worker-registry-versioning-complete.md) | Register, heartbeat, `worker_version_incompatible` |
| 10 | [10-server-queue-reclaim-lease-complete.md](10-server-queue-reclaim-lease-complete.md) | Pull, reclaim, lease |
| 11 | [11-server-sessions-chat-engine-complete.md](11-server-sessions-chat-engine-complete.md) | Sessions API + chat jobs |
| 12 | [12-server-chat-history-cap-complete.md](12-server-chat-history-cap-complete.md) | `history_truncated`, caps |
| 13 | [13-server-loop-workflows-complete.md](13-server-loop-workflows-complete.md) | `loop_n`, literal sentinel |
| 14 | [14-server-logs-retention-complete.md](14-server-logs-retention-complete.md) | Ingest, pagination, purge |
| 15 | [15-server-sse-streams-complete.md](15-server-sse-streams-complete.md) | Log + session event SSE |
| 16 | [16-worker-http-lifecycle-complete.md](16-worker-http-lifecycle-complete.md) | Register, poll, heartbeat |
| 17 | [17-worker-git-ops-spec-complete.md](17-worker-git-ops-spec-complete.md) | GIT_CLONE_SPEC compliance |
| 18 | [18-worker-agent-cli-platform-complete.md](18-worker-agent-cli-platform-complete.md) | Claude/Cursor per OS |
| 19 | [19-worker-task-execution-complete.md](19-worker-task-execution-complete.md) | Full job loop + complete/logs |
| 20 | [20-cli-core-complete.md](20-cli-core-complete.md) | Config, human errors, no `--json` |
| 21 | [21-cli-full-api-surface-complete.md](21-cli-full-api-surface-complete.md) | Sessions, workers, logs, credentials, keys |
| 22 | [22-web-shell-bootstrap-complete.md](22-web-shell-bootstrap-complete.md) | SPA, URL/key, OAuth entry |
| 23 | [23-web-sessions-workers-complete.md](23-web-sessions-workers-complete.md) | Lists + detail |
| 24 | [24-web-logs-sse-attach-complete.md](24-web-logs-sse-attach-complete.md) | History + SSE + attach |
| 25 | [25-web-ux-spec-checkpoints-complete.md](25-web-ux-spec-checkpoints-complete.md) | Outcomes, banner, retention copy |
| 26 | [26-e2e-compose-smoke-complete.md](26-e2e-compose-smoke-complete.md) | Docker smoke / integration |
| 27 | [27-phase2-personas-inboxes-design-complete.md](27-phase2-personas-inboxes-design-complete.md) | P1 design backlog |
