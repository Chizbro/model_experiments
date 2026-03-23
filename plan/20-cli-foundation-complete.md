# 20 - CLI Foundation & Config

## Goal
Build the CLI binary with clap command structure, configuration loading, and API client. The CLI shares types with the server via the api-types crate.

## What to build

### Configuration (`crates/cli/src/config.rs`)
- Load from: CLI flags > env vars > config file
- Config file: `~/.config/remote-harness/config.yaml`
- Fields:
  - `control_plane_url` / `REMOTE_HARNESS_URL` / `CONTROL_PLANE_URL`
  - `api_key` / `REMOTE_HARNESS_API_KEY` / `API_KEY`
  - `wake_url` / `WAKE_URL` (optional)
  - `wake_script` / `WAKE_SCRIPT` (optional)
- `config show` subcommand: display resolved config and precedence

### Clap command structure (`crates/cli/src/main.rs`)
```
remote-harness
  config show
  session start [--repo URL] [--workflow type] [--params JSON] [--persona-id ID] [--identity-id ID]
  session list [--status STATUS] [--limit N]
  session show <id>
  session delete <id>
  attach <session_id>
  logs tail [--session-id ID] [--job-id ID] [--level LEVEL] [--last N]
  logs delete [--session-id ID] [--job-id ID]
  workers list
  workers clear <worker_id>
  credentials show [--identity-id ID]
  credentials set [--identity-id ID] [--agent-token TOKEN] [--git-token TOKEN]
  api-key create [--label LABEL]
  api-key list
  api-key revoke <id>
  inbox send <agent_id> [--payload JSON] [--prompt TEXT] [--persona-id ID]
  inbox list <agent_id> [--limit N]
```

### API client (`crates/cli/src/api_client.rs`)
- HTTP client (reqwest) wrapping all control plane REST endpoints
- Add `Authorization: Bearer <key>` header
- Parse error responses into human-readable stderr output
- Handle connection failures: if control plane unreachable and wake_url configured, suggest "Wake up" action

### Error output
- All errors to stderr, human-readable: HTTP status + error.code + error.message
- Exit code 1 on failure
- No `--json` flag in v1

### Wake integration
- On connection failure: check if `wake_url` or `wake_script` configured
- If wake_url: suggest user run `remote-harness wake` or print "Control plane unreachable. Wake URL configured: {url}"
- Optional `wake` subcommand: HTTP GET to wake_url or exec wake_script

## Dependencies
- Task 02 (api-types — shared types)
- Task 05 (API key auth — CLI authenticates with API key)

## Test criteria
- [ ] `remote-harness --help` shows all commands
- [ ] `remote-harness config show` displays resolved config
- [ ] Config loads from env vars
- [ ] Config loads from YAML file
- [ ] Flag > env > config file precedence works
- [ ] API client sends auth header correctly
- [ ] Connection failure shows clear error message
- [ ] Error responses formatted as human-readable stderr
- [ ] `cargo build -p cli` produces working binary
- [ ] `cargo test -p cli` passes
