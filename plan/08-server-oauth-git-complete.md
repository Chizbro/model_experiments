# 08 — Server: GitHub / GitLab OAuth (full vertical)

**Status:** complete  
**Dependencies:** 07

## Objective

Implement **complete OAuth** for Git credentials in **one task** to avoid scattered half-finished auth: CSRF cookie, PKCE S256, callbacks, token storage, refresh-before-use ([API_OVERVIEW §4b](../docs/API_OVERVIEW.md#4b-oauth--git-provider-sign-in-identity-credentials)).

## Scope

**In scope**

- `GET /auth/github`, `GET /auth/github/callback`, `GET /auth/gitlab`, `GET /auth/gitlab/callback`.
- HttpOnly `SameSite=Lax` cookie for state + PKCE verifier; validate on callback.
- Store `git_token`, `refresh_token`, `token_expires_at`, `git_provider`, `git_base_url` on identity.
- Proactive refresh when expired or within **5 minutes** before use (spec).
- **503** when OAuth not configured (CLIENT_EXPERIENCE mapping).

**Out of scope**

- Agent-vendor OAuth (explicitly not v1 per PRODUCT).

## Spec references

- [API_OVERVIEW §4b](../docs/API_OVERVIEW.md#4b-oauth--git-provider-sign-in-identity-credentials)
- [CLIENT_EXPERIENCE §2.1 — 503 on OAuth](../docs/CLIENT_EXPERIENCE.md#21-web-ui-mapping)

## Acceptance criteria

- Integration tests: CSRF failure, happy path with mocked token exchange (no secrets in repo).
- Document required env vars in README or HOSTING pointer.

## Testing

| When | What | Retest |
|------|------|--------|
| After implementation | `cargo test` OAuth module | CI; manual browser smoke once |

---

## Completed / Notes

- Implemented `GET /auth/github`, `/auth/github/callback`, `/auth/gitlab`, `/auth/gitlab/callback` with `_rh_oauth` cookie (HttpOnly, SameSite=Lax), PKCE S256, and identity token persistence including refresh metadata and `git_base_url` for GitLab.
- `GET /identities/:id/repositories` refreshes OAuth access tokens when expired or within 5 minutes.
- `PATCH /identities/:id` clears OAuth metadata when `git_token` is set or cleared manually.
- Integration tests: `crates/server/tests/oauth_integration.rs` (requires `DATABASE_URL`); wiremock 0.5 for token exchange.
- OpenAPI + README + HOSTING + CLI `oauth github|gitlab` + Web sign-in links.
