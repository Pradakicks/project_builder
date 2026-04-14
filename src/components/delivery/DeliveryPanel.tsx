import { useEffect, useMemo } from "react";
import { useProjectStore } from "../../store/useProjectStore";
import { useGoalRunStore } from "../../store/useGoalRunStore";
import { useToastStore } from "../../store/useToastStore";
import { openRuntimeInBrowser } from "../../api/runtimeApi";
import type { GoalRunEvent, GoalRunTimelineEntry, VerificationResult } from "../../types";

function formatTime(value: string | null) {
  if (!value) return "unknown";
  return new Date(value).toLocaleString();
}

function VerificationResultBlock({ result }: { result: VerificationResult }) {
  const passed = result.passed;
  const totalMs = result.checks.reduce((sum, c) => sum + c.durationMs, 0);
  const totalSecs = (totalMs / 1000).toFixed(1);
  return (
    <div className={`rounded border p-2 ${passed ? "border-green-900/50 bg-green-950/20" : "border-red-900/50 bg-red-950/20"}`}>
      <div className="flex items-center justify-between">
        <p className={`text-[11px] font-medium ${passed ? "text-green-300" : "text-red-300"}`}>
          {passed ? "Verification passed" : "Verification failed"}
        </p>
        <span className="text-[10px] text-gray-500">{totalSecs}s total</span>
      </div>
      {result.checks.length > 0 && (
        <ul className="mt-1.5 space-y-0.5">
          {result.checks.map((check, i) => (
            <li key={i} className="flex items-start gap-1.5 text-[10px]">
              <span className={check.passed ? "text-green-400" : check.kind === "skipped" ? "text-gray-500" : "text-red-400"}>
                {check.kind === "skipped" ? "–" : check.passed ? "✓" : "✗"}
              </span>
              <span className={`flex-1 truncate ${check.passed ? "text-gray-300" : check.kind === "skipped" ? "text-gray-500" : "text-red-200"}`}>
                {check.name}
              </span>
              <span className="shrink-0 text-gray-600">{check.durationMs}ms</span>
            </li>
          ))}
        </ul>
      )}
      {!passed && result.message && (
        <p className="mt-1 text-[10px] text-red-300">{result.message}</p>
      )}
    </div>
  );
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
  const deliverySnapshot = useGoalRunStore((s) => s.deliverySnapshot);
  const goalRuns = useGoalRunStore((s) => s.goalRuns);
  const goalRunEvents = useGoalRunStore((s) => s.goalRunEvents);
  const runtimeStatus = useGoalRunStore((s) => s.runtimeStatus);
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
  const runtimeSnapshot = deliverySnapshot?.runtimeStatus ?? runtimeStatus;
  const currentRun = deliverySnapshot?.goalRun ?? currentGoalRun;
  const retryState = deliverySnapshot?.retryState ?? null;
  const blockingPiece = deliverySnapshot?.blockingPiece ?? null;
  const blockingTask = deliverySnapshot?.blockingTask ?? null;
  const codeEvidence = deliverySnapshot?.codeEvidence ?? null;
  const verificationResult = deliverySnapshot?.verificationResult ?? null;
  const runtimeLogs = runtimeSnapshot?.session?.recentLogs ?? [];
  const liveActivity = useGoalRunStore((s) => s.liveActivity);

  useEffect(() => {
    if (project?.id) {
      void refreshRuntimeStatus(project.id);
    }
  }, [project?.id, refreshRuntimeStatus]);

  const currentTimeline = useMemo(
    () => buildCurrentTimeline(deliverySnapshot?.recentEvents ?? goalRunEvents),
    [deliverySnapshot, goalRunEvents],
  );

  const recentRuns = useMemo(
    () => [...goalRuns].sort((a, b) => b.updatedAt.localeCompare(a.updatedAt)),
    [goalRuns],
  );

  const activeProjectId = project?.id ?? null;
  const hasAttention = Boolean(retryState?.attentionRequired ?? currentRun?.attentionRequired);

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
    if (!currentRun) return;
    try {
      await retryGoalRun(currentRun.id);
      addToast("Retried the active goal run", "info");
    } catch (error) {
      addToast(`Failed to retry goal run: ${error}`, "warning");
    }
  };

  const handleStopGoal = async () => {
    if (!currentRun) return;
    try {
      await stopGoalRun(currentRun.id);
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

  const runtimeState = runtimeSnapshot?.session?.status ?? "idle";
  const runtimeUrl = runtimeSnapshot?.session?.url ?? null;

  return (
    <div className="flex h-full flex-col bg-gradient-to-b from-slate-950 to-gray-950 text-gray-100">
      <div className="border-b border-gray-800 px-4 py-3">
        <div className="flex items-start justify-between gap-3">
          <div>
            <p className="text-xs font-semibold uppercase tracking-[0.2em] text-cyan-300">
              Delivery
            </p>
            <p className="mt-1 text-sm text-gray-200">
              {currentRun ? "Current run summary and lifecycle timeline" : "No active goal run"}
            </p>
          </div>
          {currentRun ? (
            <span
              className={`rounded px-2 py-0.5 text-[10px] font-medium ${
                currentRun.status === "completed"
                  ? "bg-emerald-900/60 text-emerald-300"
                  : currentRun.status === "running" || currentRun.status === "retrying"
                    ? "bg-blue-900/60 text-blue-300"
                    : currentRun.status === "blocked"
                      ? "bg-amber-900/60 text-amber-300"
                      : "bg-red-900/60 text-red-300"
              }`}
            >
              {currentRun.status}
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

        {liveActivity && currentRun?.phase === "implementation" && (
          <section className="rounded-xl border border-emerald-900/40 bg-emerald-950/20 p-3">
            <div className="flex items-center gap-1.5 text-[11px] text-emerald-300">
              <span className="inline-block h-1.5 w-1.5 animate-pulse rounded-full bg-emerald-400" />
              <span className="font-medium">{liveActivity.engine ?? "built-in"}</span>
              <span className="text-emerald-700">→</span>
              <span className="font-medium">{liveActivity.pieceName}</span>
              {liveActivity.total > 0 && (
                <span className="ml-auto text-emerald-600 tabular-nums">
                  task {liveActivity.currentIndex} / {liveActivity.total}
                </span>
              )}
            </div>
            {liveActivity.taskTitle && (
              <p className="mt-1 text-[10px] text-gray-400 truncate">{liveActivity.taskTitle}</p>
            )}
          </section>
        )}

        {currentRun ? (
          <section className="space-y-3 rounded-xl border border-gray-800 bg-gray-900/70 p-3 shadow-lg shadow-black/20">
            <div className="grid gap-3 text-[11px] md:grid-cols-2">
              <div className="rounded border border-gray-800 bg-gray-950/60 p-2">
                <p className="text-gray-500">Prompt</p>
                <p className="mt-1 text-gray-200">{currentRun.prompt}</p>
              </div>
              <div className="rounded border border-gray-800 bg-gray-950/60 p-2">
                <p className="text-gray-500">Current phase</p>
                <p className="mt-1 text-gray-200">
                  {currentRun.status} / {currentRun.phase}
                </p>
              </div>
              <div className="rounded border border-gray-800 bg-gray-950/60 p-2">
                <p className="text-gray-500">Plan</p>
                <p className="mt-1 font-mono text-gray-200">
                  {currentRun.currentPlanId ?? "none"}
                </p>
              </div>
              <div className="rounded border border-gray-800 bg-gray-950/60 p-2">
                <p className="text-gray-500">Retry state</p>
                <p className="mt-1 text-gray-200">
                  {retryState?.retryCount ?? currentRun.retryCount}
                  {retryState?.stopRequested ? " · stop requested" : ""}
                </p>
              </div>
            </div>

            <div className="space-y-2 text-[11px]">
              <div className="rounded border border-gray-800 bg-gray-950/60 p-2">
                <p className="text-gray-500">Retry timing</p>
                <p className="mt-1 text-gray-200">
                  {retryState?.retryBackoffUntil
                    ? `Next retry after ${formatTime(retryState.retryBackoffUntil)}`
                    : "No backoff scheduled"}
                </p>
              </div>
              {currentRun.runtimeStatusSummary ? (
                <div className="rounded border border-gray-800 bg-gray-950/60 p-2">
                  <p className="text-gray-500">Runtime summary</p>
                  <p className="mt-1 whitespace-pre-wrap text-gray-200">
                    {currentRun.runtimeStatusSummary}
                  </p>
                </div>
              ) : null}
              {verificationResult ? (
                <VerificationResultBlock result={verificationResult} />
              ) : null}
              {(currentRun.lastFailureSummary || currentRun.blockerReason) ? (
                <div className="rounded border border-amber-900/50 bg-amber-950/20 p-2 text-amber-100">
                  <p className="text-amber-300">Blocking truth</p>
                  {currentRun.blockerReason ? (
                    <p className="mt-1 whitespace-pre-wrap">{currentRun.blockerReason}</p>
                  ) : null}
                  {currentRun.lastFailureSummary ? (
                    <p className="mt-1 whitespace-pre-wrap">{currentRun.lastFailureSummary}</p>
                  ) : null}
                </div>
              ) : null}
            </div>

            <div className="flex flex-wrap items-center gap-2 text-[11px]">
              <span className="rounded border border-gray-800 bg-gray-950/60 px-2 py-0.5 text-gray-300">
                Updated {formatTime(currentRun.updatedAt)}
              </span>
              <span className="rounded border border-gray-800 bg-gray-950/60 px-2 py-0.5 text-gray-300">
                Runtime: {runtimeState}
              </span>
              {hasAttention ? (
                <span className="rounded border border-amber-700 bg-amber-950/40 px-2 py-0.5 text-amber-200">
                  attention required
                </span>
              ) : null}
              {runtimeUrl ? (
                <button
                  onClick={() => void openRuntimeInBrowser(runtimeUrl)}
                  className="rounded border border-gray-800 bg-gray-950/60 px-2 py-0.5 font-mono text-gray-300 hover:text-blue-300 cursor-pointer"
                  title="Open in browser"
                >
                  {runtimeUrl}
                </button>
              ) : null}
            </div>

            <div className="grid gap-3 text-[11px] md:grid-cols-2">
              <div className="rounded border border-gray-800 bg-gray-950/60 p-2">
                <p className="text-gray-500">Blocking piece / task</p>
                <p className="mt-1 text-gray-200">
                  {blockingPiece ? blockingPiece.name : "none"}
                </p>
                {blockingTask ? (
                  <p className="mt-1 text-gray-400">
                    {blockingTask.title} · {blockingTask.status}
                  </p>
                ) : null}
              </div>
              <div className="rounded border border-gray-800 bg-gray-950/60 p-2">
                <p className="text-gray-500">Code evidence</p>
                <p className="mt-1 text-gray-200">
                  {codeEvidence?.pieceName ?? "none"}
                </p>
                {codeEvidence?.gitBranch ? (
                  <p className="mt-1 font-mono text-gray-400">
                    {codeEvidence.gitBranch}
                    {codeEvidence.gitCommitSha ? ` · ${codeEvidence.gitCommitSha}` : ""}
                  </p>
                ) : null}
              </div>
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
              {currentRun.status !== "completed" ? (
                <button
                  onClick={() => void handleRetryGoal()}
                  disabled={orchestrating}
                  className="rounded border border-amber-700 px-3 py-1 text-[11px] text-amber-300 hover:bg-amber-950/40 disabled:opacity-50"
                >
                  {orchestrating ? "Running…" : "Retry goal"}
                </button>
              ) : null}
              {currentRun.status === "running" || currentRun.status === "retrying" ? (
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
                Code Evidence
              </p>
              <p className="text-[11px] text-gray-500">
                Generated files and git details pulled from the latest piece evidence.
              </p>
            </div>
          </div>

          {codeEvidence ? (
            <div className="space-y-3 text-[11px]">
              <div className="grid gap-2 md:grid-cols-3">
                <div className="rounded border border-gray-800 bg-gray-950/60 p-2">
                  <p className="text-gray-500">Git branch</p>
                  <p className="mt-1 font-mono text-gray-200">{codeEvidence.gitBranch ?? "none"}</p>
                </div>
                <div className="rounded border border-gray-800 bg-gray-950/60 p-2">
                  <p className="text-gray-500">Commit SHA</p>
                  <p className="mt-1 font-mono text-gray-200">{codeEvidence.gitCommitSha ?? "none"}</p>
                </div>
                <div className="rounded border border-gray-800 bg-gray-950/60 p-2">
                  <p className="text-gray-500">Diff stat</p>
                  <p className="mt-1 whitespace-pre-wrap text-gray-200">{codeEvidence.gitDiffStat ?? "none"}</p>
                </div>
              </div>
              {codeEvidence.generatedFilesArtifact ? (
                <div className="rounded border border-gray-800 bg-gray-950/60 p-2">
                  <div className="flex items-center justify-between gap-2">
                    <p className="text-gray-500">Generated files</p>
                    <span className="text-[10px] text-gray-500">
                      {formatTime(codeEvidence.generatedFilesArtifact.updatedAt)}
                    </span>
                  </div>
                  <pre className="mt-2 max-h-52 overflow-y-auto whitespace-pre-wrap rounded border border-gray-800 bg-black/40 p-2 text-[10px] text-gray-300">
                    {codeEvidence.generatedFilesArtifact.content}
                  </pre>
                </div>
              ) : (
                <p className="text-[11px] text-gray-500">No generated-files artifact yet for the blocking piece.</p>
              )}
            </div>
          ) : (
            <p className="text-[11px] text-gray-500">No code evidence available for the current run.</p>
          )}
        </section>

        <section className="space-y-3 rounded-xl border border-gray-800 bg-gray-900/70 p-3 shadow-lg shadow-black/20">
          <div className="flex items-center justify-between">
            <div>
              <p className="text-xs font-semibold uppercase tracking-[0.16em] text-cyan-300">
                Runtime Evidence
              </p>
              <p className="text-[11px] text-gray-500">
                Runtime status and tail from the persisted runtime session.
              </p>
            </div>
          </div>

          {runtimeSnapshot ? (
            <div className="space-y-2 text-[11px]">
              <div className="grid gap-2 md:grid-cols-2">
                <div className="rounded border border-gray-800 bg-gray-950/60 p-2">
                  <p className="text-gray-500">Runtime status</p>
                  <p className="mt-1 text-gray-200">{runtimeSnapshot.session?.status ?? "idle"}</p>
                </div>
                <div className="rounded border border-gray-800 bg-gray-950/60 p-2">
                  <p className="text-gray-500">Runtime URL</p>
                  <p className="mt-1 font-mono text-gray-200">{runtimeSnapshot.session?.url ?? runtimeSnapshot.spec?.appUrl ?? "none"}</p>
                </div>
              </div>
              <div className="grid gap-2 md:grid-cols-2">
                <div className="rounded border border-gray-800 bg-gray-950/60 p-2">
                  <p className="text-gray-500">Log path</p>
                  <p className="mt-1 font-mono text-gray-200">{runtimeSnapshot.session?.logPath ?? "none"}</p>
                </div>
                <div className="rounded border border-gray-800 bg-gray-950/60 p-2">
                  <p className="text-gray-500">Last runtime error</p>
                  <p className="mt-1 whitespace-pre-wrap text-gray-200">{runtimeSnapshot.session?.lastError ?? "none"}</p>
                </div>
              </div>
              {runtimeLogs.length > 0 ? (
                <div className="rounded border border-gray-800 bg-gray-950/60 p-2">
                  <p className="text-gray-500">Recent runtime logs</p>
                  <pre className="mt-2 max-h-48 overflow-y-auto whitespace-pre-wrap rounded border border-gray-800 bg-black/40 p-2 text-[10px] text-gray-300">
                    {runtimeLogs.join("\n")}
                  </pre>
                </div>
              ) : (
                <p className="text-[11px] text-gray-500">No recent runtime logs available.</p>
              )}
            </div>
          ) : (
            <p className="text-[11px] text-gray-500">No runtime evidence available.</p>
          )}
        </section>

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
                    entry.active && (currentRun?.status === "running" || currentRun?.status === "retrying")
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
                      {entry.active && (currentRun?.status === "running" || currentRun?.status === "retrying") ? "active" : entry.status}
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
                    currentRun?.id === run.id
                      ? "border-blue-700 bg-blue-950/25"
                      : "border-gray-800 bg-gray-950/60"
                  } cursor-pointer`}
                >
                  <div className="flex items-center justify-between gap-2">
                    <p className="font-medium text-gray-200">
                      {run.status} / {run.phase}
                    </p>
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

      </div>
    </div>
  );
}
