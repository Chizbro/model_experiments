# 15 - Worker Agent CLI Execution

## Goal
Implement the subprocess execution of Claude Code and Cursor CLIs. The worker spawns the agent CLI in the cloned repo, passes the prompt, captures output, and streams it back. Platform-specific handling is critical here.

## What to build

### Agent executor module (`crates/worker/src/agent_executor.rs`)

**CLI discovery**
- `find_agent_cli(agent_cli: AgentCli) -> Result<PathBuf>`
- Search PATH for:
  - Claude Code: `claude` binary
  - Cursor: `cursor` binary
- Return clear error if not found: "Claude Code CLI not found on PATH"

**CLI invocation**
- `run_agent(config: AgentRunConfig) -> Result<AgentOutput>`
- `AgentRunConfig`: agent_cli, agent_token, prompt, prompt_context (persona), work_dir, model (optional)
- Set environment for the subprocess:
  - Claude Code: `ANTHROPIC_API_KEY` = agent_token (or however Claude Code expects it)
  - Cursor: appropriate env var for Cursor authentication
- Build command:
  - Claude Code: `claude -p "{prompt}" --output-format stream-json` (non-interactive, piped mode)
  - Cursor: equivalent Cursor CLI invocation
- If persona prompt_context provided, prepend to the prompt or pass as system context
- Set working directory to cloned repo

**Output streaming**
- Capture stdout and stderr from the subprocess
- Parse streaming JSON output (Claude Code `stream-json` format)
- Extract assistant reply text from the output
- Buffer log entries and periodically send to control plane (via API client)
- Detect sentinel substring in output (for loop_until_sentinel)

**Platform-specific handling (`crates/worker/src/platform/`)**
- `mod.rs` with trait `PlatformHandler`
- `macos.rs` — Unix process spawning, stdout/stderr capture
- `linux.rs` — same as macOS with minor differences
- `windows.rs` — Windows process creation, quoting differences, console handling
- `wsl.rs` — WSL-specific: may need to invoke CLI through `wsl` command

### Agent output types
- `AgentOutput { exit_code, stdout, stderr, assistant_reply, sentinel_found }`
- `assistant_reply`: extracted text of the agent's response (no thinking/tool calls)

## Dependencies
- Task 13 (worker foundation — needs binary and config)
- Task 14 (git operations — agent runs in cloned repo)

## Design decisions
- Start with macOS/Linux support; Windows/WSL as separate platform modules
- Use `tokio::process::Command` for async subprocess
- Stream output line-by-line for real-time logging
- Kill subprocess on graceful shutdown (SIGTERM then SIGKILL after timeout)

## Test criteria
- [ ] Claude Code CLI discovered on PATH when installed
- [ ] Clear error when CLI not found
- [ ] Agent subprocess starts in correct working directory
- [ ] Environment variables set correctly for agent authentication
- [ ] Stdout captured and parsed from stream-json format
- [ ] Assistant reply text extracted (no thinking/tool calls)
- [ ] Sentinel substring detection works (case-sensitive by default)
- [ ] Non-zero exit code captured and reported
- [ ] Subprocess killed on shutdown signal
- [ ] Platform detection selects correct handler
- [ ] Unit tests for output parsing logic
- [ ] Unit tests for sentinel detection
- [ ] `cargo test -p worker` passes
