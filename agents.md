# Agent Working Rules

> **Note:** This file is kept in sync with `CLAUDE.md`. When you update one, update the other.

## Persistent Roadmap State

- `docs/next-steps.md` is the canonical roadmap. It is always up to date.
- When a task is completed, mark it `[x]` in `docs/next-steps.md` before the task is considered done.
- When new work is identified during implementation (bugs, follow-ups, gaps), add it to the appropriate section in `docs/next-steps.md` immediately.
- Never leave roadmap progress only in conversation context — if it happened, it must be reflected in the file.

## Live Documentation Philosophy

Documentation and machine-readable state files must always reflect reality. They are not write-once artifacts — they are live records that must be updated whenever anything changes that would affect their accuracy.

- If you write a file that describes how to run the project (e.g. `runtime.json`), you must update it whenever you change anything that affects how the project is run: dependencies, entry point, port, environment variables, build steps.
- If you add a feature that changes the architecture, update any design docs or spec files that describe that area.
- Never leave a documentation or state file out of sync with the code. A stale spec is worse than no spec — it actively misleads.
- The same rule applies to the roadmap (`docs/next-steps.md`), runtime specs, design docs, and any other persistent state the system relies on.

The underlying principle: **agents are the authors of their own memory. If you built it, you own keeping the record accurate.**

### This philosophy applies at two levels

**Level 1 — Us, building project builder.** The rules above apply to this codebase: keep `docs/next-steps.md` current, keep specs in sync with code, commit documentation alongside the changes that necessitate it.

**Level 2 — The agents inside project builder.** The CTO agent, implementation agents, and any future team agents should have this philosophy programmed into their behavior. When we build or update their prompts, we must encode this: agents that make changes are responsible for updating any relevant docs, specs, or state files in the same action. This is a core product design principle, not just a development convention. The goal is agents that maintain an accurate, living record of the project they're building — so any agent (or the user) can pick up context at any time without relying on conversation history.

## Commit Policy

- Every code or documentation change made by the coding agent must be committed before the task is considered complete.
- Use one commit per coherent change set.
- Prefer conventional, imperative commit messages that describe the user-visible or engineering outcome.
- Keep commits scoped, reviewable, and bisect-friendly.
