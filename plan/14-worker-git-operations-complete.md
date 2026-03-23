# 14 - Worker Git Operations

## Goal
Implement Git clone, checkout, commit, and push using libgit2 (git2 crate) following GIT_CLONE_SPEC.md exactly. This is the worker's interface to repositories — must handle token embedding, redirect following, and credential callbacks.

## What to build

### Git operations module (`crates/worker/src/git_ops.rs`)

**Token-embedded URL builder**
- `embed_token_into_url(repo_url, git_token) -> String`
- For http:// and https:// URLs: `{scheme}://git:{percent_encoded_token}@{host}{path}`
- Use `urlencoding::encode()` for token only
- Strip existing `user@` from authority before re-embedding
- Username: `git` for GitHub, `oauth2` for `*.gitlab.com`, configurable via `REMOTE_HARNESS_GIT_HTTPS_USER`

**Clone**
- `clone_repo(repo_url, ref_, git_token, work_dir) -> Result<Repository>`
- Build token-embedded URL
- Create `FetchOptions` with:
  - `follow_redirects(RemoteRedirect::All)`
  - `RemoteCallbacks::credentials` callback returning `Cred::userpass_plaintext("git", token)`
- Clone into work_dir
- Checkout specified ref (branch or commit)
- Fail fast with clear error on: invalid URL, auth failure, ref not found

**Checkout branch**
- `checkout_or_create_branch(repo, branch_name, from_ref) -> Result<()>`
- For PR/MR mode: create feature branch from HEAD after checkout
- Branch naming: `harness/{short_session_id}` or custom prefix from params

**Commit**
- `commit_changes(repo, message) -> Result<Oid>`
- Stage all changes (`git add -A` equivalent)
- Create commit with configured author/committer
- Author: from env or default "Remote Harness <harness@local>"

**Push**
- `push_to_remote(repo, branch, git_token) -> Result<()>`
- Build token-embedded URL for push
- Create `PushOptions` with:
  - `follow_redirects(RemoteRedirect::All)`
  - `RemoteCallbacks::credentials` callback
- Push to origin
- Return clear error on auth failure, rejected push, etc.

### Work directory management
- Create temp work directories per task (e.g. `/tmp/harness-{task_id}/`)
- Clean up after task completes (success or failure)

## Dependencies
- Task 13 (worker foundation — needs worker binary to run in)

## Design decisions
- Always use token-embedded URL + credential callback (belt and suspenders per GIT_CLONE_SPEC)
- Never rely on host's global git config
- Clone into a fresh directory per task (no reuse of prior clones)

## Test criteria
- [ ] `embed_token_into_url` correctly builds URLs for GitHub and GitLab
- [ ] Token with special characters (`@`, `/`, `%`) is properly percent-encoded
- [ ] Existing `user@` in URL is stripped before re-embedding
- [ ] Clone with valid token succeeds (test against a real or mock git server)
- [ ] Clone with invalid token fails with clear error message
- [ ] Checkout to specific branch/ref works
- [ ] Create and checkout new branch works
- [ ] Commit stages all changes and creates commit
- [ ] Push with valid token succeeds
- [ ] Push with `follow_redirects(All)` set on PushOptions
- [ ] FetchOptions and PushOptions both set credential callback
- [ ] Work directory is cleaned up after task
- [ ] Unit tests for URL embedding logic
- [ ] `cargo test -p worker` passes
