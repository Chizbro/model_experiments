# 17 — Worker: Git operations (GIT_CLONE_SPEC)

**Status:** pending  
**Dependencies:** 01 (`git2` in worker crate)

## Objective

Implement **`crates/worker/src/git_ops.rs`** exactly per [GIT_CLONE_SPEC.md](../docs/GIT_CLONE_SPEC.md): token-in-URL embed, percent-encode token, **`RemoteRedirect::All`** on fetch/push, credential callback always set—**release gate** is the checklist at end of that spec ([PROJECT_KICKOFF §6a — D](../docs/PROJECT_KICKOFF.md#6a-implementation-plan-concrete-checkpoints)).

## Scope

**In scope**

- `embed_token_into_url` / equivalent with GitHub `git` / GitLab `oauth2` username rules + `REMOTE_HARNESS_GIT_HTTPS_USER` override.
- Clone, fetch, checkout ref, commit, push used by task runner—**same** options everywhere.

**Out of scope**

- SSH Git URLs (document if unsupported in v1).

## Spec references

- [GIT_CLONE_SPEC](../docs/GIT_CLONE_SPEC.md)
- [CLIENT_EXPERIENCE §6 — auth errors](../docs/CLIENT_EXPERIENCE.md#6-jobs-failures-outside-the-users-control)

## Acceptance criteria

- **Unit tests** for URL embedding with nasty tokens (`@`, `%`, `#`) without hitting network.
- Optional **integration** against a local bare repo or public tiny repo with disposable token in CI secret (skip if not available).

## Testing

| When | What | Retest |
|------|------|--------|
| After implementation | `cargo test -p worker git_ops` | CI on every change to module |
