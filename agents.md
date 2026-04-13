# Agent Working Rules

> **Note:** This file is kept in sync with `CLAUDE.md`. When you update one, update the other.

## Persistent Roadmap State

- `docs/next-steps.md` is the canonical roadmap. It is always up to date.
- When a task is completed, mark it `[x]` in `docs/next-steps.md` before the task is considered done.
- When new work is identified during implementation (bugs, follow-ups, gaps), add it to the appropriate section in `docs/next-steps.md` immediately.
- Never leave roadmap progress only in conversation context — if it happened, it must be reflected in the file.

## Commit Policy

- Every code or documentation change made by the coding agent must be committed before the task is considered complete.
- Use one commit per coherent change set.
- Prefer conventional, imperative commit messages that describe the user-visible or engineering outcome.
- Keep commits scoped, reviewable, and bisect-friendly.
