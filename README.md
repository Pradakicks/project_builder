# Project Builder

*Map a software project as a system, hand work to AI agents, and push it all the way toward a running app.*

## Why I Built This

I built this because I got frustrated with one-shot AI coding chats. They can be great at writing a file or fixing a bug, but they usually fall apart when the work starts looking like an actual project with moving parts, handoffs, runtime setup, review steps, and long-lived context.

I wanted something that felt more like steering a small software team than prompting a chatbot. I wanted to see the architecture, break work into pieces, let different agents handle different scopes, keep the important decisions reviewable, and still have a clear path from "build this" to "okay, now run it and prove it works."

## What It Does

Project Builder is a desktop app for orchestrating AI-driven software work.

You model a project as connected pieces on a canvas, chat with a CTO-style agent about changes, generate a work plan from the diagram, and run piece-level agents to do the implementation. Those runs can use the built-in LLM path or external tools like Claude Code and Codex. The app streams output live, keeps history in SQLite, creates per-piece git branches, auto-commits successful runs, and shows the delivery path in one place.

Right now, the main flow looks like this:

- Create a repo-backed project from the app.
- Break it into pieces and connections on a visual canvas.
- Use the CTO chat to propose changes through reviewable action blocks.
- Turn the diagram into a structured work plan with the Leader agent.
- Run tasks one by one or sequentially.
- Detect, start, and verify the generated app from inside the desktop UI.

It’s mainly for people who want to direct software projects at the system level instead of living inside an IDE full-time.

## Tech Stack

- Tauri v2 for the desktop shell
- React 19 + TypeScript + Vite 7 for the frontend
- Tailwind CSS v4 for styling
- `@xyflow/react` for the diagram canvas
- Zustand for frontend state
- Rust + Tokio on the backend
- SQLite via `rusqlite` for persistence
- `reqwest` for LLM/runtime networking
- `keyring` for OS-backed API key storage
- `react-markdown` + `remark-gfm` for agent output rendering
- Optional external execution through Claude Code and Codex CLIs

## Getting Started

### Prerequisites

- Node.js
- Rust
- Git

If you want to use external execution engines, you’ll also want `claude` and/or `codex` on your `PATH`.

### Install

```bash
make setup
```

That installs the frontend deps and fetches the Rust crates.

### Run the app

```bash
make dev
```

Useful alternatives:

```bash
npm run dev          # frontend only
make check           # TypeScript + Rust checks
make dev-session     # desktop dev run with captured logs
```

### API keys

The app stores provider keys in the OS keychain from the Settings screen. It also supports environment variable fallback:

- `ANTHROPIC_API_KEY`
- `OPENAI_API_KEY`
- `LLM_API_KEY`

Inside the container workflow, env vars are the safer bet than keychain integration.

### Container workflow

If you want the hybrid container setup that ships with the repo:

```bash
make container-up
make container-frontend
make host-tauri-dev
```

### First run

Create a project from the Projects screen, choose a parent folder, and the app will create a repo-backed working directory with an initial `main` commit for you.

## What's Next

- [ ] Persistent multi-agent teams with pause/resume and crash recovery
- [ ] Agent-to-agent coordination across pieces
- [ ] Richer live agent status and monitoring, not just running/failed
- [ ] 24/7 continuous operation for long-lived projects
