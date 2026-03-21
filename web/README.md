# Web UI

Production UI is **Vite + React + TypeScript** per [docs/TECH_STACK.md](../docs/TECH_STACK.md#4-web-ui). CI scaffolding shipped in plan task **03**; **Settings + routing shell + bootstrap + API client** shipped in plan task **22** ([`plan/22-web-shell-bootstrap-complete.md`](../plan/22-web-shell-bootstrap-complete.md)); **sessions + workers dashboards** shipped in plan task **23** ([`plan/23-web-sessions-workers-complete.md`](../plan/23-web-sessions-workers-complete.md)).

- **Dev:** `npm run dev` (Vite).
- **Routes:** `/settings` (first-run URL, health, API key, BYOL agent/Git tokens + OAuth, optional wake URL, gated bootstrap), `/` (home + health), `/sessions` (list), `/sessions/new` (create), `/sessions/:id` (detail + jobs), `/workers` (list + heterogeneity banner), `/playground` (full REST debug surface, including duplicate OAuth links).
- **Health check against a running server:** `npm run check-health` (see [GETTING_STARTED.md](../docs/GETTING_STARTED.md)).
