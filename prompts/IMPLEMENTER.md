Find the next feature spec (you can sort them ascending, they are numbered) in @plan/ that is still marked 'pending.md'.

If there are no more pending tasks: your very first output must be exactly this single line (no other text, no markdown, no explanation before or after):
`{ "event": "terminate", "reason": "NO MORE JOBS" }`
Then stop. Do not run any tools or do any other work when there are no more jobs.

If there is a pending spec:
- Mark the feature spec as in progress by renaming it from *.pending.md to *.processing.md
- Read the @docs/TECH_STACK.md and @plan/design-system.md
- Read the feature spec
- Implement and test the feature
- Make sure the solution still builds and lints
- Make sure any migrations you added work
- Mark the feature as complete by renaming to *.complete.md
- Dump your context to `logs/{feature}.log`. DO NOT SUMMARIZE YOUR CONTEXT I WANT THE ENTIRE CONTENTX OF YOUR CONTENTS WINDOW