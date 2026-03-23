# ISSUES LOG — Unimplemented Features

Cross-referenced against `docs/` spec. One line per gap.

| # | Area | Missing Feature | Spec Source |
|---|------|----------------|-------------|
| 1 | API | `POST /sessions/{id}/inbox` and `GET /sessions/{id}/inbox` endpoints not implemented — inbox only exists as a workflow type, no dedicated REST resource | API_OVERVIEW §Inboxes |
| 2 | API | No OpenAPI 3.x spec file checked into the repo; contracts exist only as markdown | API_OVERVIEW §5, CICD_DESIGN |
| 3 | Web UI | No first-time setup wizard (control-plane URL → API key → bootstrap → credentials flow) | CLIENT_EXPERIENCE §First-time setup |
| 4 | Web UI | No log retention/purge controls in UI (server has retention logic but no user-facing config or manual purge) | CLIENT_EXPERIENCE §Log retention and purge UX |
| 5 | Web UI | No worker pool heterogeneity warnings when mixed OS/platform workers are registered | CLIENT_EXPERIENCE §Worker pool heterogeneity, PRODUCT O4 |
| 6 | Web UI | No worker delete button in the workers page (DELETE endpoint exists server-side) | CLIENT_EXPERIENCE |
| 7 | Logging | No keyword/full-text search on log messages — only level and job_id filters exist | API_OVERVIEW §Logs, CLIENT_EXPERIENCE |
| 8 | Identity | `GET /identities/{id}/auth-status` checks stored expiry timestamp but does not validate tokens against GitHub/GitLab API | API_OVERVIEW §Identities, CLIENT_EXPERIENCE §Credentials |
| 9 | Git | Branch naming uses `harness/{short_id}` instead of documented `rh/{session_id}/{short_slug}` convention | ARCHITECTURE §Git integration |
| 10 | Git | PR base branch is hardcoded to `"main"` — no support for configurable default branch | ARCHITECTURE §Git integration |
| 11 | Workflow | No standalone "planning mode" workflow (only `BranchMode::Pr` exists as a branch strategy, not a separate execution mode) | PRODUCT, ARCHITECTURE §Personas/Sentinel |
| 12 | CI/CD | No OpenAPI parity check in CI pipeline (spec drift detection mentioned in docs but not wired) | CICD_DESIGN, API_OVERVIEW §5 |
| 13 | Web UI | No compatibility/version-upgrade warning banner when worker and control-plane versions diverge | CLIENT_EXPERIENCE §Compatibility and upgrades |
| 14 | Web UI | No long-session history truncation indicator shown to user when chat history is trimmed | CLIENT_EXPERIENCE §Long chat sessions |
| 15 | Hosting | Wake integration is CLI-only — no server-side or Web UI trigger to wake sleeping backend/workers | HOSTING §Wake integration, CLIENT_EXPERIENCE |
| 16 | Got confused between job id and task id in a few places |
| 17 | Didn't bundle the agent CLIs with the docker container |
| 18 | Didn't set yolo mode on the agents |
| 19 | Stream key not working for streaming endpoint |
| 20 | Created a git submodule for web? lol |