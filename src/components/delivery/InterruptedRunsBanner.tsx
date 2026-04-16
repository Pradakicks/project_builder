import { useEffect, useState } from "react";
import * as goalRunApi from "../../api/goalRunApi";
import { useGoalRunStore } from "../../store/useGoalRunStore";
import { useToastStore } from "../../store/useToastStore";
import type { GoalRun } from "../../types";

/// Startup banner for goal runs that were mid-execution when the app was
/// last closed (or crashed). One-click resume-all + per-run resume. Dismiss
/// is session-only — a new app launch will re-surface any still-interrupted runs.
export function InterruptedRunsBanner() {
  const [runs, setRuns] = useState<GoalRun[]>([]);
  const [dismissed, setDismissed] = useState(false);
  const [busy, setBusy] = useState(false);
  const retryGoalRun = useGoalRunStore((s) => s.retryGoalRun);
  const addToast = useToastStore((s) => s.addToast);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const list = await goalRunApi.listInterruptedRuns();
        if (!cancelled) setRuns(list);
      } catch {
        // Silent: banner is opportunistic, not critical.
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  if (dismissed || runs.length === 0) return null;

  const resumeOne = async (id: string) => {
    setBusy(true);
    try {
      await retryGoalRun(id);
      setRuns((prev) => prev.filter((r) => r.id !== id));
    } catch (err) {
      addToast(`Failed to resume run: ${err}`, "warning");
    } finally {
      setBusy(false);
    }
  };

  const resumeAll = async () => {
    setBusy(true);
    try {
      for (const run of runs) {
        await retryGoalRun(run.id);
      }
      setRuns([]);
    } catch (err) {
      addToast(`Failed to resume all runs: ${err}`, "warning");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="fixed left-1/2 top-3 z-50 w-[min(640px,calc(100%-24px))] -translate-x-1/2 rounded-lg border border-orange-800/70 bg-orange-950/90 px-4 py-3 text-[12px] text-orange-100 shadow-lg shadow-black/30 backdrop-blur">
      <div className="flex items-start gap-3">
        <div className="flex-1">
          <p className="font-medium">
            {runs.length === 1
              ? "1 goal run was interrupted"
              : `${runs.length} goal runs were interrupted`}
          </p>
          <p className="mt-0.5 text-[11px] text-orange-200/80">
            These runs were mid-execution when the app closed. Resume to pick up at the last phase.
          </p>
          <ul className="mt-2 space-y-1">
            {runs.map((run) => (
              <li key={run.id} className="flex items-center gap-2 text-[11px]">
                <span className="flex-1 truncate text-orange-100/90">{run.prompt}</span>
                <button
                  onClick={() => void resumeOne(run.id)}
                  disabled={busy}
                  className="rounded border border-orange-700/80 px-2 py-0.5 text-[10px] text-orange-200 hover:bg-orange-900/60 disabled:opacity-50"
                >
                  Resume
                </button>
              </li>
            ))}
          </ul>
        </div>
        <div className="flex flex-col gap-1">
          <button
            onClick={() => void resumeAll()}
            disabled={busy}
            className="rounded border border-emerald-700 px-2 py-1 text-[11px] text-emerald-200 hover:bg-emerald-950/40 disabled:opacity-50"
          >
            Resume all
          </button>
          <button
            onClick={() => setDismissed(true)}
            className="rounded border border-gray-700 px-2 py-1 text-[11px] text-gray-300 hover:bg-gray-800"
          >
            Dismiss
          </button>
        </div>
      </div>
    </div>
  );
}
