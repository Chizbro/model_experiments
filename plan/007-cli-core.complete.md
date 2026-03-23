# 007: CLI — Core Commands

## Goal
A working CLI binary with config management, health check, session CRUD, worker listing, and credentials management. After this spec, a user can drive the system entirely from the command line (except log tailing/attach, which is next spec).

## Scope
### CLI binary structure
- Config from env/YAML/flags: `REMOTE_HARNESS_URL` (or `CONTROL_PLANE_URL`), `REMOTE_HARNESS_API_KEY` (or `API_KEY`). Config file: `~/.config/remote-harness/config.yaml` with `control_plane_url` and `api_key`. Precedence: flag > env > config file.
- Uses clap derive API for command parsing.

### Commands
- `remote-harness config show` — Show resolved config (URL, key masked, source of each)
- `remote-harness health` — GET /health, print status
- `remote-harness session start --repo <url> --workflow <type> [--prompt "..."] [--agent-cli cursor|claude_code] [--n N] [--sentinel "..."] [--branch-mode main|pr] [--ref <ref>] [--persona-id <id>] [--model <model>]` — POST /sessions, print session_id + status + web_url
- `remote-harness session list [--status <status>]` — GET /sessions, tabular output
- `remote-harness session show <id>` — GET /sessions/:id, detailed view with jobs
- `remote-harness session delete <id>` — DELETE /sessions/:id
- `remote-harness workers list` — GET /workers, tabular output
- `remote-harness workers clear <id>` — DELETE /workers/:id
- `remote-harness credentials show` — GET /identities/default, show has_git_token / has_agent_token
- `remote-harness credentials set [--git-token <token>] [--agent-token <token>]` — PATCH /identities/default. If flags not provided, prompt interactively (masked input via rpassword or similar).

### API client
- `api_client.rs` — Typed HTTP client wrapping reqwest. Methods for every endpoint used by the CLI. Uses api-types for request/response types.

### Error handling
- Print HTTP status + error.code + error.message to stderr
- Exit non-zero on error
- No --json flag in v1

## Prerequisites
- Spec 001 (foundation, api-types)
- Server running (specs 002-005) for manual testing

## Files to create/modify
- `crates/cli/Cargo.toml` — Full dependencies (clap, reqwest, tokio, api-types, serde, rpassword)
- `crates/cli/src/main.rs` — Clap app definition, dispatch to command handlers
- `crates/cli/src/config.rs` — Config resolution (flag > env > file)
- `crates/cli/src/api_client.rs` — HTTP client
- `crates/cli/src/commands/mod.rs` — Re-exports
- `crates/cli/src/commands/health.rs` — health command
- `crates/cli/src/commands/session.rs` — session start/list/show/delete
- `crates/cli/src/commands/workers.rs` — workers list/clear
- `crates/cli/src/commands/credentials.rs` — credentials show/set

## Acceptance criteria
1. `cargo run -p cli -- --help` shows all commands
2. `cargo run -p cli -- health` → prints "ok" when server running
3. `cargo run -p cli -- session start --repo https://github.com/foo/bar --workflow chat --prompt "hello" --agent-cli cursor` → prints session_id
4. `cargo run -p cli -- session list` → tabular list of sessions
5. `cargo run -p cli -- session show <id>` → session details with jobs
6. `cargo run -p cli -- session delete <id>` → "deleted"
7. `cargo run -p cli -- workers list` → tabular list
8. `cargo run -p cli -- workers clear <id>` → "removed"
9. `cargo run -p cli -- credentials show` → shows token presence
10. `cargo run -p cli -- credentials set --git-token ghp_xxx --agent-token cur_xxx` → "saved"
11. Config resolution works: env overrides file, flag overrides env
12. Errors print status + code + message to stderr, exit non-zero
13. `cargo build -p cli` succeeds
14. `cargo clippy -p cli -- -D warnings` clean

## Implementation notes
- For interactive token input, use `rpassword::prompt_password` crate.
- Table output: use a simple fixed-width format or the `tabled` crate. Keep it simple.
- `session start` builds the params object based on workflow type and supplied flags.
- The CLI binary name should be `remote-harness` (set in Cargo.toml `[[bin]]`).
