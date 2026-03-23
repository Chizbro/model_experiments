# 24 - Web UI Dashboard, Sessions & Creation

## Goal
Build the main dashboard (session list), session detail page, and session creation form. This is the primary UI for managing workflows.

## What to build

### Dashboard / Session List (`web/src/pages/Dashboard.tsx`)
- Fetch sessions: GET /sessions with pagination
- Table columns: session_id (truncated), workflow type, status (badge), repo, created_at
- Status badges: pending (yellow), running (blue), completed (green), failed (red)
- Click row -> navigate to session detail
- "New Session" button -> navigate to create form
- Status filter dropdown (all, pending, running, completed, failed)
- Auto-refresh with TanStack Query (poll every 5s for active sessions)

### Session Detail (`web/src/pages/SessionDetail.tsx`)
- Fetch: GET /sessions/:id
- Header: session_id, workflow type, status badge, repo_url, ref, created_at
- Jobs table: job_id | status | error_message | branch | commit_ref | PR URL | created_at
  - `error_message` prominently displayed when set (not hidden behind "Failed")
  - `pull_request_url` as clickable link
  - If PR expected but missing: show explanation per CLIENT_EXPERIENCE §8
- Actions:
  - "Retain forever" toggle (PATCH /sessions/:id)
  - "Delete" button with confirmation dialog (DELETE /sessions/:id)
  - "Delete logs" button with confirmation (DELETE /sessions/:id/logs)
- Chat input (if workflow === "chat" and session is running):
  - Text input + "Send" button
  - POST /sessions/:id/input
  - Show "history_truncated" warning when applicable (per CLIENT_EXPERIENCE §12)
- Log panel (embedded log viewer — see Task 25)
- Session events via SSE (useSessionEvents): update status, jobs in real-time

### Git & PR/MR outcome display
- Follow CLIENT_EXPERIENCE §8 strictly:
  - Job failed + error_message: show error prominently with action hints
  - Job completed, no commit_ref: explain push didn't complete
  - PR mode but no pull_request_url: show reason (job not successful / missing branch / provider error)
  - Agent exited non-zero with commits: explain distinction

### Session Creation (`web/src/pages/CreateSession.tsx`)
- Form fields:
  - **Repo URL**: input or repo picker (GET /identities/default/repositories)
  - **Ref**: input (default "main")
  - **Workflow**: dropdown (Chat, Loop N, Loop Until Sentinel)
  - **Prompt**: textarea
  - **Agent CLI**: dropdown (Claude Code, Cursor)
  - **Model**: input (optional)
  - **Branch mode**: radio (Main, PR)
  - **Branch name prefix**: input (optional, shown when PR mode)
  - Workflow-specific:
    - Loop N: **N** number input
    - Loop Until Sentinel: **Sentinel** text input
  - **Persona**: dropdown (GET /personas, optional)
  - **Identity**: dropdown (default "default")
  - **Retain forever**: checkbox
- Validation: repo_url required, prompt required, n > 0 for loop_n
- Check credentials before submit: warn if identity missing tokens
- POST /sessions -> navigate to session detail on success

### Repo picker component
- Dropdown/combobox that fetches repos from GET /identities/:id/repositories
- Searchable
- Shows full_name + sets clone_url

## Dependencies
- Task 22 (web UI foundation — routing, layout, API client, query hooks)
- Task 09 (sessions API — server endpoints)

## Test criteria
- [ ] Dashboard loads and displays sessions in table
- [ ] Pagination works (load more / cursor-based)
- [ ] Status filter works
- [ ] Session detail shows all fields correctly
- [ ] Jobs table displays error_message, branch, commit_ref, PR URL
- [ ] PR missing explanation shown when applicable
- [ ] Chat input works for chat sessions
- [ ] History truncated warning appears when flag is true
- [ ] Retain forever toggle works
- [ ] Delete with confirmation works
- [ ] Session creation form validates and submits
- [ ] Repo picker loads repos from identity
- [ ] Real-time updates via session events SSE
