#!/bin/bash
# Mock agent for integration testing.
# Reads prompt from args or stdin, echoes a response, and exits 0.
if [ -n "$1" ]; then
    prompt="$1"
else
    read -r prompt
fi
echo "Mock agent response to: $prompt"
echo "DONE"
exit 0
