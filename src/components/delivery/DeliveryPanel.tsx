import { useEffect, useMemo } from "react";
import { useProjectStore } from "../../store/useProjectStore";
import { useGoalRunStore } from "../../store/useGoalRunStore";
import { useToastStore } from "../../store/useToastStore";
import type { GoalRunEvent, GoalRunTimelineEntry } from "../../types";

function formatTime(value: string | null) {
  if (!value) return "unknown";
  return new Date(value).toLocaleString();
}

function buildCurrentTimeline(events: GoalRunEvent[]): GoalRunTimelineEntry[] {
  return events.map((event) => ({
    id: event.id,
    kind:
      event.phase === "runtime-configuration" || event.phase === "runtime-execution"
        ? "runtime"
        : event.phase === "verification"
          ? "verification"
          : "phase",
    title: `${event.phase} / ${event.kind}`,
    detail: event.summary,
    timestamp: event.createdAt,
    active: event.kind === "phase-started",
    status:
      event.kind === "failed" || event.kind === "blocked"
        ? "warning"
        : event.kind === "phase-completed"
          ? "success"
          : "info",
  }));
}

export function DeliveryPanel() {
  const project = useProjectStore((s) => s.project);
  const currentGoalRun = useGoalRunStore((s) => s.currentGoalRun);
  const goalRuns = useGoalRunStore((s) => s.goalRuns);
  const goalRunEvents = useGoalRunStore((s) => s.goalRunEvents);
  const runtimeStatus = useGoalRunStore((s) => s.runtimeStatus);
  const runtimeLogs = useGoalRunStore((s) => s.runtimeLogs);
  const loading = useGoalRunStore((s) => s.loading);
  const orchestrating = useGoalRunStore((s) => s.orchestrating);
  const lastError = useGoalRunStore((s) => s.lastError);
  const refreshRuntimeStatus = useGoalRunStore((s) => s.refreshRuntimeStatus);
  const startRuntime = useGoalRunStore((s) => s.startRuntime);
  const stopRuntime = useGoalRunStore((s) => s.stopRuntime);
  const retryGoalRun = useGoalRunStore((s) => s.retryGoalRun);
  const stopGoalRun = useGoalRunStore((s) => s.stopGoalRun);
  const selectGoalRun = useGoalRunStore((s) => s.selectGoalRun);
  const addToast = useToastStore((s) => s.addToast);

  useEffect(() => {
    if (project?.id) {
      void refreshRuntimeStatus(project.id);
    }
  }, [project?.id, refreshRuntimeStatus]);

  const currentTimeline = useMemo(
    () => buildCurrentTimeline(goalRunEvents),
    [goalRunEvents],
  );

  const recentRuns = useMemo(
    () => [...goalRuns].sort((a, b) => b.updatedAt.localeCompare(a.updatedAt)),
    [goalRuns],
  );

  const activeProjectId = project?.id ?? null;

  const handleStartRuntime = async () => {
    if (!activeProjectId) return;
    try {
      await startRuntime(activeProjectId);
      addToast("Started the configured runtime", "info");
    } catch (error) {
      addToast(`Failed to start runtime: ${error}`, "warning");
    }
  };

  const handleStopRuntime = async () => {
    if (!activeProjectId) return;
    try {
      await stopRuntime(activeProjectId);
      addToast("Stopped the runtime", "info");
    } catch (error) {
      addToast(`Failed to stop runtime: ${error}`, "warning");
    }
  };

  const handleRetryGoal = async () => {
    if (!currentGoalRun) return;
    try {
      await retryGoalRun(currentGoalRun.id);
      addToast("Retried the active goal run", "info");
    } catch (error) {
      addToast(`Failed to retry goal run: ${error}`, "warning");
    }
  };

  const handleStopGoal = async () => {
    if (!currentGoalRun) return;
    try {
      await stopGoalRun(currentGoalRun.id);
      addToast("Stopped the active goal run", "info");
    } catch (error) {
      addToast(`Failed to stop goal run: ${error}`, "warning");
    }
  };

  if (!project) {
    return (
      <div className="flex h-full items-center justify-center px-4 text-center text-[11px] text-gray-500">
        Open a project to view the delivery timeline.
      </div>
    );
  }

  const runtimeState = runtimeStatus?.session?.status ?? "idle";
  const runtimeUrl = runtimeStatus?.session?.url ?? null;

  return (
    <div className="flex h-full flex-col bg-gradient-to-b from-slate-950 to-gray-950 text-gray-100">
      <div className="border-b border-gray-800 px-4 py-3">
        <div className="flex items-start justify-between gap-3">
          <div>
            <p className="text-xs font-semibold uppercase tracking-[0.2em] text-cyan-300">
              Delivery
            </p>
            <p className="mt-1 text-sm text-gray-200">
              {currentGoalRun ? "Current run summary and lifecycle timeline" : "No active goal run"}
            </p>
          </div>
          {currentGoalRun ? (
            <span
              className={`rounded px-2 py-0.5 text-[10px] font-medium ${
                currentGoalRun.status === "completed"
                  ? "bg-emerald-900/60 text-emerald-300"
                  : currentGoalRun.status === "running"
                    ? "bg-blue-900/60 text-blue-300"
                    : currentGoalRun.status === "blocked"
                      ? "bg-amber-900/60 text-amber-300"
                      : "bg-red-900/60 text-red-300"
              }`}
            >
              {currentGoalRun.status}
            </span>
          ) : null}
        </div>
        <p className="mt-2 text-[11px] text-gray-400">
          {loading ? "Loading persisted goal runs..." : project.description || "Goal-run delivery state for this project."}
        </p>
      </div>

      <div className="flex-1 overflow-y-auto px-4 py-3 space-y-4">
        {lastError && (
          <div className="rounded border border-red-900/50 bg-red-950/20 px-3 py-2 text-[11px] text-red-200">
            {lastError}
          </div>
        )}

        {currentGoalRun ? (
          <section className="space-y-3 rounded-xl border border-gray-800 bg-gray-900/70 p-3 shadow-lg shadow-black/20">
            <div className="grid gap-3 text-[11px] md:grid-cols-2">
              <div className="rounded border border-gray-800 bg-gray-950/60 p-2">
                <p className="text-gray-500">Prompt</p>
                <p className="mt-1 text-gray-200">{currentGoalRun.prompt}</p>
              </div>
              <div className="rounded border border-gray-800 bg-gray-950/60 p-2">
                <p className="text-gray-500">Current phase</p>
                <p className="mt-1 text-gray-200">
                  {currentGoalRun.status} / {currentGoalRun.phase}
                </p>
              </div>
              <div className="rounded border border-gray-800 bg-gray-950/60 p-2">
                <p className="text-gray-500">Plan</p>
                <p className="mt-1 font-mono text-gray-200">
                  {currentGoalRun.currentPlanId ?? "none"}
                </p>
              </div>
              <div className="rounded border border-gray-800 bg-gray-950/60 p-2">
                <p className="text-gray-500">Retry count</p>
                <p className="mt-1 text-gray-200">{currentGoalRun.retryCount}</p>
              </div>
            </div>

            {(currentGoalRun.runtimeStatusSummary || currentGoalRun.verificationSummary || currentGoalRun.lastFailureSummary || currentGoalRun.blockerReason) && (
              <div className="space-y-2 text-[11px]">
                {currentGoalRun.runtimeStatusSummary ? (
                  <div className="rounded border border-gray-800 bg-gray-950/60 p-2">
                    <p className="text-gray-500">Runtime summary</p>
                    <p className="mt-1 whitespace-pre-wrap text-gray-200">{currentGoalRun.runtimeStatusSummary}</p>
                  </div>
                ) : null}
                {currentGoalRun.verificationSummary ? (
                  <div className="rounded border border-gray-800 bg-gray-950/60 p-2">
                    <p className="text-gray-500">Verification summary</p>
                    <pre className="mt-1 whitespace-pre-wrap text-gray-200">{currentGoalRun.verificationSummary}</pre>
                  </div>
                ) : null}
                {currentGoalRun.lastFailureSummary ? (
                  <div className="rounded border border-amber-900/50 bg-amber-950/20 p-2 text-amber-100">
                    <p className="text-amber-300">Last failure</p>
                    <p className="mt-1 whitespace-pre-wrap">{currentGoalRun.lastFailureSummary}</p>
                  </div>
                ) : null}
                {currentGoalRun.blockerReason ? (
                  <div className="rounded border border-amber-900/50 bg-amber-950/20 p-2 text-amber-100">
                    <p className="text-amber-300">Blocker</p>
                    <p className="mt-1 whitespace-pre-wrap">{currentGoalRun.blockerReason}</p>
                  </div>
                ) : null}
              </div>
            )}

            <div className="flex flex-wrap items-center gap-2 text-[11px]">
              <span className="rounded border border-gray-800 bg-gray-950/60 px-2 py-0.5 text-gray-300">
                Updated {formatTime(currentGoalRun.updatedAt)}
              </span>
              <span className="rounded border border-gray-800 bg-gray-950/60 px-2 py-0.5 text-gray-300">
                Runtime: {runtimeState}
              </span>
              {runtimeUrl ? (
                <span className="rounded border border-gray-800 bg-gray-950/60 px-2 py-0.5 font-mono text-gray-300">
                  {runtimeUrl}
                </span>
              ) : null}
            </div>

            <div className="flex flex-wrap gap-2">
              <button
                onClick={() => void refreshRuntimeStatus(activeProjectId ?? undefined)}
                className="rounded border border-gray-700 px-3 py-1 text-[11px] text-gray-300 hover:bg-gray-800"
              >
                Refresh runtime
              </button>
              <button
                onClick={() => void handleStartRuntime()}
                className="rounded border border-emerald-700 px-3 py-1 text-[11px] text-emerald-300 hover:bg-emerald-950/40"
              >
                Run app
              </button>
              <button
                onClick={() => void handleStopRuntime()}
                className="rounded border border-red-700 px-3 py-1 text-[11px] text-red-300 hover:bg-red-950/40"
              >
                Stop app
              </button>
              {currentGoalRun.status !== "completed" ? (
                <button
                  onClick={() => void handleRetryGoal()}
                  disabled={orchestrating}
                  className="rounded border border-amber-700 px-3 py-1 text-[11px] text-amber-300 hover:bg-amber-950/40 disabled:opacity-50"
                >
                  {orchestrating ? "Running…" : "Retry goal"}
                </button>
              ) : null}
              {currentGoalRun.status === "running" ? (
                <button
                  onClick={() => void handleStopGoal()}
                  className="rounded border border-red-700 px-3 py-1 text-[11px] text-red-300 hover:bg-red-950/40"
                >
                  Stop goal
                </button>
              ) : null}
            </div>
          </section>
        ) : (
          <section className="rounded-xl border border-gray-800 bg-gray-900/70 p-4 text-[11px] text-gray-400">
            No goal run has started yet. Send a CTO prompt to create the first delivery run.
          </section>
        )}

        <section className="space-y-3 rounded-xl border border-gray-800 bg-gray-900/70 p-3 shadow-lg shadow-black/20">
          <div className="flex items-center justify-between">
            <div>
              <p className="text-xs font-semibold uppercase tracking-[0.16em] text-cyan-300">
                Timeline
              </p>
              <p className="text-[11px] text-gray-500">
                Persisted backend executor events for the selected run.
              </p>
            </div>
          </div>

          {currentTimeline.length > 0 ? (
            <div className="space-y-2">
              {currentTimeline.map((entry) => (
                <div
                  key={entry.id}
                  className={`rounded border px-3 py-2 text-[11px] ${
                    entry.active && currentGoalRun?.status === "running"
                      ? "border-cyan-700 bg-cyan-950/25"
                      : "border-gray-800 bg-gray-950/60"
                  }`}
                >
                  <div className="flex items-center justify-between gap-2">
                    <p className="font-medium text-gray-200">{entry.title}</p>
                    <span
                      className={`rounded px-2 py-0.5 text-[10px] font-medium ${
                        entry.status === "success"
                          ? "bg-emerald-900/60 text-emerald-300"
                          : entry.status === "warning"
                            ? "bg-amber-900/60 text-amber-300"
                            : "bg-blue-900/60 text-blue-300"
                      }`}
                    >
                      {entry.active && currentGoalRun?.status === "running" ? "active" : entry.status}
                    </span>
                  </div>
                  <p className="mt-1 text-[10px] text-gray-500">{formatTime(entry.timestamp)}</p>
                  {entry.detail ? (
                    <p className="mt-1 whitespace-pre-wrap text-gray-400">{entry.detail}</p>
                  ) : null}
                </div>
              ))}
            </div>
          ) : (
            <p className="text-[11px] text-gray-500">Timeline will appear once a goal run exists.</p>
          )}
        </section>

        <section className="space-y-3 rounded-xl border border-gray-800 bg-gray-900/70 p-3 shadow-lg shadow-black/20">
          <div className="flex items-center justify-between">
            <div>
              <p className="text-xs font-semibold uppercase tracking-[0.16em] text-cyan-300">
                Persisted Runs
              </p>
              <p className="text-[11px] text-gray-500">
                Recent saved goal runs for this project.
              </p>
            </div>
            <span className="text-[10px] text-gray-500">{recentRuns.length} total</span>
          </div>

          {recentRuns.length > 0 ? (
            <div className="space-y-2">
              {recentRuns.map((run) => (
                <div
                  key={run.id}
                  onClick={() => void selectGoalRun(run.id)}
                  className={`rounded border px-3 py-2 text-[11px] ${
                    currentGoalRun?.id === run.id
                      ? "border-blue-700 bg-blue-950/25"
                      : "border-gray-800 bg-gray-950/60"
                  } cursor-pointer`}
                >
                  <div className="flex items-center justify-between gap-2">
                    <p className="font-medium text-gray-200">{run.status} / {run.phase}</p>
                    <span className="text-[10px] text-gray-500">{formatTime(run.updatedAt)}</span>
                  </div>
                  <p className="mt-1 line-clamp-2 text-gray-400">{run.prompt}</p>
                </div>
              ))}
            </div>
          ) : (
            <p className="text-[11px] text-gray-500">No persisted goal runs yet.</p>
          )}
        </section>

        {runtimeLogs.length > 0 ? (
          <section className="space-y-2 rounded-xl border border-gray-800 bg-gray-900/70 p-3 shadow-lg shadow-black/20">
            <div>
              <p className="text-xs font-semibold uppercase tracking-[0.16em] text-cyan-300">
                Runtime Logs
              </p>
              <p className="text-[11px] text-gray-500">
                Latest tail from the active runtime session.
              </p>
            </div>
            <pre className="max-h-52 overflow-y-auto whitespace-pre-wrap rounded border border-gray-800 bg-black/40 p-2 text-[10px] text-gray-300">
              {runtimeLogs.join("\n")}
            </pre>
          </section>
        ) : null}
      </div>
    </div>
  );
}
