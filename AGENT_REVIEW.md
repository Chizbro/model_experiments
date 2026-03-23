# Agent log review (`logs/`)

This document summarizes a read-through of all task dumps under `logs/` (28 files). Those files are transcripts from agents that executed numbered plan tasks. The review focused on whether anything **weird** occurred: wasted or duplicated context, unfixed tests, missing tests, or unfinished features left unacknowledged.

---

## Executive summary

The logs reflect a **mostly disciplined build-out**. Tasks generally end with formatting, clippy, and `cargo test` (and later Vitest for the web). Several write-ups document **real bugs found and fixed** (e.g. compose smoke exit criteria, worker git test env leakage).

The dominant anomaly is **process-shaped**, not “silent sabotage”: a standing instruction to dump the **entire** model context produced **very large, redundant log files**, often embedding **full source files** already versioned in git. There is also **repeated rediscovery** of the same plan-file glob quirk, and **stale absolute paths** (`remote_harness_3` vs the current repo folder name).

---

## 1. Log volume and redundant context

Approximate line counts (useful for scale):

| Log file | Lines (approx.) | Note |
|----------|-----------------|------|
| `24-web-logs-sse-attach.log` | ~6,900 | Full “window dump” with large pasted sources |
| `25-web-ux-spec-checkpoints.log` | ~4,600 | Same pattern |
| `06-server-api-keys-bootstrap.log` | ~2,700 | Long code blocks |
| `13-server-loop-workflows.log` | ~2,200 | Good summary at top, then many lines of pasted `config.rs` / migration |
| `22-web-shell-bootstrap.log` | ~2,100 | Many files pasted in full under `.../remote_harness_3/...` |

Smaller logs (e.g. `21-cli-full-api-surface.log` ~180 lines, `26-e2e-compose-smoke.log` ~45 lines) are **much higher signal per kilobyte**.

**Cause:** Repeated user text in the dumps: do not summarize—dump the **entire contents window** (including the typo **“CONTENTX”**). That led to:

- **Duplication** of the same sources across tasks (e.g. `api-types` / `App.tsx`-scale content in both **24** and **25**).
- **Repeated boilerplate** (the same orchestration instructions at the top of nearly every file).
- **Low auditability** for humans: thousands of lines to scroll for a short narrative of what actually changed.

**Verdict:** Dumping whole `main.rs` / `App.tsx` / OpenAPI fragments is **not necessary** for audit if the repo is canonical; a **commit-ish summary** (files touched, commands, test tail) would suffice.

---

## 2. Repeated operational snag: pending task globs

Many logs document the same discovery:

- Glob **`*.pending.md`** returned **0** files; the real naming pattern is **`*-pending.md`**.

This appears across multiple tasks (e.g. **06, 21, 23, 24, 27**). Each agent paid the same discovery cost. **Fix once** in the orchestrator prompt (or list `plan/` explicitly) instead of per-agent.

---

## 3. Stale or inconsistent workspace paths

Several logs embed paths under:

`/Users/chizbro/Desktop/code/utils/remote_harness_3/...`

which may not match the current checkout (e.g. **`remote_harness_composer_2`**). Examples show up in **22**, **23** (Vitest path), **19**, **06** metadata, etc.

**Verdict:** Likely **rename, copy, or parallel harness directories**. It does not by itself prove wrong code in the current tree, but it **hurts log-based forensics** if paths are taken literally.

---

## 4. Testing and “broken tests”

**Failures fixed, not left broken:**

- **17-worker-git-ops-spec.log** describes **five failing tests** (env leakage, SSH URL parsing), then **documents fixes**.

**Green outcomes claimed:**

- Many tasks state **`cargo test --workspace`** (or equivalent) **passed** after changes (**06, 13, 14, 19, 21, 23**, etc.).
- **23** records Vitest **19 tests passed** for that slice.

**Early placeholder web scripts:**

- **01** / **03** show `web` `lint` / `typecheck` as **`echo ... && exit 0`** before a real UI existed. That is **bootstrap scaffolding**, not hidden CI failure.

**Heavy mocking (documented):**

- **23** notes **`SessionCreatePage.test.tsx`** stubs `fetch` and `window.confirm`. Reasonable for unit tests; **not** E2E. The same log says **Playwright E2E was optional**.

---

## 5. Unfinished work: deferrals vs gaps

**Intentional / spec-driven deferral:**

- **27-phase2-personas-inboxes-design.log**: **Design only**; implementation deferred to future tasks.
- **21-cli-full-api-surface.log**: **Inbox** CLI/workflow deferred to later backlog (**27** area).
- **25-web-ux-spec-checkpoints.log**: **Changelog** deferred until formal releases.
- **17-worker-git-ops-spec.log**: **Optional network integration** skipped; unit tests treated as sufficient for the task.

**Documented follow-ups (UX/product, not silent drops):**

- **23-web-sessions-workers.log** §11: SSE/logs/attach called out for **plan 24** (addressed in **24**’s completed notes); **Playwright** optional; OAuth still **playground**-heavy vs dedicated Settings—consistent with **`ISSUE_LOG.md`**.

**Strong incident write-up:**

- **26-e2e-compose-smoke.log**: Compose smoke appeared to “hang” because the script waited for `session.status == "completed"` for **chat**, while the server **leaves chat sessions `running`** after a completed job by design. **Root cause and fix** are clearly explained—good template for future logs.

---

## 6. Minor oddities

- **06-server-api-keys-bootstrap.log**: The quoted user block **duplicates** “Make sure any migrations you added work” **twice**—**template copy-paste** propagated into dumps.
- **`ISSUE_LOG.md`** states there is **no dotenvy**; the **server** crate **does** use **dotenvy** (`crates/server/src/main.rs`). Treat **`ISSUE_LOG.md`** as **notes**: verify against the repo before relying on it.

---

## Recommendations (orchestration / hygiene)

1. **Change the dump rule** to: files touched, command list, short test output, link to `plan/*-complete.md`—**not** full file bodies.
2. **Fix the pending-task discovery** string once (`*-pending.md` or explicit listing).
3. **Align workspace path** in agent instructions so dumps do not embed obsolete absolute paths.
4. Use **26-style** logs as the norm: **short, causal, verifiable**.

---

## Bottom line

The `logs/` directory does **not** show a pattern of agents **merging with broken tests** or **dropping features without a paper trail**. The main issues are **self-inflicted**: **oversized redundant dumps**, **repeated glob confusion**, **stale paths**, and **explicit deferrals** (Phase 2 design, inbox, Playwright, changelog) that match the **plan** and **`ISSUE_LOG.md`** rather than unexplained failure.
