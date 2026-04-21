# Development Loop

Use the captured desktop session workflow when iterating on CTO, IPC, task, merge, or review failures.

The current dev baseline also includes goal-run and runtime orchestration. When debugging “one prompt to running app” work, inspect both the CTO decision trail and the active goal-run/runtime state before changing prompts or planner behavior. The Delivery tab is the canonical run-health view: it now surfaces blocking truth, retry state, generated-file evidence, git evidence, and runtime evidence from the backend snapshot.

## Agent Verification Loop

For feature work that needs a tighter implementation-and-verification cycle, use the briefs and runbook in `docs/verification/`:

- `docs/verification/features/feature-brief-template.md`
- `docs/verification/features/forced-fail-repair.md`
- `docs/verification/agent-loop.md`

The default local gate is `make verify`. For a single feature brief or scenario, use `make verify-feature FEATURE=<feature-slug>`.

When a CTO response uses `createPiece` plus `runPiece`, also inspect the created piece's `agentPrompt`, `executionEngine`, and generated-file artifacts. That path is now the shortest verified bridge from chat actions to real repo mutations.

## Start A Captured Session

Run the standard desktop loop with log capture:

```bash
make dev-session
```

Run the native Tauri shell against the containerized frontend with the same capture flow:

```bash
make dev-session-host
```

Each run creates `.debug-sessions/<session-id>/` with:

- `desktop.log`: combined `tauri dev` output
- `session.json`: session metadata
- `latest-scenario.json`: the most recent captured CTO failure/rejection artifact

Tail the current session:

```bash
make dev-session-tail
```

## In-App Diagnostics

In development builds, the app shows a **Dev Diagnostics** button in the lower-right corner.

The panel exposes:

- recent frontend log and IPC events
- current debug session metadata
- the latest captured CTO failure/rejection scenario
- current goal-run phase/status and runtime summary
- a tail of the current captured desktop log
- a copied JSON debug report for pasting into follow-up investigations

## CTO Failure Workflow

When a CTO request fails or a reviewed decision cannot be logged, the app captures:

- the user prompt
- the conversation that led to the request
- the assistant response
- the parsed CTO review result
- the decision payload submitted to `log_cto_decision`
- the returned error

That scenario is visible in the diagnostics panel and is also written to `latest-scenario.json` when the app is running under `make dev-session`.

## Replay

The diagnostics panel can replay the latest captured CTO scenario against the currently open project.

If the captured scenario belongs to another project, the panel will open that project first. Replay becomes available as soon as the CTO panel for that project has loaded.

## Current First-Class Regression

The current diagnostics workflow includes a regression for the `log_cto_decision` payload mismatch where Rust expected `piece_id`/`plan_id` while the frontend sent `pieceId`/`planId`.

Keep adding real desktop failures to this loop:

1. reproduce under `make dev-session`
1. inspect the diagnostics panel and session files
1. fix the contract or flow
1. add a regression before moving on
