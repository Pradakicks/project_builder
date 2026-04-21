# Feature Brief Template

Use this template when turning an idea into an implementation brief that can be handed to an agent or subagent without extra context.

```md
---
id: <feature-slug>
title: <short feature name>
status: draft
owner: <team or agent>
---

# Goal

What problem this feature solves and why it matters.

# User-Visible Behavior

What a person should see or be able to do when it works.

# Acceptance Criteria

- ...
- ...

# Scenarios

- Happy path:
- Failure path:
- Recovery or repair path:

# Verification

- Command: `make verify-feature FEATURE=<feature-slug>`
- Automated checks:
- Manual checks:
- Evidence required:

# Risks And Constraints

- ...
- ...

# Out Of Scope

- ...
- ...

# Notes For Agents

- Reuse existing UI and backend contracts where possible.
- Keep the scope narrow enough to verify in one run.
- Preserve historical evidence, but make the current state obvious.
```
