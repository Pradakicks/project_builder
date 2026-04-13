# Project Builder Dashboard

Desktop app for composing projects, pieces, agents, plans, and CTO-driven workflows.

The current delivery baseline is no longer just diagram editing: each project can now track a top-level goal run, operate in `manual`, `guided`, or `autopilot` mode, and use a normalized runtime spec to start and verify a generated app from inside the desktop UI.

## Setup

1. Install Node.js and Rust.
1. Install frontend dependencies:

```bash
npm install
```

1. Make sure the Rust toolchain is available in `src-tauri`.

## Run

Start the frontend-only dev server:

```bash
npm run dev
```

Start the Tauri desktop app:

```bash
npm run tauri dev
```

Start a captured desktop debug session:

```bash
make dev-session
```

That workflow writes session logs and the latest captured CTO failure artifact into `.debug-sessions/` and enables the in-app **Dev Diagnostics** panel. The panel can copy a structured debug report and replay the latest captured CTO scenario once the relevant project is open.

Build the frontend bundle:

```bash
npm run build
```

The app now lazy-loads the main views and the editor/chat/plan surfaces, so a cold start only fetches the shell plus the active view chunk. The first time you open the editor, CTO chat, or work plan on a fresh session, the app may briefly show a loading state while that chunk is fetched.

Run Rust checks and tests:

```bash
cd src-tauri
cargo check
cargo test
```

Run the focused regression matrix:

```bash
# Frontend build / typecheck coverage
npm run build

# Rust orchestration, merge, migration, and recovery coverage
cd src-tauri
cargo test --lib -- --test-threads=1
```

## Data Location

The app stores its SQLite database at:

- macOS: `~/Library/Application Support/project-builder-dashboard/data.db`
- Linux: `~/.local/share/project-builder-dashboard/data.db`
- Windows: `%APPDATA%\project-builder-dashboard\data.db`

The database bootstrap is versioned with `PRAGMA user_version`, and startup migrations are idempotent. Existing databases are upgraded in place.

## Troubleshooting

- If the desktop app fails to start, check the terminal output for the first Rust error and then rerun `cd src-tauri && cargo check`.
- For fast CTO/IPC debugging, prefer `make dev-session` over raw `npm run tauri dev`, then inspect the in-app **Dev Diagnostics** panel and `.debug-sessions/current/`.
- If the UI dev server port is busy, stop the other process using port `5174`.
- If a view opens with a short loading spinner, that is expected. Projects, settings, the editor shell, and the heavier CTO/plan panels are split into separate runtime chunks.
- If you need a clean local database, delete the `data.db` file at the path above and relaunch the app.
- If Tauri cannot find a working directory or keyring backend, verify the project permissions and platform keychain access.

## CTO Actions

CTO chat is review-gated.

- The model should emit fenced ` ```action ` blocks, one JSON object per block.
- The frontend validates and reviews those blocks before execution.
- Invalid or malformed blocks are rejected, preserved in the audit log, and shown in the Decisions tab.
- A simple inline `action { ... }` block can still be recovered as a fallback, but it is not the supported contract.

Each CTO decision now stores structured audit data:

- assistant text and normalized actions
- validation errors
- execution steps and errors
- rollback metadata for the reversible action subset

Rollback is exposed only for the safest reversible CTO actions. Destructive or ambiguous actions remain non-rollbackable.

The CTO prompt also includes the current goal-run and runtime context, and it can now use runtime-oriented actions such as `configureRuntime`, `runProject`, `stopProject`, and `retryGoalStep` when the project has enough structure to continue autonomously.

It can also create a more concrete implementation piece in one step by attaching `agentPrompt`, `outputMode`, and `executionEngine` to `createPiece`, then dispatch that work immediately with `runPiece`. That is the current shortest path from a reviewed CTO response to real files appearing in the working directory.

## Goal Runs And Runtime

The app now persists a per-project goal-run record for top-level prompts such as “create a simple todo web app.” A goal run tracks:

- prompt, phase, and status
- linked plan id
- retry count and last failure summary
- runtime status summary
- verification summary

In `autopilot` mode, a successful reviewed CTO action can now chain into:

1. plan generation or reuse
1. plan approval
1. task execution
1. merge and integration review
1. runtime detection/configuration
1. runtime start and verification

This does not guarantee a fully working app for every prompt yet, but it does give the product a real end-to-end control loop instead of a chat-only handoff.

Runtime configuration is stored on project settings and currently supports:

- install command
- run command
- readiness check
- verify command
- stop behavior
- app URL / port hint

If no runtime spec is configured, the desktop app will first try to auto-detect one from the working directory before blocking the goal run.

## Operator Runbook

For task failures, malformed CTO responses, merge conflicts, rollback guidance, and local reset/recovery steps, see [docs/operator-runbook.md](./docs/operator-runbook.md).

## Development Workflow

For the captured desktop debugging loop, scenario replay behavior, and the current `log_cto_decision` regression workflow, see [DEVELOPMENT.md](./docs/DEVELOPMENT.md).

## Expanded Test Matrix

The current regression suite covers:

- deterministic project bootstrap and rollback
- external agent runs writing real files into the repo working directory
- schema upgrade/idempotency for local SQLite databases
- happy-path plan generation, execution, merge, and integration review
- failed external execution and retry/recovery
- manual merge conflict handling
- CTO action parsing for valid `generatePlan` blocks and malformed fenced action output

When adding new behavior, prefer a focused Rust test or Vitest regression over relying on manual verification.

## External Run Evidence

Successful external piece runs now persist a `generated_files` artifact alongside the existing git metadata. In the Piece editor's Agent tab, you can inspect:

- branch and commit SHA
- diff summary
- generated file listing captured from the piece branch

This is the first baseline proof that the system wrote real files into the project working directory.
