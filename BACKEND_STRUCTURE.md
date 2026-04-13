# Backend Structure

> How the Rust backend is organized in `src-tauri/src/`. Match these patterns when adding new code.

## Directory structure

```
src-tauri/src/
├── main.rs              # Binary entry point
├── lib.rs               # Tauri app init, IPC handler registration, schema migrations
├── agent/               # Agent execution
│   ├── mod.rs              # Agent model + runner entry point
│   ├── runner.rs           # Built-in LLM invocation, streaming
│   ├── external.rs         # Claude Code / Codex subprocess spawning, working-dir validation
│   ├── git_ops.rs          # Branch create/checkout, WIP commit, diff capture
│   └── merge.rs            # Branch merge, conflict resolution (manual / AI-assisted / auto)
├── commands/            # Tauri IPC handlers (~13 modules, one per domain)
├── db/                  # SQLite access
│   ├── mod.rs              # Connection pool, migrations, bootstrap
│   ├── queries.rs          # Project / piece / connection CRUD
│   ├── agent_queries.rs    # Agent history + token tracking
│   ├── plan_queries.rs     # Work plan CRUD
│   ├── cto_queries.rs      # CTO decision audit log
│   ├── goal_run_queries.rs # Goal-run persistence
│   └── artifact_queries.rs # Design docs + context summaries
├── llm/                 # LLM provider abstraction
│   ├── mod.rs              # Trait + factory
│   ├── claude.rs           # Anthropic API
│   └── openai_compat.rs    # OpenAI-compatible endpoints
├── models/              # serde data structures (mirror src/types in frontend)
│   ├── project.rs
│   ├── piece.rs
│   ├── connection.rs
│   ├── agent.rs
│   ├── work_plan.rs
│   ├── goal_run.rs
│   ├── cto_decision.rs
│   ├── artifact.rs
│   ├── runtime.rs
│   └── signoff.rs
└── test_support.rs      # Dev helpers
```

## Database
- **SQLite** via `rusqlite` 0.31 with the `bundled` feature.
- Single file at the OS-conventional app data dir (macOS: `~/Library/Application Support/project-builder-dashboard/data.db`).
- Schema versioned with `PRAGMA user_version`; migrations run from `db/mod.rs` on app start.
- Add a new migration as the next version step — never edit a prior one.

## IPC commands

All Tauri commands are registered in `lib.rs` and live under `commands/`. Domains:

- **Project** — create / read / update / delete / list
- **Piece** — create / update / delete / get
- **Connection** — create / update / delete
- **Agent** — run (built-in LLM or external CLI), fetch history
- **WorkPlan** — generate (Leader agent), get, list, update task status
- **CTO chat** — send, fetch history, execute actions, rollback
- **Merge** — merge branches, resolve conflicts, run integration review
- **Runtime** — detect, configure, start, stop, fetch status/logs
- **GoalRun** — create, update, fetch
- **Settings** — save API keys to keyring, fetch LLM config

When adding a command:
1. Define the request/response in `models/`
2. Add the handler under `commands/<domain>.rs`
3. Register it in `lib.rs`
4. Add a typed wrapper in `src/api/tauriApi.ts` and a TS type in `src/types`

## Agent runtime model

- A piece agent run is a **goal run**: branch off `main` → execute → stream output → commit on success → optionally merge back.
- Two execution backends:
  - **Built-in LLM** (`agent/runner.rs`) via the `llm/` trait
  - **External CLI** (`agent/external.rs`) — spawns Claude Code / Codex with validated working dir, streams stdout/stderr, enforces timeout
- Git operations isolated in `agent/git_ops.rs`.
- Merge + conflict resolution in `agent/merge.rs`.

## LLM provider trait
- Defined in `llm/mod.rs`. Implementations: `claude.rs`, `openai_compat.rs`.
- Add a new provider by implementing the trait and registering it in the factory.

## Async runtime
- `tokio` 1.x with full features. All IPC handlers are async.
- HTTP via `reqwest` 0.12 (json + stream features).

## Secrets
- API keys go in the OS keyring via the `keyring` crate. Never write them to the SQLite DB or any file.

## Logging
- `tracing` + `tracing-subscriber`. Dev-only output; do not log secrets or full prompt bodies.
