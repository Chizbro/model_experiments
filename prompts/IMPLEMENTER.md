Find the next task spec (you can sort them ascending, they are numbered) in @plan/ that is still marked 'pending.md'.

If there are no more pending tasks: your very first output must be exactly this single line (no other text, no markdown, no explanation before or after):
`{ "event": "terminate", "reason": "NO MORE JOBS" }`
Then stop. Do not run any tools or do any other work when there are no more jobs.

If there is a pending spec:
- Mark the task spec as in progress by renaming it from *.pending.md to *.processing.md
- Read the task spec
- Implement and test the task, refer to other docs if you need to
- Make sure the solution still builds and lints
- Make sure any migrations you added work
- Mark the task as complete by renaming to *.complete.md
- Dump your context to `logs/{task}.log`. DO NOT SUMMARIZE YOUR CONTEXT I WANT THE ENTIRE CONTENTX OF YOUR CONTENTS WINDOW