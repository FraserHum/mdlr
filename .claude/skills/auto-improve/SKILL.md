---
name: auto-improve
description: Use whenever the user requests you to use mdlr to auto improve the codebase.
---

# Overview:

Use the `mdlr` tool to help you improve the modularity of the code base.

**IMPORTANT**: Never use cargo run. Always use `task link` to build `mdlr` and then use `mdlr` directly as a binary.

1. Run `mdlr prompt` and follow the prompt. Make sure to create a task list for any large changes.
2. **IMPORTANT**: You MUST commit all changes after finishing. Do not skip this step. Do not end without committing.

## Commit messages

The commit message MUST include "auto" somewhere. Use the format above, substituting the metric name and target you improved.

```bash
git add <changed files>
git commit -m "refactor: auto-reduce <metric> of <target>"
```
