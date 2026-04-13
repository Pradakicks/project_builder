# Frontend Guidelines

> How frontend code is organized and engineered in `src/`. Match these patterns; deviations need a reason.

## Directory structure

```
src/
├── api/         # Tauri IPC wrappers (tauriApi.ts) — call these, never invoke() directly
├── components/  # UI organized by feature area
│   ├── agents/      # Agents panel
│   ├── canvas/      # xyflow diagram editor
│   ├── chat/        # CTO chat
│   ├── editor/      # Piece / connection editor
│   ├── layout/      # Top-level shells, view routing
│   ├── leader/      # Leader plan + task cards
│   ├── projects/    # Projects list / create modal
│   ├── settings/    # API keys, runtime spec, LLM config
│   ├── ui/          # Shared primitives (Button, Markdown, …)
│   └── debug/       # Dev-only views
├── store/       # Zustand stores (one per domain)
├── hooks/       # Custom React hooks
├── types/       # TS interfaces — must mirror Rust models in src-tauri/src/models/
└── utils/       # devLog.ts (dev-only), helpers
```

## State management
- **Zustand** stores, one per domain. Existing stores:
  - `useAppStore` — view routing, global UI state
  - `useProjectStore` — open project, pieces, connections
  - `useChatStore` — CTO chat history + streaming state
  - `useLeaderStore` — current work plan, task run state
  - `useGoalRunStore` — goal-run lifecycle
  - `useAgentStore` — piece agent statuses
  - `useDebugStore`, `useDialogStore`, `useToastStore`
- Add a new store only when a domain doesn't fit into an existing one.
- Components subscribe to the slices they need; avoid pulling whole stores.

## Styling
- **Tailwind 4.2** utility classes only. No CSS modules, no styled-components, no inline `style={}` for anything tokenizable.
- Tailwind directives live in `src/index.css`.
- Dark theme is default; use Tailwind utilities directly (e.g. `bg-neutral-900 text-neutral-100`).
- See `DESIGN_SYSTEM.md` for tokens (currently a stub — extract in-use values when adding new components).

## Backend calls
- All Tauri IPC goes through `src/api/tauriApi.ts`. Never call `invoke(...)` from a component.
- Add a typed wrapper to `tauriApi.ts` for any new command, then expose it through the relevant store action.

## Types
- `src/types/index.ts` mirrors the Rust models in `src-tauri/src/models/`. Update both sides together.
- Prefer importing types from `src/types` over redeclaring them in components.

## Streaming
- LLM output is streamed via Tauri events. Subscribe in the store, write incrementally, render via `Markdown` (`src/components/ui/Markdown`).
- See `useChatStore` and `useLeaderStore` for the existing patterns.

## Logging
- Use `devLog` from `src/utils/devLog.ts`. It's a no-op in production builds — never use raw `console.log` in committed code.

## Component patterns
- Functional components only.
- Co-locate sub-components inside the feature directory; promote to `components/ui/` only when reused across features.
- Keep components focused; if a file passes ~300 lines, consider splitting.

## Accessibility
- Keyboard navigation and visible focus states are required for interactive elements.
- ARIA labels on icon-only buttons.
- This is enforced by the `CLAUDE.md` completion checklist.

## What not to do
- Don't bypass `tauriApi.ts` and call `invoke` directly.
- Don't introduce a second styling system alongside Tailwind.
- Don't add a new state library — use Zustand.
- Don't redeclare types that already exist in `src/types`.
