#!/usr/bin/env bash
set -euo pipefail

# Usage: run_agent_loop.sh ITERATIONS [MODEL]
#   ITERATIONS - positive integer, number of times to run the agent with the prompt
#   MODEL      - optional model name (default: claude-opus-4-6)
# Prompt is read from stdin (e.g. cat prompt.txt | ./run_agent_loop.sh 5 opus)
# Each run: prompt is piped to claude; streaming JSON output is checked for terminate marker.
#
# Loop stops early if the assistant output contains: { "event": "terminate", "reason": "NO MORE JOBS" }
# Only assistant lines are checked so the same text in the prompt (user message) does not trigger stop.

usage() {
  echo "Usage: $0 ITERATIONS [MODEL]" >&2
  echo "  ITERATIONS  number of iterations (positive integer)" >&2
  echo "  MODEL       model name (default: claude-opus-4-6)" >&2
  echo "" >&2
  echo "Example: cat prompt.txt | $0 5 opus" >&2
  exit 1
}

[[ $# -ge 1 && $# -le 2 ]] || usage

ITERATIONS="$1"
MODEL="${2:-claude-opus-4-6}"

# Validate iterations is a positive integer
if ! [[ "$ITERATIONS" =~ ^[1-9][0-9]*$ ]]; then
  echo "Error: ITERATIONS must be a positive integer, got: $ITERATIONS" >&2
  usage
fi

# Read entire prompt from stdin
PROMPT=$(cat)

# Terminate marker: agent should emit this (exactly) when there are no more jobs.
# We look for it inside stream-json "assistant" lines only (so the prompt's own text doesn't trigger).
TERMINATE_MARKER='{ "event": "terminate", "reason": "NO MORE JOBS" }'
# Relaxed regex for matching (agent may omit spaces; in stream the text can be JSON-escaped).
TERMINATE_MARKER_FLEX='event.*terminate.*reason.*NO MORE JOBS'

run_agent() {
  # Pipe prompt to claude in print mode with streaming JSON output.
  # Tee raw stream-json to a checker to detect the terminate marker.
  # Only consider assistant lines so we don't stop on the prompt (user message).
  printf '%s' "$PROMPT" | claude \
    -p \
    --model "$MODEL" \
    --output-format stream-json \
    --dangerously-skip-permissions \
    --verbose \
  | tee >(while IFS= read -r line; do
    # Must be an assistant line to avoid matching the prompt
    [[ "$line" != *'"type":"assistant"'* ]] && continue
    # Check if this line contains the terminate payload (exact or flexible)
    if [[ "$line" == *"$TERMINATE_MARKER"* ]] || [[ "$line" =~ $TERMINATE_MARKER_FLEX ]]; then
      touch "$STOP_FLAG"
    fi
  done) | \
  agent-pretty-print
}

STOP_FLAG=$(mktemp)
trap 'rm -f "$STOP_FLAG"' EXIT

for (( i = 1; i <= ITERATIONS; i++ )); do
  echo "--- iteration $i/$ITERATIONS ---" >&2
  rm -f "$STOP_FLAG"
  run_agent
  if [[ -f "$STOP_FLAG" ]]; then
    echo "Stopping early: agent emitted terminate (NO MORE JOBS)." >&2
    break
  fi
  echo "" >&2
done

[[ -f "$STOP_FLAG" ]] && RUNS=$i || RUNS=$ITERATIONS
echo "Done ($RUNS iteration(s))." >&2
