use std::time::Duration;
use tauri::{AppHandle, Manager};
use tracing::info;

use crate::commands::goal_run_executor::spawn_goal_run_executor;
use crate::AppState;

/// Tick interval for the backoff scheduler. Keeps a safe 2x gap under the
/// 30s stale-heartbeat threshold so we never race the interrupt sweeper.
const SCHEDULER_TICK_SECS: u64 = 10;

/// Spawn the backoff scheduler. Call once from `Builder::setup`.
///
/// Wakes runs whose `retry_backoff_until` has elapsed and re-enters the
/// executor. `spawn_goal_run_executor` is itself double-spawn safe via
/// `running_goal_runs`, so overlapping ticks are harmless.
pub fn spawn_backoff_scheduler<R: tauri::Runtime>(app_handle: AppHandle<R>) {
    tauri::async_runtime::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(SCHEDULER_TICK_SECS));
        // Skip the immediate first tick so startup code runs uncontested.
        ticker.tick().await;

        loop {
            ticker.tick().await;

            let state = match app_handle.try_state::<AppState>() {
                Some(s) => s,
                None => return, // app teardown
            };

            let due: Vec<String> = match state.db.lock() {
                Ok(db) => db.list_runs_due_for_backoff().unwrap_or_default(),
                Err(_) => continue,
            };

            if due.is_empty() {
                continue;
            }

            // Cheap pre-check — skip runs that already have a live executor.
            let already_running: std::collections::HashSet<String> =
                match state.goal_run_cancels.lock() {
                    Ok(map) => map.keys().cloned().collect(),
                    Err(_) => continue,
                };

            for goal_run_id in due {
                if already_running.contains(&goal_run_id) {
                    continue;
                }
                info!(goal_run_id = %goal_run_id, "Backoff scheduler resuming run");
                // spawn_goal_run_executor re-checks the running-set atomically,
                // so even if another path spawns concurrently, we won't double-run.
                spawn_goal_run_executor(app_handle.clone(), goal_run_id);
            }
        }
    });
}
