---
trigger: model_decision
description: Use for implementation, debugging, architectural changes, or multi-file code changes where an external implementation plan would be useful. Do not use for trivial text-only edits.
---

# DeepSeek Planner Rule

For non-trivial implementation tasks in this repository:

1. First inspect the repository yourself and identify the files and code relevant to the task.
2. Summarize only the context necessary for implementation planning.
3. Call the `anthro-bridge/plan` MCP tool exactly once with:
   - the user's task;
   - the relevant repository context;
   - important constraints.
4. Use the returned DeepSeek plan as the basis for implementation.
5. Perform file edits, builds, and tests yourself.
6. Do not call `anthro-bridge/plan` again unless the implementation encounters a major unresolved problem.
7. Do not ask the user to repeat this workflow.
8. Do not use the planner for trivial tasks such as a one-word text change unless planning would materially help.
