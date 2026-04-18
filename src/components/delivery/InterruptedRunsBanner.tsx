import { useEffect, useState } from "react";
import * as goalRunApi from "../../api/goalRunApi";
import { useToastStore } from "../../store/useToastStore";
import type { GoalRun } from "../../types";

/// Startup banner for goal runs that were mid-execution when the app was
/// last closed (or crashed). One-click resume-all + per-run resume. Dismiss
/// cancels the listed runs so they don't resurface on the next launch —
/// preserves the runs as history (status=failed, reason="Cancelled by
/// operator") but gets them out of the interrupted queue.
export function InterruptedRunsBanner() {
  const [runs, setRuns] = useState<GoalRun[]>([]);
  const [dismissed, setDismissed] = useState(false);
  const [busy, setBusy] = useState(false);
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
      await goalRunApi.resumeGoalRun(id);
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
        await goalRunApi.resumeGoalRun(run.id);
      }
      setRuns([]);
    } catch (err) {
      addToast(`Failed to resume all runs: ${err}`, "warning");
    } finally {
      setBusy(false);
    }
  };

  const dismissAll = async () => {
    setBusy(true);
    try {
      for (const run of runs) {
        await goalRunApi.cancelGoalRun(run.id);
      }
      setRuns([]);
      setDismissed(true);
    } catch (err) {
      addToast(`Failed to dismiss runs: ${err}`, "warning");
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
            onClick={() => void dismissAll()}
            disabled={busy}
            title="Cancel these runs so they don't re-appear on next launch"
            className="rounded border border-gray-700 px-2 py-1 text-[11px] text-gray-300 hover:bg-gray-800 disabled:opacity-50"
          >
            Dismiss
          </button>
        </div>
      </div>
    </div>
  );
}
