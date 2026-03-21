# Git clone and push: avoiding "too many redirects or authentication replays"

When clone or push fails in production, surface a **clear** message to the user (job/session error + link to Settings for Git credentials). See [CLIENT_EXPERIENCE.md §6](CLIENT_EXPERIENCE.md#6-jobs-failures-outside-the-users-control).

**`file://` remotes (non-production):** The worker also supports **`file://`** URLs for **local development and automated tests** (clone/push against a local bare repository). Production deployments should use **HTTPS** remotes with a PAT per the checklist below; `file://` is intentionally excluded from the production-focused problem statement in §1.

**Implementation checklist (end of this doc):** This is a **release / code-review gate** for the worker: every box should be verifiable before declaring Git HTTPS support done. It is **not** a public operator checklist; unchecked items here do **not** mean the spec is optional—only that the implementer has not yet confirmed the code path. The canonical file path is **`crates/worker/src/git_ops.rs`** in the **implementation** monorepo ([docs/README — Repository layout](README.md#repository-layout)).

## Problem

When the worker clones a repo over HTTP/HTTPS using libgit2 (via the Rust `git2` crate), clone or push can fail with:

```text
clone failed: too many redirects or authentication replays; class=Http (34)
```

This happens when:

1. **Redirects**: The server responds with HTTP redirects (e.g. HTTP→HTTPS, or through a load balancer/CDN). libgit2 follows redirects but has an internal limit; if the credential callback is invoked again on each redirect, it counts as "authentication replays" and triggers the error.
2. **Credential callback only**: If credentials are supplied only via a `RemoteCallbacks::credentials` callback (and not in the URL), libgit2 may call the callback again after each redirect. Multiple redirects or auth challenges then hit the replay limit.

## Required solution (do not regress)

The worker **must** implement all of the following. In the standard monorepo, implementation lives in **`crates/worker/src/git_ops.rs`** (or an equivalent module if renamed—keep behavior identical).

### 1. Embed the token in the clone URL (HTTP and HTTPS)

- For both `http://` and `https://` repo URLs, build a URL that includes the token in the authority so that the **first** request and any **redirected** request carry credentials without re-invoking the callback.
- Format: `{scheme}://git:{ENCODED_TOKEN}@{host}{path}`  
  Example: `https://git:ghp_xxxx%40encoded@github.com/org/repo.git`
- Use username **`git`** for GitHub; **`oauth2`** for **gitlab.com** (and `*.gitlab.com`) with a PAT — GitLab’s documented HTTPS form. Self‑hosted or other hosts: set **`REMOTE_HARNESS_GIT_HTTPS_USER`** if needed. The password is the token.
- If the repo URL already contains a user (e.g. from a previous embed), strip it and use only the host part before re-embedding (e.g. take the segment after the last `@` in the authority).

### 2. URL-encode the token

- Tokens can contain characters that are invalid or meaningful in a URL (e.g. `@`, `/`, `%`, `#`, `?`). The token **must** be percent-encoded when placed in the URL.
- Use a single, consistent encoding (e.g. `urlencoding::encode` in Rust) for the token segment only. Do not double-encode.
- Do not encode the rest of the URL (scheme, host, path) beyond the token.

### 3. Follow redirects at all stages (fetch and push)

- libgit2’s default is to follow redirects only on the **initial** request. Multi-step redirects (e.g. HTTP→HTTPS then to a CDN) can still trigger "too many redirects" or replay limits.
- Set **fetch** options: `FetchOptions::follow_redirects(RemoteRedirect::All)` for all clone operations.
- Set **push** options: `PushOptions::follow_redirects(RemoteRedirect::All)` for all push operations.

### 4. Set a credential callback in addition to URL embedding

- Some servers or redirect targets do not receive or use credentials from the URL (e.g. redirect to another host strips them), and libgit2 then reports "remote authentication required but no callback set". So **always** set `RemoteCallbacks::credentials` that returns `Cred::userpass_plaintext("git", token)` for fetch (and push). The URL embedding still helps by satisfying the first request and same-URL redirects without invoking the callback; the callback covers redirects or 401s where URL credentials are not used.
- If you see "too many redirects or authentication replays" again, the cause may be an unusually long redirect chain or a server that repeatedly challenges auth; keep `follow_redirects(RemoteRedirect::All)` and URL embedding; consider checking the repo URL (avoid unnecessary redirects) or the git host’s docs.

## Implementation checklist

- [x] **Embed token in URL** (`embed_token_into_url`): For `http://` and `https://` URLs, produce `scheme://git:{percent_encoded_token}@{host}{path}`; strip any existing `user@` from the authority before adding `git:token@`.
- [x] **Token encoding**: Use URL percent-encoding for the token only (e.g. `urlencoding::encode(git_token)` in Rust).
- [x] **FetchOptions**: Every clone path uses `fetch_opts.follow_redirects(RemoteRedirect::All)` and sets `remote_callbacks.credentials` to return the same token (so redirects that lose URL credentials still get auth).
- [x] **PushOptions**: Every push path uses `push_opts.follow_redirects(RemoteRedirect::All)` and sets credentials callback (push uses the same token).
- [x] **Credentials callback**: Always set for fetch and push when a git token is used (required for "remote authentication required" when URL creds are not sent on redirect).
- [x] **`file://` (dev/tests only):** `clone_repository` / `fetch_origin` / `push_refspec` branch for `file:` URLs without embedding tokens (see module docs on `crates/worker/src/git_ops.rs`).

## References

- libgit2: [git_remote_redirect_t](https://libgit2.org/docs/reference/main/remote/) — `GIT_REMOTE_REDIRECT_INITIAL` (default) vs `GIT_REMOTE_REDIRECT_ALL`.
- git2 crate: [FetchOptions::follow_redirects](https://docs.rs/git2/latest/git2/struct.FetchOptions.html), [PushOptions::follow_redirects](https://docs.rs/git2/latest/git2/struct.PushOptions.html), [RemoteRedirect](https://docs.rs/git2/latest/git2/enum.RemoteRedirect.html).
- Error: `class=Http (34)` corresponds to libgit2’s HTTP "too many redirects or authentication replays" condition.
