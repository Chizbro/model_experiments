# 06 — Server: API key authentication and bootstrap

**Status:** pending  
**Dependencies:** 04, 05

## Objective

Implement **Bearer / X-API-Key** validation for protected routes and full **API key lifecycle** + **`POST /api-keys/bootstrap`** per [API_OVERVIEW §4c](../docs/API_OVERVIEW.md#4c-rest--api-keys-control-plane-auth) and [CLIENT_EXPERIENCE §7](../docs/CLIENT_EXPERIENCE.md#7-first-time-setup-web-ui).

## Scope

**In scope**

- Middleware or extractor: reject missing/invalid key with standard error body §2.
- `POST /api-keys`, `GET /api-keys` (pagination), `DELETE /api-keys/:id`.
- Bootstrap: **201** when no keys exist; **403** when keys already exist; documented safety copy for operators ([API_OVERVIEW — Bootstrap safety](../docs/API_OVERVIEW.md#bootstrap-safety-operators-must-read-this)).
- Optional env `API_KEY` bootstrap key if spec requires—align with HOSTING.

**Out of scope**

- Web UI (task 22)—server-only.

## Spec references

- [API_OVERVIEW §2, §4c](../docs/API_OVERVIEW.md)
- [HOSTING §13–14](../docs/HOSTING.md#13-production-and-first-run-checklist)

## Acceptance criteria

- Integration tests: authenticated vs `401`; bootstrap only when empty DB keys.
- OpenAPI + implementation synced; hashes stored, plaintext shown once on create.

## Testing

| When | What | Retest |
|------|------|--------|
| After implementation | `cargo test` integration | CI |
