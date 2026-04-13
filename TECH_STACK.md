# Tech Stack

> Exact versions. Update whenever a dependency changes. Source: `package.json`, `src-tauri/Cargo.toml`, `vite.config.ts`, `src-tauri/tauri.conf.json`.

## Application shell
- **Tauri** 2.x (Rust backend + system webview)
- **Node.js + npm** for the frontend toolchain
- **Rust** stable toolchain for the backend

## Frontend
- **React** 19.2.4
- **TypeScript** 5.9.3
- **Vite** 7.3.1
- **Tailwind CSS** 4.2.1 (via `@tailwindcss/vite` 4.2.1)
- **@xyflow/react** 12.10.1 — canvas / diagram editor
- **Zustand** 5.0.11 — state management
- **react-markdown** 10.1.0 + **remark-gfm** 4.0.1 — LLM output rendering
- **@tauri-apps/api** 2.10.1
- **@tauri-apps/plugin-dialog** 2.6.0

## Backend (Rust)
- **rusqlite** 0.31 (bundled SQLite)
- **tokio** 1.x (full features)
- **reqwest** 0.12 (json + stream)
- **serde** 1.x + **serde_json** 1.x
- **uuid** 1.x (v4)
- **chrono** 0.4 (serde feature)
- **keyring** 3.x — OS credential store
- **tracing** 0.1 + **tracing-subscriber** 0.3
- **async-trait** 0.1
- **futures** 0.3

## Storage
- **SQLite** via rusqlite, bundled
- DB path (macOS): `~/Library/Application Support/project-builder-dashboard/data.db`
- Schema versioned via `PRAGMA user_version`

## LLM providers
- Anthropic API (Claude) — `src-tauri/src/llm/claude.rs`
- OpenAI-compatible endpoints — `src-tauri/src/llm/openai_compat.rs`
- External coding CLIs spawned as subprocesses: Claude Code, Codex
