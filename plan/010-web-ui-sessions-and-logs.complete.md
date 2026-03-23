# 010: Web UI — Sessions, Log Viewer & Workers

## Goal
The Web UI can create sessions, list and view them with full log history + live streaming, display workers, and show session events. This makes the UI fully functional for the core workflow.

## Scope
### Dashboard page (/)
- Session list with status badges (pending=gray, running=blue, completed=green, failed=red)
- Columns: session_id (truncated), repo, workflow, status, created_at
- Click row → navigate to /sessions/:id
- "New Session" button → navigate to /sessions/new
- Auto-refresh via TanStack Query (poll every 5s or on window focus)

### Session Create page (/sessions/new)
- Form: repo_url (text input or repo picker from GET /identities/default/repositories), workflow (select: chat, loop_n, loop_until_sentinel), prompt (textarea), agent_cli (select: cursor, claude_code)
- Conditional fields: n (number, for loop_n), sentinel (text, for loop_until_sentinel)
- Optional: ref, branch_mode (select: main, pr), model, persona_id (select from GET /personas)
- Submit → POST /sessions → navigate to session detail on success
- Validation: require repo_url, prompt, agent_cli

### Session Detail page (/sessions/:id)
- Header: session_id, repo, workflow, status badge, created_at
- Jobs section: list of jobs with status, error_message, pull_request_url (as link), commit_ref
- Log viewer (main area):
  1. Load full history (paginate GET /sessions/:id/logs until next_cursor is null)
  2. Render log entries (timestamp, level with color, source, message)
  3. Open SSE stream for live logs
  4. Auto-scroll to bottom on new entries (with "scroll to bottom" button if user has scrolled up)
- Session events: show in-line notifications (e.g., "Job started", "Session completed")
- For chat sessions: input box at bottom to send follow-up (POST /sessions/:id/input)
- Delete button (with confirm dialog) → DELETE /sessions/:id → redirect to dashboard
- retain_forever toggle

### Workers page (/workers)
- Table: worker_id, host, platform (from labels), status badge, last_seen_at
- Delete button per worker (with confirm) → DELETE /workers/:id
- Heterogeneity warning banner: if workers have different platform labels, show warning per CLIENT_EXPERIENCE.md §10

### Log viewer component
- `components/LogViewer.tsx` — Reusable. Props: sessionId, optional jobId. Handles history loading + SSE streaming internally.
- Log entry rendering: monospace font, level-colored badge, timestamp (relative or absolute toggle), source tag
- Loading state while fetching history
- "Reconnecting..." banner on SSE disconnect

## Prerequisites
- Spec 009 (UI scaffold, API client, settings)
- Server specs 001-005 (all endpoints the UI calls)

## Files to create/modify
- `web/src/pages/Dashboard.tsx` — Full implementation
- `web/src/pages/SessionCreate.tsx` — Full implementation
- `web/src/pages/SessionDetail.tsx` — Full implementation
- `web/src/pages/Workers.tsx` — Full implementation
- `web/src/components/LogViewer.tsx` — New
- `web/src/components/SessionList.tsx` — New
- `web/src/components/StatusBadge.tsx` — New (reusable status badge)
- `web/src/api/types.ts` — Ensure all needed types exist

## Acceptance criteria
1. Dashboard shows list of sessions with correct statuses
2. "New Session" form creates a session and navigates to detail
3. Session detail loads and shows session info + jobs
4. Log viewer loads full history, then streams new entries
5. Log viewer auto-scrolls and has scroll-to-bottom button
6. Chat session detail has input box for follow-up messages
7. Session events appear in the detail view
8. Workers page shows all workers with status
9. Workers page shows heterogeneity warning when platforms differ
10. Delete session/worker works with confirmation
11. retain_forever toggle works on session detail
12. `npm run build` succeeds
13. `npm run lint` passes

## Implementation notes
- For log history pagination: use a `useEffect` loop that fetches pages until next_cursor is null, accumulating entries in state. Show loading spinner during this.
- SSE streaming: use the fetch-based SSE helper from spec 009. Parse events and append to log state.
- Auto-scroll: track whether user has scrolled up (scroll position < max). If yes, show "New logs" button. If no, auto-scroll.
- Heterogeneity detection: from workers list, check if Set of platform labels has size > 1.
