# Issue Log: Implementation Gaps

## Not Implemented

- [ ] **Log auto-purge** — No background task enforces 7-day retention (L5)
- [ ] **OpenAPI spec** — No openapi.yaml checked in; no CI drift check (PROJECT_KICKOFF §6)
- [ ] **CI/CD pipeline** — No workflow/pipeline config exists (CICD_DESIGN)
- [ ] **`npm run typecheck`** — Missing script in web/package.json (CICD_DESIGN §2)
- [ ] **Sleep-inhibit helper** — No script/docs for host-side idle polling (HOSTING §4a)

## Partial

- [ ] **Inbox workflow** — Schema + types exist but no runtime; `create_inbox_jobs()` is a no-op (W4/W5, P1)
- [ ] **Worker `client_version` rejection** — Field accepted but server never validates or rejects incompatible versions (ARCHITECTURE §3)
- [ ] **Chat history truncation warning** — `history_truncated` set in backend but never surfaced in UI or CLI (CLIENT_EXPERIENCE §12)
- [ ] **Credential-specific session create errors** — UI shows generic error instead of directing to Settings → Credentials (CLIENT_EXPERIENCE §5)
- [ ] **Job failure message translation** — Backend sets `[MAX_WORKER_LOSS_RETRIES]` etc. but UI/CLI don't map to human-readable copy (CLIENT_EXPERIENCE §6)
- [ ] **PR/MR absence explanation** — No logic to explain *why* PR is missing on successful job (CLIENT_EXPERIENCE §8)
- [ ] **Bootstrap gating** — Button shown without strict probe-based check for zero keys (CLIENT_EXPERIENCE §7)
- [ ] **CLI worker heterogeneity warning** — Web UI has it, CLI `workers list` does not (CLIENT_EXPERIENCE §10)
- [ ] **Wake integration** — No wake URL config, no "Wake up" button, no `WAKE_SCRIPT` (HOSTING §4, P2)
- [ ] **Server-side log dual-write to disk** — Workers do it; control plane may not (L2)
- [ ] **Per-session retention countdown in UI** — `retain_forever` toggle exists but no expiry timer shown (L5)

## P1 Deferred (tracked, not blocking)

- [ ] Continuous Inbox Agent (W4)
- [ ] Cross-Agent Spawn (W5)
- [ ] Label-based dispatch (O4) — labels stored, dispatch ignores them
- [ ] Log full-text search (L5)
- [ ] Regex sentinel matching (W3)
- [x] Personas (W6) — implemented despite P1 label
- [x] PR/MR creation (O2) — implemented despite P1 label

## Doc Gaps

- [ ] No `SSE_EVENTS.md` (PROJECT_KICKOFF §6)
- [ ] No systemd/launchd unit examples (HOSTING §8)

## Others, found debugged and resolved manually
- Using an old rust version that can't use the dependencies it installed
- Didn't put the agents in the containers
- Committed after failure, didn't return agnet error message to the UI
- Didn't run the agents with print or yolo modes
- Not sending agent logs to the UI, just harness 
- No stream partial output
- Not using agent summarisation for branch and commit names
