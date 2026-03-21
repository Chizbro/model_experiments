# 22 — Web: SPA shell, CORS, first-run bootstrap

**Status:** complete  
**Dependencies:** 03 (web build), 04, 06, 08 (OAuth URLs optional until 08 done)

## Objective

**Vite + React + TypeScript** SPA with **client-only** API access ([TECH_STACK §4](../docs/TECH_STACK.md#4-web-ui)), **TanStack Query**, control-plane URL + API key storage, and **gated bootstrap** per [CLIENT_EXPERIENCE §7](../docs/CLIENT_EXPERIENCE.md#7-first-time-setup-web-ui) ([PROJECT_KICKOFF §6a — H](../docs/PROJECT_KICKOFF.md#6a-implementation-plan-concrete-checkpoints)).

## Scope

**In scope**

- Settings: URL validation via `GET /health`; probe for bootstrap only when appropriate (`401` / documented empty keys).
- **CORS** error detection path ([CLIENT_EXPERIENCE §3](../docs/CLIENT_EXPERIENCE.md#3-browser-network-tls-and-cors)).
- API client module with shared error mapping ([CLIENT_EXPERIENCE §2.1](../docs/CLIENT_EXPERIENCE.md#21-web-ui-mapping)).
- Base layout, routing shell, theme (Tailwind/shadcn per stack doc).

**Out of scope**

- Session list UI (23).

## Spec references

- [HOSTING — CORS](../docs/HOSTING.md)
- [CLIENT_EXPERIENCE §7, §11](../docs/CLIENT_EXPERIENCE.md)

## Acceptance criteria

- `npm run build` passes; **no** server-side proxy to control plane.
- Smoke: open UI → set URL → health ok → key → authenticated call succeeds.

## Testing

| When | What | Retest |
|------|------|--------|
| After implementation | `npm test` / `npm run lint` if configured | CI |

## Completed / Notes

- **Routing:** `react-router-dom` with `AppLayout` (Home, API playground, Settings). Routes `/` and `/playground` require a persisted control plane URL (redirect to `/settings`).
- **State:** `SettingsProvider` + `useSettings`; localStorage keys in `src/settings/storage.ts` (`rh_control_plane_url`, `rh_api_key`, `rh_wake_url`, bootstrap ineligible per base URL).
- **TanStack Query:** health checks on Home and Settings; mutations for save URL, verify key, bootstrap.
- **API layer:** `src/api/client.ts` (`controlPlaneFetch`, `controlPlaneJson`), `src/api/errors.ts` (`mapHttpError`, `mapFetchFailure` for CORS vs same-origin network). Vitest unit tests in `src/api/errors.test.ts`.
- **Bootstrap UX:** Shown only when no API key is stored and `rh_bootstrap_ineligible:<base>` is not set; cleared after successful `GET /api-keys` or `403` from `POST /api-keys/bootstrap`.
- **Theme:** Tailwind v4 via `@tailwindcss/vite`; prior monolith UI moved to **`/playground`** (OAuth links unchanged).
- **Docs:** `docs/CLIENT_EXPERIENCE.md` §7 implementation note; `docs/PROJECT_KICKOFF.md` checkpoint **H** marked done.
