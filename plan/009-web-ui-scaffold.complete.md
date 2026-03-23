# 009: Web UI — Scaffold, API Client & Settings

## Goal
A working React SPA with routing, API client layer, and a Settings page where users can configure the control plane URL, API key, and BYOL credentials. After this spec, the UI connects to the server and can display basic health status.

## Scope
### Project setup
- Vite + React 18 + TypeScript (strict)
- Tailwind CSS + shadcn/ui (Button, Input, Card, Dialog, Toast, Tabs)
- React Router v6
- TanStack Query v5
- ESLint + Prettier config

### Application shell
- `App.tsx` — Router with routes: / (dashboard), /sessions/:id (detail), /sessions/new (create), /workers, /settings
- `Layout.tsx` — Sidebar nav + main content area. Nav items: Dashboard, Workers, Settings.
- Responsive: sidebar collapses on mobile.

### API client layer
- `api/client.ts` — Wraps fetch. Reads control plane URL and API key from localStorage. Attaches Authorization header. Handles errors (parse error body, throw typed errors). Detects CORS/network vs 401 vs 5xx per CLIENT_EXPERIENCE.md §3.
- `api/types.ts` — TypeScript interfaces mirroring api-types crate (Session, Job, Worker, LogEntry, etc.)
- `api/sse.ts` — SSE helper: connect to EventSource URL with auth (polyfilled if needed since EventSource doesn't support headers — use fetch-based SSE or pass API key as query param with server support). Reconnect with backoff.

### Settings page
- Control plane URL input (validated with GET /health on save)
- API key input (masked, validated with an authenticated request on save)
- Credentials section: show has_git_token / has_agent_token (from GET /identities/default), inputs to set them (PATCH /identities/default)
- "Sign in with GitHub" and "Sign in with GitLab" buttons (link to /auth/github and /auth/gitlab on the control plane — only show if server reports they're configured)
- Success toast on save, error toast on failure

### Connection status
- Top bar or status indicator: green = connected, red = cannot reach API, yellow = connected but auth failed
- On failed fetch: show appropriate message per CLIENT_EXPERIENCE.md §3 (CORS vs network vs auth)

## Prerequisites
- Spec 001+ (server running with health and identity endpoints)

## Files to create/modify
All new files in `web/`:
- `package.json`, `tsconfig.json`, `vite.config.ts`, `tailwind.config.js`, `postcss.config.js`, `.eslintrc.cjs`
- `index.html`
- `src/main.tsx` — React root + QueryClient + Router
- `src/App.tsx` — Routes
- `src/api/client.ts` — API client
- `src/api/types.ts` — TypeScript types
- `src/api/sse.ts` — SSE helper
- `src/components/Layout.tsx` — App shell
- `src/pages/Settings.tsx` — Full settings page
- `src/pages/Dashboard.tsx` — Placeholder (will be populated in next spec)
- `src/pages/Workers.tsx` — Placeholder
- `src/pages/SessionDetail.tsx` — Placeholder
- `src/pages/SessionCreate.tsx` — Placeholder

## Acceptance criteria
1. `cd web && npm install && npm run dev` → Vite dev server starts
2. `npm run build` → production build succeeds
3. `npm run lint` → no errors
4. Settings page: can set control plane URL and API key, persisted in localStorage
5. Settings page: health check validates URL before saving
6. Settings page: shows credential status (has_git_token, has_agent_token)
7. Settings page: can set git_token and agent_token
8. Settings page: shows OAuth sign-in buttons when available
9. Connection status indicator works (green/red/yellow)
10. Navigation works between all routes
11. Error messages distinguish CORS vs network vs auth failures

## Implementation notes
- For SSE with auth: EventSource doesn't support custom headers. Options: (a) Use fetch-based SSE (read response as stream, parse SSE format), or (b) Pass API key as query param `?api_key=` and have server accept that for SSE endpoints. Option (a) is more secure; implement that.
- shadcn/ui: use `npx shadcn-ui@latest init` then add components. Or manually create the components if the CLI doesn't work in this context.
- localStorage keys: `rh_control_plane_url`, `rh_api_key`
- API client should have a `isConfigured()` check and redirect to Settings if not configured.
