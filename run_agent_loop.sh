#!/usr/bin/env bash
set -euo pipefail

# Usage: run_agent_loop.sh ITERATIONS
#   ITERATIONS - positive integer, number of times to run the agent with the prompt
# Prompt is read from stdin (e.g. cat prompt.txt | ./run_agent_loop.sh 5)
# Each run: prompt is piped to agent; raw agent (stream-json) is checked for terminate, then
# passed to agent-pretty-print; pretty-printed output goes to stdout.
#
# Loop stops early if the raw agent stream (before agent-pretty-print) contains a stream-json
# line with type=assistant whose content includes: { "event": "terminate", "reason": "NO MORE JOBS" }
# Only assistant lines are checked so the same text in the prompt (user message) does not trigger stop.

usage() {
  echo "Usage: $0 ITERATIONS" >&2
  echo "  ITERATIONS  number of iterations (positive integer)" >&2
  echo "" >&2
  echo "Example: cat prompt.txt | $0 5" >&2
  exit 1
}

[[ $# -eq 1 ]] || usage

ITERATIONS="$1"

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
  # Tee raw stream-json to a checker (so we match before agent-pretty-print reformats it).
  # Cursor CLI emits NDJSON: agent text is in type=assistant lines (message.content[].text), not as a raw line.
  # Only consider assistant lines so we don't stop on the prompt (user message) which also contains the instruction.
  printf '%s' "$PROMPT" | agent -f --model auto --print --stream-partial-output --output-format stream-json | \
  tee >(while IFS= read -r line; do
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
