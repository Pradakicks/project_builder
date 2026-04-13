# Implementation Plan

> Phased build plan. Reference phase numbers from `progress.txt`. Canonical roadmap detail lives in `docs/next-steps.md`; this file is the high-level phase map.

## Phase 0 — Foundations ✅ COMPLETE
- Tauri shell, SQLite schema + migrations, project CRUD, settings (API keys via OS keyring), error handling, save/load dialogs, canvas persistence, project rename.

## Phase 1 — Single-piece agent execution ✅ COMPLETE
- One piece, one agent, streaming output, git branch + auto-commit on success.

## Phase 2 — Multi-piece orchestration ✅ COMPLETE
- Leader agent generates structured work plans
- Task cards with `Run ▶` and `Run All ▶`
- Sequential task execution with stop-on-failure
- Inline failure feedback + retry
- Branch merge & integration review (manual / AI-assisted / auto conflict resolution)

## Phase 3 — CTO agent ✅ COMPLETE
- CTO chat with streaming, full diagram + history context
- Review-gated actions (create / update / connect / delete)
- Phase control policies: manual / gated / autonomous
- Audit log with rollback metadata

## Phase 4 — Runtime ✅ COMPLETE
- Auto-detect install / run / verify commands (static patterns + LLM agent fallback)
- Manual runtime spec override
- Embedded subprocess + streamed logs + readiness probe
- Verify command + goal-run completion
- Blocked-state UI when detection fails

## Phase 5 — Live documentation enforcement ✅ COMPLETE
- Agents update `runtime.json`, design docs, and `docs/next-steps.md` alongside code changes
- Documented in `AGENTS.md` / `CLAUDE.md`

## Phase 6 — Polish & robustness 🚧 IN PROGRESS
See `docs/next-steps.md` for the live checklist. Highlights:
- [ ] Real placeholder icons (currently solid-blue PNGs)
- [ ] End-to-end smoke test with `make dev`
- [ ] Validate container workflow end-to-end
- [ ] Reduce host-side Tauri dependency footprint or document it
- [ ] Interrupted-run recovery (mark runs started but not completed when app closes)
- [ ] Real-time agent activity feed
- [ ] Design doc generation during the Design phase
- [ ] Extract Tailwind tokens into `DESIGN_SYSTEM.md`

## Phase 7 — Future vision (not started)
- Specialized sub-agents per piece (implementation / testing / review)
- Agent-to-agent communication
- Persistent long-lived agent processes with lifecycle management
- Token budgeting hierarchy
- Monitoring dashboard
- 24/7 continuous operation mode
