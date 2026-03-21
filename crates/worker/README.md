# `worker` crate

Long-lived **worker** binary: registers with the control plane, sends periodic **heartbeats** (idle vs **busy** with `current_job_id` while a task runs), and **polls** `POST /workers/tasks/pull` with exponential backoff when idle. When a task is assigned, it runs the full loop in [`task_execution`](src/task_execution.rs): **clone → checkout → branch planning → agent → commit/push → `POST` logs → `POST` complete**.

HTTP calls are centralized in [`ControlPlaneClient`](src/control_plane.rs) (`reqwest` + shared types from `api-types`).

Git (clone, fetch, checkout, commit, push) lives in [`git_ops`](src/git_ops.rs) (`git2`): for **HTTPS**, the token is embedded in the URL (percent-encoded), `RemoteRedirect::All` on fetch/push, and a credential callback—see [GIT_CLONE_SPEC.md](../../docs/GIT_CLONE_SPEC.md). **Production** remotes are `http://` / `https://` only (no SSH). **`file://`** is supported for **local dev and tests** (e.g. bare repo on disk).

**Agent CLI (Claude Code / Cursor)** — [`agent_cli`](src/agent_cli/mod.rs): builds argv + env from `agent_cli` + prompt (`build_invocation`), applies Windows vs Unix spawn options (`AgentCliRunner`), and streams child stdout/stderr through a redacting sink while capturing raw text for `task_complete` fields (`run_invocation`). **Agent tokens** are passed in **environment variables only** (`CURSOR_API_KEY`, `ANTHROPIC_API_KEY`), never as argv. For **Cursor**, when session `params.model` is set, the worker adds **`--model <name>`** to argv (the supported switch per Cursor CLI docs); it also sets `CURSOR_MODEL` for compatibility. See [ARCHITECTURE.md §4c](../../docs/ARCHITECTURE.md#4c-platform-specific-workers-cli-invocation).

| Variable | Required | Description |
|----------|----------|-------------|
| `CURSOR_AGENT_PATH` | no | Cursor `agent` binary path; default `agent` (Unix) / `agent.exe` (Windows). |
| `REMOTE_HARNESS_CURSOR_AGENT_BIN` | no | Alternate override for Cursor binary path. |
| `REMOTE_HARNESS_CURSOR_AGENT_ARGS` | no | Space-separated argv **before** prompt (default: `run --print`). The worker **always** injects `-f` after the first token when it is missing (non-interactive), including when this variable overrides the default. If you already pass **`--model`** here, the worker will not add a second one from `params.model`. |
| `CLAUDE_CLI_PATH` | no | Claude Code CLI path; default `claude` / `claude.exe`. |
| `REMOTE_HARNESS_CLAUDE_BIN` | no | Alternate override for Claude binary path. |
| `REMOTE_HARNESS_CLAUDE_AGENT_ARGS` | no | Space-separated argv override; when empty, worker uses `-p` plus prompt or `-p -` + stdin. |

### Task execution & work directory

| Behavior | Detail |
|----------|--------|
| Work dir | `<REMOTE_HARNESS_WORK_DIR>/jobs/<job_id>/` — the directory is **removed** before each job, then a **fresh clone** is created (no reuse across jobs). |
| Default root | `{system temp}/remote_harness_worker_jobs` when `REMOTE_HARNESS_WORK_DIR` is unset. |
| `branch_mode` | **`main`:** work on the checked-out branch; if **detached HEAD**, worker creates `rh/job-<hex>`. **`pr`:** new branch `{branch_name_prefix}/job-<hex>` (default prefix `rh/`). After the main agent, placeholder branches are **renamed** to `{branch_name_prefix}/<slug>` when the slug comes from the metadata agent (see below). |
| `mr_title` | For `pr` mode only, worker sends **`Harness: <metadata commit subject>`** when a commit was made; if the tree stayed clean, **`Harness: <branch>`**. The control plane still owns real PR/MR creation. |
| Git metadata agent | After local changes, a **second** Cursor/Claude invocation asks for JSON: `branch_slug`, `commit_subject`, `commit_body`. **`REMOTE_HARNESS_SKIP_GIT_METADATA_AGENT=1`** skips it and uses a deterministic fallback from prompt + `git diff HEAD`. Also skipped for **`REMOTE_HARNESS_STUB_AGENT`**. |
| Chat / sentinel | **chat:** `assistant_reply` from captured stdout (trimmed, capped). **loop_until_sentinel:** `output` + `sentinel_reached` from combined stdout/stderr; v1 match is **case-sensitive** substring ([API_OVERVIEW §4](../../docs/API_OVERVIEW.md#4-rest--sessions)). |
| Commit identity | `REMOTE_HARNESS_GIT_AUTHOR_NAME` / `REMOTE_HARNESS_GIT_AUTHOR_EMAIL` (defaults: `remote-harness-worker` / `worker@remote-harness.local`). |
| Stub agent (tests) | `REMOTE_HARNESS_STUB_AGENT=1` (or `true` / `yes`) skips the vendor CLI, writes `.remote-harness-stub`, prints optional `REMOTE_HARNESS_STUB_AGENT_STDOUT`. |

### Manual smoke checklist (macOS, one OS)

Prerequisites: real **Cursor** or **Claude Code** CLI on `PATH` (or set `CURSOR_AGENT_PATH` / `CLAUDE_CLI_PATH`), valid `agent_token` in session/identity, control plane running.

1. `cargo test -p worker` — passes (includes subprocess `echo` redaction test; no vendor CLI required).
2. In `cargo test -p worker agent_cli::runner::tests::fake_echo_subprocess_no_token_in_logs -- --nocapture` optional: confirms log sink never prints the injected secret substring.
3. **Operator check (real CLI):** from a throwaway repo clone directory, set `CURSOR_API_KEY` or `ANTHROPIC_API_KEY` in the environment only; run the same argv your worker would use (`agent run -f --print` + prompt, or with a model from session params `agent run -f --model composer-2 --print` + prompt, or `claude -p "…"`) and confirm the process exits 0 and prints expected assistant text. The worker uses that same invocation shape from `build_invocation` + `run_invocation`.

## Environment

| Variable | Required | Description |
|----------|----------|-------------|
| `CONTROL_PLANE_URL` or `REMOTE_HARNESS_URL` | yes | Base URL of the control plane (no trailing slash). |
| `API_KEY` or `REMOTE_HARNESS_API_KEY` | yes | Same API key the server accepts (`Authorization: Bearer …`). |
| `WORKER_ID` | no | Stable worker id; default: hostname + `-worker`, or a random UUID suffix if hostname is missing. |
| `WORKER_HOST` | no | Optional `host` field on register (observability). |
| `WORKER_HEARTBEAT_INTERVAL_SECS` | no | Default **30**; must be ≥ 1 if set. |
| `REMOTE_HARNESS_WORK_DIR` | no | Base directory for per-job clones (see above). |
| `WORKER_INBOX_AGENT_ID` or `REMOTE_HARNESS_INBOX_AGENT_ID` | no | If set, after register the worker calls **`POST /workers/:id/inbox-listener`** with this **`agent_id`** (must match **`params.agent_id`** on an inbox session). Required for **`POST /agents/:id/inbox`** queue processing ([API_OVERVIEW §8](../../docs/API_OVERVIEW.md#8-rest--inboxes-p1)). |
| `REMOTE_HARNESS_GIT_HTTPS_USER` | no | HTTPS Git username for non-GitHub / non-GitLab.com hosts (overrides `git` / `oauth2` defaults). |
| `REMOTE_HARNESS_SKIP_GIT_METADATA_AGENT` | no | When `1` / `true` / `yes`, skip the post-change JSON metadata agent call; branch rename still applies on placeholder branches using the fallback slug. |

`client_version` on register is **`CARGO_PKG_VERSION`** of this crate (workspace semver). **`409 Conflict`** on register (same id already registered) is treated as **success** so restarts are idempotent.

## Run (bare metal)

```bash
export CONTROL_PLANE_URL=http://127.0.0.1:3000
export API_KEY=your-key
cargo run -p worker
```

## Docker Compose

The root `docker-compose.yml` builds and runs the worker with `CONTROL_PLANE_URL=http://server:3000` and the same `API_KEY` as the server. **`Dockerfile.worker`** installs **Cursor `agent`** during the image build (`CURSOR_AGENT_VERSION` build arg). **Claude Code** is not included — set `CLAUDE_CLI_PATH` or extend the image. For smoke without any vendor CLI, use `REMOTE_HARNESS_STUB_AGENT=1` (see [GETTING_STARTED.md §1.4](../../docs/GETTING_STARTED.md#14-agent-cli-in-the-worker-container)).

## Tests

```bash
cargo test -p worker
```

Integration tests use **wiremock** for HTTP (register, pull, heartbeat, logs, complete). Git URL embedding is covered by `cargo test -p worker git_ops` (no network). **`e2e_file_remote_stub`** (Unix, `git` on `PATH`) exercises a full **stub** job against a **`file://`** bare remote and asserts log + complete calls.
