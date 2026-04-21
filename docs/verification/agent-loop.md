# Agent Feedback Loop

This repo uses a staged loop for turning an idea into a verified change:

1. Write a feature brief.
2. Implement the scoped change.
3. Run the verification gate.
4. Inspect the evidence bundle.
5. Repair failures up to the allowed budget.
6. Commit only after the run is clean or the blocker is fully understood.

## How To Use It

Start with a brief in `docs/verification/features/` or copy the template in `docs/verification/features/feature-brief-template.md`.

Keep the brief small and decision-complete:

- the user-visible goal
- the acceptance criteria
- the scenarios to seed or exercise
- the evidence expected from a successful run
- the failure states that must stay understandable

For the first proving loop, use the forced-fail repair scenario:

- brief: `docs/verification/features/forced-fail-repair.md`
- local verification: `make verify`
- feature-specific verification: `make verify-feature FEATURE=forced-fail-repair`

## What The Loop Should Produce

The agent loop should leave behind:

- a clear current state in the UI
- historical evidence that stays visible but is labeled as historical
- a structured verification artifact bundle
- a final commit once the change is verified

## Roadmap Notes

### Native Tauri WebDriver

This is the later confidence tier for proving the real desktop shell and IPC bridge. It belongs on the roadmap, not the V1 gate.

Why it matters:

- it exercises the actual Tauri desktop app instead of a browser harness
- it verifies native window startup and real command wiring
- it reduces the gap between “passed in test” and “passed in app”

What it should cover when added:

- app launch
- project loading
- rendering the Delivery view for a seeded run
- invoking `Resume with repair`
- observing the resulting real IPC/state change

Initial scope:

- target Linux CI first
- keep the suite smoke-level
- do not block V1 on macOS WebDriver support

### Live LLM Smoke

This is an optional confidence check that exercises a real model/provider path without making it part of the required verification gate.

Why it matters:

- mocks do not catch API-key setup problems
- provider request/response shapes can drift
- prompt changes can fail only with a real model
- latency, timeout, and rate-limit behavior are often invisible in mocked tests

What it should cover when added:

- provider call succeeds
- response parses
- the app handles accepted or rejected actions cleanly
- no secrets leak into logs or artifacts

Initial scope:

- opt-in only
- separate command from `make verify`
- strict timeout and low token budget
- own evidence bundle
- never assert exact model wording

Why it stays roadmap-only for now:

- it is nondeterministic
- it can cost money
- it can fail for provider/network reasons unrelated to the app
- the main feedback loop should remain fast and repeatable
