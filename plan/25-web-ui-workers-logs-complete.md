# 25 - Web UI Workers & Log Viewer

## Goal
Build the worker list page (with heterogeneity warnings) and the log viewer component (history + SSE streaming). The log viewer is used both standalone and embedded in session detail.

## What to build

### Worker List (`web/src/pages/Workers.tsx`)
- Fetch: GET /workers
- Table columns: worker_id | host | platform | status (badge) | last_seen_at
- Status badges: active (green), stale (red/gray)
- "Remove" button per worker (DELETE /workers/:id with confirmation)
- Auto-refresh (poll every 10s)

**Heterogeneity warning banner (CLIENT_EXPERIENCE §10)**
- Detect: when 2+ non-stale workers have different `labels.platform` values
- Show warning banner: "Mixed worker pool detected. The engine may assign any session to any worker. Mixed OS or missing CLIs can cause failures."
- Link to documentation
- Also detect WSL vs native Windows mix

### Log Viewer Component (`web/src/components/LogViewer.tsx`)
- Reusable component, used in:
  - Session detail (embedded)
  - Standalone log tail (optional page)
- Props: sessionId, jobId? (filter), level? (filter)

**History loading**
- On mount: GET /sessions/:id/logs, paginate until all loaded
- Show loading indicator while fetching
- Render all log entries in scrollable container

**SSE streaming**
- After history loaded: connect to GET /sessions/:id/logs/stream
- Append new entries to the bottom
- Auto-scroll to bottom (with "stick to bottom" toggle)
- Show "Reconnecting..." on disconnect

**Log entry display**
- Format: `[HH:MM:SS] [LEVEL] [source] message`
- Color-code by level: debug (gray), info (default), warn (yellow), error (red)
- Source badge: agent, worker, control_plane
- Monospace font for log content

**Filters**
- Level filter: dropdown (all, debug, info, warn, error)
- Job filter: dropdown populated from session's jobs (when in session detail context)
- Filters apply to both history fetch and SSE stream

**Performance**
- Virtualized list for large log volumes (e.g. react-window or @tanstack/virtual)
- Limit rendered entries with "Load earlier" button if thousands of entries

### Session detail integration
- Log viewer embedded below the session/jobs section
- Full width, resizable height
- Collapses to summary when session is completed

## Dependencies
- Task 22 (web UI foundation)
- Task 08 (worker registration — server endpoints)
- Task 11 (log ingestion — server endpoints)
- Task 12 (SSE streaming — server SSE endpoints)

## Test criteria
- [ ] Worker list displays all workers with correct status
- [ ] Heterogeneity warning shown when workers have different platforms
- [ ] Warning hidden when all workers have same platform
- [ ] Worker removal works with confirmation
- [ ] Log viewer loads full history on mount
- [ ] Log viewer streams new entries via SSE
- [ ] Auto-scroll to bottom works
- [ ] Level filter applies to history and stream
- [ ] Job filter narrows to specific job's logs
- [ ] Log entries color-coded by level
- [ ] "Reconnecting..." shown on SSE disconnect
- [ ] Handles large log volumes without freezing (virtualized)
- [ ] Log viewer works embedded in session detail
