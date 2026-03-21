# 22 — Web: SPA shell, CORS, first-run bootstrap

**Status:** pending  
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
