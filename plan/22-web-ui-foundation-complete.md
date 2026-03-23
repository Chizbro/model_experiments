# 22 - Web UI Foundation & Routing

## Goal
Set up the React SPA with routing, API client layer, SSE client, global state, and base layout. This is the foundation all UI pages will build on.

## What to build

### Project setup (in `web/`)
- Vite + React + TypeScript (from Task 01 scaffold)
- Tailwind CSS configured
- shadcn/ui components initialized (Button, Input, Card, Table, Dialog, Badge, etc.)
- Path aliases (`@/components`, `@/lib`, `@/hooks`, etc.)

### API client (`web/src/lib/api.ts`)
- Typed fetch wrapper using control plane URL + API key from localStorage
- All methods typed with api-types equivalents (TypeScript interfaces matching Rust types)
- Standard error handling: parse `{ error: { code, message, details } }` response
- Handle 401 -> redirect to settings/login
- Handle network failure -> show "Cannot reach control plane" with wake URL option

### SSE client (`web/src/lib/sse.ts`)
- Generic SSE hook: `useSSE(url, options)` returning event stream
- Auto-reconnect with exponential backoff (cap at 30s)
- Show "Reconnecting..." state
- Close on component unmount
- Specific hooks:
  - `useLogStream(sessionId, jobId?, level?)` — for log viewer
  - `useSessionEvents(sessionId)` — for session lifecycle

### TanStack Query setup (`web/src/lib/query.ts`)
- QueryClient provider
- Common query keys and hooks:
  - `useSessions()`, `useSession(id)`, `useWorkers()`, `useIdentity(id)`, etc.
- Mutation hooks for POST/PATCH/DELETE operations

### Routing (`web/src/router.tsx`)
- React Router v6 routes:
  - `/` — Dashboard (session list)
  - `/sessions/:id` — Session detail
  - `/workers` — Worker list
  - `/settings` — Settings (API key, credentials, OAuth)
  - `/sessions/new` — Create session

### Layout (`web/src/components/layout/`)
- `AppLayout` — sidebar navigation + main content area
- Sidebar: Dashboard, Workers, Settings links
- Header: app name, connection status indicator
- Connection status: green dot when healthy, yellow when reconnecting, red when unreachable

### Auth/config state (`web/src/lib/config.ts`)
- Store in localStorage: `control_plane_url`, `api_key`, `wake_url`
- Context provider for global access
- First-visit flow: if no URL or key, redirect to Settings

### Theme
- Light/dark mode toggle (Tailwind dark class)
- Clean, professional design

## Dependencies
- Task 01 (web/ scaffold)

## Test criteria
- [ ] `npm run dev` starts Vite dev server
- [ ] `npm run build` produces production build with no errors
- [ ] `npm run typecheck` passes
- [ ] Routes render correct page components
- [ ] API client sends auth header from localStorage
- [ ] API client handles 401 -> redirects to settings
- [ ] SSE client connects, receives events, auto-reconnects on disconnect
- [ ] Connection status indicator reflects API reachability
- [ ] Layout renders with sidebar navigation
- [ ] First-visit without config redirects to Settings
