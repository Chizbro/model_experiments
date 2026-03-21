# 07 — Server: identities (BYOL credentials)

**Status:** complete  
**Dependencies:** 06

## Objective

**Identity** APIs for Git + agent tokens: status endpoints without leaking secrets, PATCH merge behavior with session params, repo list for picker—**one vertical slice** so BYOL is coherent before OAuth ([API_OVERVIEW §4a](../docs/API_OVERVIEW.md#4a-rest--identities-byol-credentials)).

## Scope

**In scope**

- `GET /identities/:id` — `has_git_token`, `has_agent_token` (exact shape per OpenAPI).
- `GET /identities/:id/auth-status` — statuses from spec (`healthy`, `expiring_soon`, ...).
- `PATCH /identities/:id` — partial token updates; **never** return token values.
- `GET /identities/:id/repositories` — GitHub/GitLab list using stored token; map errors to 401/502 per spec.
- Session create rejection when default identity missing tokens (once sessions exist—coordinate with 11).

**Out of scope**

- OAuth token refresh logic lives partly here but **browser redirects** are task 08.

## Spec references

- [PRODUCT — BYOL](../docs/PRODUCT.md#bring-your-own-licence-byol)
- [CLIENT_EXPERIENCE §5](../docs/CLIENT_EXPERIENCE.md#5-credentials-and-byol)

## Acceptance criteria

- Integration tests with mocked GitHub/GitLab HTTP client or stub provider (avoid live network in default CI).
- OpenAPI reflects all fields; no plaintext tokens in responses.

## Testing

| When | What | Retest |
|------|------|--------|
| After implementation | `cargo test` + contract tests | CI |
