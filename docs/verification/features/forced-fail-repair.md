# Forced Fail Repair

## Goal

Make the forced-fail smoke scenario understandable and recoverable from inside the app without the operator needing to guess what is current versus historical.

## User-Visible Behavior

- The Delivery view clearly shows the current blocker for the blocked verification run.
- Older warnings and failures remain visible as history, but they are labeled as historical evidence.
- Clicking `Resume with repair` requests a fresh operator repair attempt even when automatic retries are exhausted.
- The app shows a persistent receipt or status update for that action instead of relying only on toast feedback.

## Acceptance Criteria

- A blocked forced-fail run is recognizable from the Delivery panel alone.
- The current blocker is visually distinct from prior warnings and prior repair attempts.
- The `Resume with repair` action creates a fresh repair-requested outcome.
- The runtime/verification state updates are visible without refreshing the page manually.
- The evidence bundle or diagnostics view makes it obvious which events are old and which are new.

## Scenarios

- Happy path:
  - Open the forced-fail run and confirm the current blocker is the fatal log scan.
  - Click `Resume with repair`.
  - Confirm the app records a fresh repair request and surfaces the new status.
- Recovery path:
  - If the repair attempt is skipped or fails, confirm the UI says why and keeps the current blocker explicit.
- Historical-state path:
  - Ensure older warnings stay visible, but are marked as prior evidence rather than the present blocker.

## Verification

- Command: `make verify-feature FEATURE=forced-fail-repair`
- Check that the live run state and the historical log trail are both visible.
- Check that the latest repair action is recorded as a new event or receipt.
- Capture screenshots and log evidence if the scenario fails.

## Risks And Constraints

- The UI can become confusing if historical messages and current blockers are not labeled differently.
- Verification should not depend on live LLM calls or API keys.
- The scenario must remain deterministic enough for repeated agent repair loops.
