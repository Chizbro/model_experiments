#!/usr/bin/env bash
# Mock agent CLI for integration tests.
#
# Simulates Claude Code / Cursor CLI behavior:
#   - Reads the prompt from args (--prompt "...")
#   - Outputs a configurable response to stdout
#   - Exits with a configurable exit code
#
# Configuration via environment variables:
#   MOCK_AGENT_OUTPUT   — text to print to stdout (default: "mock agent output")
#   MOCK_AGENT_EXIT     — exit code (default: 0)
#   MOCK_AGENT_STDERR   — text to print to stderr (default: empty)
#   MOCK_AGENT_DELAY    — seconds to sleep before responding (default: 0)
#
# Usage (by worker):
#   MOCK_AGENT_OUTPUT="task DONE" ./mock_agent.sh --prompt "Check status"

set -euo pipefail

# Optional delay to simulate execution time
if [ -n "${MOCK_AGENT_DELAY:-}" ] && [ "$MOCK_AGENT_DELAY" != "0" ]; then
    sleep "$MOCK_AGENT_DELAY"
fi

# Print stderr if configured
if [ -n "${MOCK_AGENT_STDERR:-}" ]; then
    echo "$MOCK_AGENT_STDERR" >&2
fi

# Print output
echo "${MOCK_AGENT_OUTPUT:-mock agent output}"

# Exit with configured code
exit "${MOCK_AGENT_EXIT:-0}"
