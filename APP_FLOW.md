# App Flow

> How users move through the app, screen-by-screen. Source: `README.md`, `docs/architecture.md`, `src/components/`.

## Top-level views
1. **Projects view** — list, create, rename, delete projects
2. **Editor view** — canvas + side panels for one open project
3. **Settings view** — API keys, LLM config, runtime spec

View routing is handled in `src/store/useAppStore.ts` (Zustand).

## Editor layout
- **Center**: canvas (xyflow/react)
- **Left tabs**: CTO chat • Leader plan • Agents panel • Decisions audit log
- **Right**: piece/connection editor (when a node/edge is selected)

## Primary flow

### 1. Setup
1. Launch app → Projects view (empty on first run)
2. **Create Project** modal → name, description, parent folder
3. Backend creates a git repo in the chosen folder + initial commit
4. Navigate into the project → Editor view

### 2. Design phase
1. Drag on the canvas to create pieces; configure name/type/responsibilities/interfaces in the right panel
2. Draw edges between pieces; add labels and constraints
3. Open **CTO chat**; prompt e.g. "add a piece for authentication"
4. CTO streams its analysis and proposes actions
5. Per the phase-control policy (manual / gated / autonomous), user reviews and confirms; CTO executes the actions and updates the diagram
6. Every action is written to the CTO Decisions audit log with rollback metadata

### 3. Plan phase
1. User clicks **Generate Plan** (or CTO triggers via action)
2. Leader agent reads the full diagram and produces a structured JSON work plan
3. Frontend renders a plan summary plus collapsible task cards, color-coded by priority

### 4. Execution phase
1. User approves the plan; tasks unlock
2. **Run ▶** on a task spawns the assigned piece agent (built-in LLM or external CLI)
3. Agent output streams inline in the task card
4. On success, the task auto-completes and the piece phase auto-advances (in gated/autonomous mode)
5. On failure, an inline feedback textarea appears; user can retry or skip
6. **Run All ▶** runs pending tasks sequentially, stopping on the first failure
7. Each run gets its own git branch; successful runs auto-commit

### 5. Integration phase
1. Once tasks complete (or manually triggered), piece branches merge back to `main`
2. Conflicts are resolved manually, with AI assistance, or automatically
3. Integration review agent checks cross-piece API/data/config consistency
4. Review streams as markdown in the LeaderPanel

### 6. Runtime phase
1. Project settings configure install / run / readiness / verify commands (auto-detected or manual)
2. **Start Runtime** spawns the subprocess and streams output
3. The built app URL is fetched (config or auto-detected port) and embedded
4. **Verify** runs the validation command
5. On success, the goal run is marked complete

## Cross-cutting screens
- **Agents panel**: every piece grouped by execution state (idle / running / success / failure / validation-failed)
- **Canvas**: each piece node shows a phase pill + status indicator
- **CTO Decisions tab**: full audit log with review / execution / rollback metadata
