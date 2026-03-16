# Doc Review: What’s Still Uncertain or Undecided

Pass over the docs to reach a rock-solid design before building. **Decision 11 (out-of-scope) is done.** Below is everything that still reads as open.

---

## 1. Decisions still TBD in DECISIONS.md

| ID | Item | Affects product design? |
|----|------|--------------------------|
| **8b** | CI platform (GitHub Actions, GitLab CI, etc.) | No — only when you add CI. |
| **8c** | Git host (GitHub, GitLab, etc.) | No — only when you push the repo. |
| **12** | Success criteria for first milestone — approve PRODUCT.md list or change? | Yes — defines “done” for first milestone. |

**Action:** Decide **12** (approve as-is or edit). 8b and 8c can stay “when ready” unless you want to pick them now.

---

## 2. Either/or not yet pinned (design choices)

These are still written as alternatives; locking one per row will remove ambiguity.

| ID | Where | Current wording | Decision needed |
|----|--------|------------------|----------------|
| **B1** | TECH_STACK §6, API_OVERVIEW | Control plane: “API key **or** OIDC”. Workers: “API key **or** certificate”. | v1: API key only for both? Or OIDC/mTLS in v1? |
| **B2** | PRODUCT F3, ARCHITECTURE §3, API_OVERVIEW | “optional heartbeat” | v1: heartbeat in scope (workers call `POST /workers/:id/heartbeat`) or out? |
| **B4** | TECH_STACK §5, Summary table | “Append-only files **or** DB”; “file or DB persistence” | v1: logs in Postgres only, or files only, or both? |
| **B5** | TECH_STACK §5, ARCHITECTURE §6, Summary table | “WebSocket **or** SSE” for log tail and session attach | v1: WebSocket only, SSE only, or both (which is default)? |
| **B6** | TECH_STACK §1, ARCHITECTURE §4, API_OVERVIEW | “Workers poll or are notified via DB”; “Pull task (or long-poll)” | v1: workers poll / long-poll only, or also DB LISTEN/NOTIFY? |
| **B7** | PRODUCT BYOL | “e.g. OAuth or token in the Web UI or CLI” | v1: user pastes token only, or OAuth for Claude/Cursor if available? |
| **B8** | TECH_STACK §4 | “Settings (e.g. control plane URL **if configurable**)” | v1: control plane URL configurable in the UI (e.g. for Tailscale) or only env/config/CLI? |
| **B9** | PRODUCT L5, ARCHITECTURE §6, DECISIONS §2 | “configurable, e.g. 30 days” | Is **30 days** the actual default retention, or just an example? |

**Already decided (no action):**  
- **B3** — Log transport: HTTPS POST to control plane only (reflected in TECH_STACK).  
- **B10** — v1 agent CLIs: Claude Code and Cursor only (reflected in PRODUCT and TECH_STACK).

---

## 3. Wording that still sounds tentative

**Applied.** The following wording changes have been made in the docs.

| Location | Current | Suggestion |
|----------|---------|------------|
| **API_OVERVIEW** (top) | “Placeholder for REST and WebSocket contracts. Fill in as you implement.” | “REST and WebSocket contract sketch. These will be formalized in the OpenAPI spec and implemented accordingly.” |
| **API_OVERVIEW** | “## REST (conceptual)” and “## WebSocket (conceptual)” | Remove “(conceptual)” or change to “(to be formalized in OpenAPI)”. |
| **API_OVERVIEW** (bottom) | “Refine the endpoints … when implementing” | “Endpoints and message shapes will be formalized in the OpenAPI spec and implemented in server, worker, and CLI.” |
| **TECH_STACK** §7 | “**Suggested** Repo Layout” | “**Repo layout**” (we’ve decided). |
| **TECH_STACK** Summary table | “**Suggested** stack” | “**Stack**”. |
| **PROJECT_KICKOFF** §5 | “Phases / Milestones (**Suggested**)” | “Phases / Milestones” (or “(approved)” once you lock them). |

---

## 4. Optional / “later” (intentionally open)

These are fine to leave as optional or future scope; no change needed unless you want to lock one.

- Worker **labels** (optional).
- **Wake integration** (optional, deployer-provided).
- **Execution isolation:** “container later”.
- **Log:** “optional Loki/ClickHouse later”.
- **Phases:** “Adjust order and scope to match your priorities”.
- **Risks:** “optional checkpointing”, “optional HA later”.

---

## 5. Summary: what to do for a rock-solid design

1. **Decide**  
   - **12** (success criteria: approve or change).  
   - **B1–B9** (auth, heartbeat, log persistence, log tail protocol, worker task acquisition, BYOL sign-in, control plane URL in UI, default retention).

2. **Edit docs**  
   - Update DECISIONS.md with each decision.  
   - Replace either/or and “optional” in TECH_STACK, PRODUCT, ARCHITECTURE, API_OVERVIEW with the chosen option.  
   - Apply the **Section 3** wording changes (placeholders and “Suggested” → decided).

3. **Leave as-is (for now)**  
   - **8b** and **8c** (CI platform and Git host) until you’re ready to use CI/host.  
   - **Section 4** optional/later items unless you want to lock them.

After (1) and (2), the written design is fully decided and consistent for building.
