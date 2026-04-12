# Project Builder Dashboard

Desktop app for composing projects, pieces, agents, plans, and CTO-driven workflows.

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

Build the frontend bundle:

```bash
npm run build
```

Run Rust checks and tests:

```bash
cd src-tauri
cargo check
cargo test
```

## Data Location

The app stores its SQLite database at:

- macOS: `~/Library/Application Support/project-builder-dashboard/data.db`
- Linux: `~/.local/share/project-builder-dashboard/data.db`
- Windows: `%APPDATA%\project-builder-dashboard\data.db`

The database bootstrap is versioned with `PRAGMA user_version`, and startup migrations are idempotent. Existing databases are upgraded in place.

## Troubleshooting

- If the desktop app fails to start, check the terminal output for the first Rust error and then rerun `cd src-tauri && cargo check`.
- If the UI dev server port is busy, stop the other process using port `5174`.
- If you need a clean local database, delete the `data.db` file at the path above and relaunch the app.
- If Tauri cannot find a working directory or keyring backend, verify the project permissions and platform keychain access.

