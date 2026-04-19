import { useEffect, useMemo, useState } from "react";
import { useProjectStore } from "../../store/useProjectStore";
import { useGoalRunStore } from "../../store/useGoalRunStore";
import { useToastStore } from "../../store/useToastStore";
import { openRuntimeInBrowser } from "../../api/runtimeApi";
import type {
  CheckKind,
  GoalRun,
  GoalRunEvent,
  GoalRunRetryState,
  VerificationCheck,
  VerificationResult,
} from "../../types";
import { QuickRuntimeSetup } from "./QuickRuntimeSetup";

function formatTime(value: string | null) {
  if (!value) return "unknown";
  return new Date(value).toLocaleString();
}

export interface RuntimeLogView {
  lines: string[];
  source: "live" | "snapshot" | "empty";
  updatedAt: string | null;
}

export function selectRuntimeLogView(
  liveLogs: string[],
  liveUpdatedAt: string | null,
  snapshotLogs: string[],
  snapshotUpdatedAt: string | null,
): RuntimeLogView {
  if (liveLogs.length > 0) {
    return {
      lines: liveLogs,
      source: "live",
      updatedAt: liveUpdatedAt,
    };
  }

  if (snapshotLogs.length > 0) {
    return {
      lines: snapshotLogs,
      source: "snapshot",
      updatedAt: snapshotUpdatedAt,
    };
  }

  return {
    lines: [],
    source: "empty",
    updatedAt: liveUpdatedAt ?? snapshotUpdatedAt,
  };
}

export interface FailureView {
  currentBlocker: { text: string; updatedAt: string | null } | null;
  previousFailures: Array<{ label: string; text: string; updatedAt: string | null }>;
}

function pushFailure(
  list: Array<{ label: string; text: string; updatedAt: string | null }>,
  label: string,
  text: string | null | undefined,
  updatedAt: string | null,
) {
  if (!text) return;
  if (list.some((item) => item.text === text)) return;
  list.push({ label, text, updatedAt });
}

export function buildFailureView(
  currentRun: GoalRun | null,
  retryState: GoalRunRetryState | null,
  verificationResult: VerificationResult | null,
): FailureView {
  const currentBlocker =
    currentRun?.status === "blocked"
      ? {
          text: currentRun.blockerReason ?? currentRun.lastFailureSummary ?? "Blocked",
          updatedAt: currentRun.updatedAt,
        }
      : null;
  const previousFailures: Array<{ label: string; text: string; updatedAt: string | null }> = [];

  pushFailure(
    previousFailures,
    "Last failure",
    currentRun?.lastFailureSummary,
    currentRun?.updatedAt ?? null,
  );
  pushFailure(
    previousFailures,
    "Retry failure",
    retryState?.lastFailureSummary,
    currentRun?.updatedAt ?? null,
  );
  pushFailure(
    previousFailures,
    "Verification",
    verificationResult?.message,
    verificationResult?.finishedAt ?? null,
  );

  if (currentBlocker && previousFailures.some((item) => item.text === currentBlocker.text)) {
    return {
      currentBlocker,
      previousFailures: previousFailures.filter((item) => item.text !== currentBlocker.text),
    };
  }

  return { currentBlocker, previousFailures };
}

const KIND_LABEL: Record<CheckKind, string> = {
  shell: "shell",
  http: "http",
  tcpPort: "tcp",
  logScan: "log",
  skipped: "skip",
};

function CheckDetailRow({ check }: { check: VerificationCheck }) {
  const [open, setOpen] = useState(false);
  const hasExtras = Boolean(check.expected || check.actual);
  const clickable = hasExtras || check.detail.length > 0;
  const actual = check.actual ?? "";
  const actualIsMultiline = actual.includes("\n");

  return (
    <li className="text-[10px]">
      <button
        type="button"
        onClick={() => clickable && setOpen((v) => !v)}
        className={`flex w-full items-start gap-1.5 text-left ${clickable ? "hover:bg-white/5" : "cursor-default"} rounded px-0.5 py-0.5`}
      >
        <span
          className={
            check.passed
              ? "text-green-400"
              : check.kind === "skipped"
                ? "text-gray-500"
                : "text-red-400"
          }
        >
          {check.kind === "skipped" ? "–" : check.passed ? "✓" : "✗"}
        </span>
        <span className="shrink-0 rounded bg-gray-800/60 px-1 text-[9px] uppercase tracking-wide text-gray-400">
          {KIND_LABEL[check.kind] ?? check.kind}
        </span>
        <span
          className={`flex-1 truncate ${check.passed ? "text-gray-300" : check.kind === "skipped" ? "text-gray-500" : "text-red-200"}`}
        >
          {check.name}
        </span>
        <span className="shrink-0 text-gray-600">{check.durationMs}ms</span>
        {clickable && (
          <span className="shrink-0 text-gray-600">{open ? "▾" : "▸"}</span>
        )}
      </button>
      {open && (
        <div className="ml-4 mt-0.5 space-y-0.5 rounded bg-black/20 p-1.5 font-mono text-[10px] text-gray-300">
          {check.expected && (
            <div>
              <span className="text-gray-500">expected </span>
              <span className="text-gray-200">{check.expected}</span>
            </div>
          )}
          {check.actual && !actualIsMultiline && (
            <div>
              <span className="text-gray-500">actual&nbsp;&nbsp; </span>
              <span className={check.passed ? "text-gray-200" : "text-red-200"}>{check.actual}</span>
            </div>
          )}
          {check.actual && actualIsMultiline && (
            <div>
              <div className="text-gray-500">actual</div>
              <pre
                className={`mt-0.5 whitespace-pre-wrap border-l-2 border-gray-700 pl-2 ${check.passed ? "text-gray-200" : "text-red-200"}`}
              >
                {check.actual}
              </pre>
            </div>
          )}
          {!check.expected && !check.actual && check.detail && (
            <div>
              <span className="text-gray-500">detail&nbsp;&nbsp; </span>
              <span className="text-gray-200">{check.detail}</span>
            </div>
          )}
          {(check.expected || check.actual) && check.detail && (
            <div className="text-gray-500">
              <span className="text-gray-600">detail&nbsp;&nbsp; </span>
              <span className="text-gray-400">{check.detail}</span>
            </div>
          )}
        </div>
      )}
    </li>
  );
}

function VerificationResultBlock({ result }: { result: VerificationResult }) {
  const passed = result.passed;
  const totalMs = result.checks.reduce((sum, c) => sum + c.durationMs, 0);
  const totalSecs = (totalMs / 1000).toFixed(1);
  const passedCount = result.checks.filter((c) => c.passed).length;
  return (
    <div className={`rounded border p-2 ${passed ? "border-green-900/50 bg-green-950/20" : "border-red-900/50 bg-red-950/20"}`}>
      <div className="flex items-center justify-between">
        <p className={`text-[11px] font-medium ${passed ? "text-green-300" : "text-red-300"}`}>
          {passed ? "Verification passed" : "Verification failed"}
          <span className="ml-2 text-[10px] text-gray-500">
            {passedCount}/{result.checks.length} checks
          </span>
        </p>
        <span className="text-[10px] text-gray-500">{totalSecs}s total</span>
      </div>
      {result.checks.length > 0 && (
        <ul className="mt-1.5 space-y-0.5">
          {result.checks.map((check, i) => (
            <CheckDetailRow key={i} check={check} />
          ))}
        </ul>
      )}
      {!passed && result.message && (
        <p className="mt-1 text-[10px] text-red-300">{result.message}</p>
      )}
    </div>
  );
}

interface PhaseArc {
  phase: string;
  label: string;
  startedAt: string | null;
  endedAt: string | null;
  durationLabel: string | null;
  outcome: "completed" | "failed" | "blocked" | "running" | "pending";
  repairCount: number;
  repairSummaries: string[];
  contextDetail: string | null;
}

const PHASE_ORDER = [
  "prompt-received",
  "planning",
  "implementation",
  "merging",
  "runtime-configuration",
  "runtime-execution",
  "verification",
] as const;

const PHASE_LABELS: Record<string, string> = {
  "prompt-received": "Started",
  "planning": "Planning",
  "implementation": "Implementation",
  "merging": "Merging",
  "runtime-configuration": "Runtime Configuration",
  "runtime-execution": "Runtime Execution",
  "verification": "Verification",
};

const REPAIR_EVENT_KINDS = new Set([
  "retry-scheduled",
  "retry-resumed",
  "repair-requested",
  "repair-started",
  "repair-skipped",
  "repair-executed",
  "repair-failed",
]);

const REPAIR_OUTCOME_EVENT_KINDS = new Set([
  "repair-requested",
  "repair-started",
  "repair-skipped",
  "repair-executed",
  "repair-failed",
]);

function formatDuration(startedAt: string, endedAt: string): string {
  const ms = Date.parse(endedAt) - Date.parse(startedAt);
  if (ms < 0) return "";
  const secs = Math.round(ms / 1000);
  if (secs < 60) return `${secs}s`;
  const mins = Math.floor(secs / 60);
  const rem = secs % 60;
  return rem > 0 ? `${mins}m ${rem}s` : `${mins}m`;
}

function buildPhaseArcs(events: GoalRunEvent[], currentRun: GoalRun | null): PhaseArc[] {
  const byPhase: Record<string, GoalRunEvent[]> = {};
  for (const e of events) {
    if (!byPhase[e.phase]) byPhase[e.phase] = [];
    byPhase[e.phase].push(e);
  }

  const arcs: PhaseArc[] = [];
  for (const phase of PHASE_ORDER) {
    const phaseEvents = byPhase[phase];
    if (!phaseEvents || phaseEvents.length === 0) continue;

    let startedAt: string | null = null;
    let endedAt: string | null = null;
    let outcome: PhaseArc["outcome"] = "pending";
    const repairSummaries: string[] = [];
    let contextDetail: string | null = null;

    for (const e of phaseEvents) {
      if (e.kind === "phase-started") {
        startedAt = e.createdAt;
        outcome = "running";
        // Context: for implementation, show plan ID from payload
        if (phase === "planning" || phase === "implementation") {
          try {
            const p = JSON.parse(e.payloadJson ?? "{}") as Record<string, string>;
            if (p.planId) contextDetail = `Plan: ${String(p.planId).slice(0, 8)}`;
          } catch { /* ignore */ }
        }
      } else if (e.kind === "phase-completed") {
        endedAt = e.createdAt;
        outcome = "completed";
        // Context: for runtime-configuration, show run command from payload
        if (phase === "runtime-configuration") {
          try {
            const p = JSON.parse(e.payloadJson ?? "{}") as Record<string, string>;
            if (p.runCommand) contextDetail = p.runCommand;
          } catch { /* ignore */ }
        }
      } else if (e.kind === "failed") {
        endedAt = e.createdAt;
        outcome = "failed";
      } else if (e.kind === "blocked") {
        endedAt = e.createdAt;
        outcome = "blocked";
      } else if (e.kind === "cancelled-mid-phase") {
        endedAt = e.createdAt;
        outcome = "failed";
      } else if (REPAIR_EVENT_KINDS.has(e.kind)) {
        repairSummaries.push(e.summary);
      }
    }

    // If phase-started exists but no terminal event and it's not the current phase, mark running
    // (shouldn't happen, but guard against stale data)
    if (outcome === "running" && currentRun) {
      if (currentRun.phase !== phase && currentRun.status !== "running" && currentRun.status !== "retrying") {
        outcome = "running"; // keep as-is — executor may not have written PhaseCompleted yet for old runs
      }
    }

    const durationLabel = startedAt && endedAt ? formatDuration(startedAt, endedAt) : null;

    arcs.push({
      phase,
      label: PHASE_LABELS[phase] ?? phase,
      startedAt,
      endedAt,
      durationLabel,
      outcome,
      repairCount: repairSummaries.length,
      repairSummaries,
      contextDetail,
    });
  }

  return arcs;
}

export function DeliveryPanel() {
  const project = useProjectStore((s) => s.project);
  const currentGoalRun = useGoalRunStore((s) => s.currentGoalRun);
  const deliverySnapshot = useGoalRunStore((s) => s.deliverySnapshot);
  const goalRuns = useGoalRunStore((s) => s.goalRuns);
  const goalRunEvents = useGoalRunStore((s) => s.goalRunEvents);
  const runtimeStatus = useGoalRunStore((s) => s.runtimeStatus);
  const runtimeLogs = useGoalRunStore((s) => s.runtimeLogs);
  const runtimeLogsUpdatedAt = useGoalRunStore((s) => s.runtimeLogsUpdatedAt);
  const loading = useGoalRunStore((s) => s.loading);
  const orchestrating = useGoalRunStore((s) => s.orchestrating);
  const lastError = useGoalRunStore((s) => s.lastError);
  const refreshRuntimeStatus = useGoalRunStore((s) => s.refreshRuntimeStatus);
  const startRuntime = useGoalRunStore((s) => s.startRuntime);
  const stopRuntime = useGoalRunStore((s) => s.stopRuntime);
  const continueAutopilotWithRepair = useGoalRunStore((s) => s.continueAutopilotWithRepair);
  const stopGoalRun = useGoalRunStore((s) => s.stopGoalRun);
  const pauseGoalRun = useGoalRunStore((s) => s.pauseGoalRun);
  const rerunVerification = useGoalRunStore((s) => s.rerunVerification);
  const selectGoalRun = useGoalRunStore((s) => s.selectGoalRun);
  const addToast = useToastStore((s) => s.addToast);
  const runtimeSnapshot = runtimeStatus ?? deliverySnapshot?.runtimeStatus;
  const currentRun = deliverySnapshot?.goalRun ?? currentGoalRun;
  const retryState = deliverySnapshot?.retryState ?? null;
  const blockingPiece = deliverySnapshot?.blockingPiece ?? null;
  const blockingTask = deliverySnapshot?.blockingTask ?? null;
  const codeEvidence = deliverySnapshot?.codeEvidence ?? null;
  const verificationResult = deliverySnapshot?.verificationResult ?? null;
  const liveActivity = useGoalRunStore((s) => s.liveActivity);
  const phaseActivity = useGoalRunStore((s) => s.phaseActivity);

  useEffect(() => {
    if (project?.id) {
      void refreshRuntimeStatus(project.id);
    }
  }, [project?.id, refreshRuntimeStatus]);

  const phaseArcs = useMemo(
    () => buildPhaseArcs(deliverySnapshot?.recentEvents ?? goalRunEvents, currentRun ?? null),
    [deliverySnapshot, goalRunEvents, currentRun],
  );

  const recentRuns = useMemo(
    () => [...goalRuns].sort((a, b) => b.updatedAt.localeCompare(a.updatedAt)),
    [goalRuns],
  );

  const runtimeLogView = useMemo(
    () =>
      selectRuntimeLogView(
        runtimeLogs,
        runtimeLogsUpdatedAt,
        runtimeSnapshot?.session?.recentLogs ?? [],
        runtimeSnapshot?.session?.updatedAt ?? null,
      ),
    [runtimeLogs, runtimeLogsUpdatedAt, runtimeSnapshot],
  );

  const failureView = useMemo(
    () => buildFailureView(currentRun ?? null, retryState, verificationResult),
    [currentRun, retryState, verificationResult],
  );

  const repairEvents = useMemo(() => {
    const events = deliverySnapshot?.recentEvents ?? goalRunEvents;
    return [...events]
      .filter((event) => REPAIR_OUTCOME_EVENT_KINDS.has(event.kind))
      .sort((a, b) => b.createdAt.localeCompare(a.createdAt));
  }, [deliverySnapshot, goalRunEvents]);

  const activeProjectId = project?.id ?? null;
  const hasAttention = Boolean(retryState?.attentionRequired ?? currentRun?.attentionRequired);

  const handleStartRuntime = async () => {
    if (!activeProjectId) return;
    try {
      await startRuntime(activeProjectId);
      addToast("Started app. Watch Recent runtime logs for fresh output.", "info");
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

  const handleStopGoal = async () => {
    if (!currentRun) return;
    try {
      await stopGoalRun(currentRun.id);
      addToast("Stopped the active goal run", "info");
    } catch (error) {
      addToast(`Failed to stop goal run: ${error}`, "warning");
    }
  };

  const handlePauseGoal = async () => {
    if (!currentRun) return;
    try {
      await pauseGoalRun(currentRun.id);
    } catch (error) {
      addToast(`Failed to pause goal run: ${error}`, "warning");
    }
  };

  const handleRerunVerification = async () => {
    if (!currentRun) return;
    try {
      await rerunVerification(currentRun.id);
    } catch (error) {
      addToast(`Failed to rerun verification: ${error}`, "warning");
    }
  };

  const handleResumeGoal = async () => {
    if (!currentRun) return;
    try {
      await continueAutopilotWithRepair(currentRun.id);
    } catch (error) {
      addToast(`Failed to resume with repair: ${error}`, "warning");
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
                    : currentRun.status === "paused"
                      ? "bg-yellow-900/60 text-yellow-300"
                      : currentRun.status === "blocked"
                        ? "bg-amber-900/60 text-amber-300"
                        : currentRun.status === "interrupted"
                          ? "bg-orange-900/60 text-orange-300"
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

        {phaseActivity && (
          <section className="rounded-xl border border-cyan-900/40 bg-cyan-950/20 p-3 text-[11px] text-cyan-100">
            <div className="flex items-center justify-between gap-3">
              <p className="font-medium">
                {phaseActivity.phase}
                {phaseActivity.stepIndex != null && phaseActivity.stepTotal != null
                  ? ` (${phaseActivity.stepIndex}/${phaseActivity.stepTotal})`
                  : ""}
              </p>
              <span className="text-cyan-300/70">{formatTime(phaseActivity.updatedAt)}</span>
            </div>
            <p className="mt-1 text-cyan-100/80">{phaseActivity.message}</p>
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
                <p className="mt-1 text-gray-400">
                  Retry count {retryState?.retryCount ?? currentRun.retryCount}
                  {retryState?.stopRequested ? " · stop requested" : ""}
                </p>
                {repairEvents.length > 0 && (
                  <p className="mt-1 text-amber-300/80">
                    Repair status: {repairEvents[0]?.summary}
                  </p>
                )}
                {(retryState?.retryCount ?? currentRun.retryCount) >= 3 ? (
                  <p className="mt-1 text-gray-500">
                    Automatic repair budget is exhausted; Resume with repair requests one operator repair attempt.
                  </p>
                ) : null}
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
              {failureView.currentBlocker || failureView.previousFailures.length > 0 ? (
                <div className="grid gap-2 md:grid-cols-2">
                  {failureView.currentBlocker ? (
                    <div className="rounded border border-amber-900/50 bg-amber-950/20 p-2 text-amber-100">
                      <div className="flex items-center justify-between gap-2">
                        <p className="text-amber-300">Current blocker</p>
                        <span className="text-[10px] text-amber-300/70">
                          {formatTime(failureView.currentBlocker.updatedAt)}
                        </span>
                      </div>
                      <p className="mt-1 whitespace-pre-wrap">{failureView.currentBlocker.text}</p>
                    </div>
                  ) : null}
                  {failureView.previousFailures.length > 0 ? (
                    <div className="rounded border border-gray-800 bg-gray-950/60 p-2 text-gray-100">
                      <div className="flex items-center justify-between gap-2">
                        <p className="text-gray-300">Previous failures</p>
                        <span className="text-[10px] text-gray-500">
                          {failureView.previousFailures.length} item
                          {failureView.previousFailures.length === 1 ? "" : "s"}
                        </span>
                      </div>
                      <ul className="mt-1.5 space-y-1">
                        {failureView.previousFailures.map((failure) => (
                          <li key={`${failure.label}:${failure.text}`} className="space-y-0.5">
                            <div className="flex items-center justify-between gap-2">
                              <p className="text-gray-400">{failure.label}</p>
                              <span className="text-[10px] text-gray-600">
                                {formatTime(failure.updatedAt)}
                              </span>
                            </div>
                            <p className="whitespace-pre-wrap text-gray-200">{failure.text}</p>
                          </li>
                        ))}
                      </ul>
                    </div>
                  ) : null}
                </div>
              ) : null}
            </div>

            {currentRun.phase === "runtime-configuration" &&
              currentRun.status === "blocked" &&
              !deliverySnapshot?.runtimeStatus?.spec && (
              <QuickRuntimeSetup
                projectId={project.id}
                goalRunId={currentRun.id}
                onApplied={() => {
                  // Refresh the goal run view after applying
                  void selectGoalRun(currentRun.id);
                }}
              />
            )}

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
                Refresh logs
              </button>
              <button
                onClick={() => void handleStartRuntime()}
                className="rounded border border-emerald-700 px-3 py-1 text-[11px] text-emerald-300 hover:bg-emerald-950/40"
              >
                Start app
              </button>
              <button
                onClick={() => void handleStopRuntime()}
                className="rounded border border-red-700 px-3 py-1 text-[11px] text-red-300 hover:bg-red-950/40"
              >
                Stop app
              </button>
              {currentRun.status === "paused" ||
              currentRun.status === "interrupted" ||
              currentRun.status === "blocked" ||
              currentRun.status === "failed" ? (
                <button
                  onClick={() => void handleResumeGoal()}
                  disabled={orchestrating}
                  className="rounded border border-emerald-700 px-3 py-1 text-[11px] text-emerald-300 hover:bg-emerald-950/40 disabled:opacity-50"
                >
                  {orchestrating ? "Running…" : "Resume with repair"}
                </button>
              ) : null}
              {currentRun.phase === "verification" &&
              (currentRun.status === "blocked" ||
                currentRun.status === "failed" ||
                currentRun.status === "completed") ? (
                <button
                  onClick={() => void handleRerunVerification()}
                  disabled={orchestrating}
                  className="rounded border border-sky-700 px-3 py-1 text-[11px] text-sky-300 hover:bg-sky-950/40 disabled:opacity-50"
                  title="Rerun the acceptance suite without invoking repair agent"
                >
                  Rerun checks only
                </button>
              ) : null}
              {currentRun.status === "running" || currentRun.status === "retrying" ? (
                <>
                  <button
                    onClick={() => void handlePauseGoal()}
                    className="rounded border border-yellow-700 px-3 py-1 text-[11px] text-yellow-300 hover:bg-yellow-950/40"
                  >
                    Pause goal
                  </button>
                  <button
                    onClick={() => void handleStopGoal()}
                    className="rounded border border-red-700 px-3 py-1 text-[11px] text-red-300 hover:bg-red-950/40"
                  >
                    Stop goal
                  </button>
                </>
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
                Live runtime status and the freshest tail the app has available.
              </p>
            </div>
          </div>

          {runtimeSnapshot ? (
            <div className="space-y-2 text-[11px]">
              <div className="grid gap-2 md:grid-cols-3">
                <div className="rounded border border-gray-800 bg-gray-950/60 p-2">
                  <p className="text-gray-500">Runtime status</p>
                  <p className="mt-1 text-gray-200">{runtimeSnapshot.session?.status ?? "idle"}</p>
                </div>
                <div className="rounded border border-gray-800 bg-gray-950/60 p-2">
                  <p className="text-gray-500">Runtime session</p>
                  <p className="mt-1 font-mono text-gray-200">{runtimeSnapshot.session?.sessionId ?? "none"}</p>
                </div>
                <div className="rounded border border-gray-800 bg-gray-950/60 p-2">
                  <p className="text-gray-500">Runtime URL</p>
                  <p className="mt-1 font-mono text-gray-200">{runtimeSnapshot.session?.url ?? runtimeSnapshot.spec?.appUrl ?? "none"}</p>
                </div>
              </div>
              <div className="grid gap-2 md:grid-cols-3">
                <div className="rounded border border-gray-800 bg-gray-950/60 p-2">
                  <p className="text-gray-500">Log path</p>
                  <p className="mt-1 font-mono text-gray-200">{runtimeSnapshot.session?.logPath ?? "none"}</p>
                </div>
                <div className="rounded border border-gray-800 bg-gray-950/60 p-2">
                  <p className="text-gray-500">Last runtime error</p>
                  <p className="mt-1 whitespace-pre-wrap text-gray-200">{runtimeSnapshot.session?.lastError ?? "none"}</p>
                </div>
                <div className="rounded border border-gray-800 bg-gray-950/60 p-2">
                  <p className="text-gray-500">Log tail updated</p>
                  <p className="mt-1 text-gray-200">
                    {runtimeLogView.updatedAt ? formatTime(runtimeLogView.updatedAt) : "unknown"}
                  </p>
                </div>
              </div>
              {runtimeLogView.lines.length > 0 ? (
                <div className="rounded border border-gray-800 bg-gray-950/60 p-2">
                  <div className="flex items-center justify-between gap-2">
                    <p className="text-gray-500">Recent runtime logs</p>
                    <span className="text-[10px] text-gray-500">
                      {runtimeLogView.source === "live"
                        ? "live store"
                        : runtimeLogView.source === "snapshot"
                          ? "snapshot fallback"
                          : "empty"}
                    </span>
                  </div>
                  <pre className="mt-2 max-h-48 overflow-y-auto whitespace-pre-wrap rounded border border-gray-800 bg-black/40 p-2 text-[10px] text-gray-300">
                    {runtimeLogView.lines.join("\n")}
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
                Phase-by-phase arc of the selected run.
              </p>
            </div>
          </div>

          {phaseArcs.length > 0 ? (
            <div className="space-y-2">
              {phaseArcs.map((arc) => {
                const isActive =
                  arc.outcome === "running" &&
                  (currentRun?.status === "running" || currentRun?.status === "retrying");
                return (
                  <div
                    key={arc.phase}
                    className={`rounded border px-3 py-2 text-[11px] ${
                      isActive
                        ? "border-cyan-700 bg-cyan-950/25"
                        : arc.outcome === "completed"
                          ? "border-green-900/40 bg-green-950/10"
                          : arc.outcome === "failed"
                            ? "border-red-900/40 bg-red-950/10"
                            : arc.outcome === "blocked"
                              ? "border-amber-900/40 bg-amber-950/10"
                              : "border-gray-800 bg-gray-950/60"
                    }`}
                  >
                    <div className="flex items-center justify-between gap-2">
                      <div className="flex items-center gap-1.5">
                        <span className={
                          arc.outcome === "completed" ? "text-green-400"
                          : arc.outcome === "failed" ? "text-red-400"
                          : arc.outcome === "blocked" ? "text-amber-400"
                          : isActive ? "text-cyan-400 animate-pulse"
                          : "text-gray-600"
                        }>
                          {arc.outcome === "completed" ? "✓"
                           : arc.outcome === "failed" ? "✗"
                           : arc.outcome === "blocked" ? "⊘"
                           : isActive ? "●"
                           : "○"}
                        </span>
                        <p className="font-medium text-gray-200">{arc.label}</p>
                        {arc.repairCount > 0 && (
                          <span className="rounded bg-amber-900/40 px-1.5 py-0.5 text-[10px] text-amber-300">
                            {arc.repairCount} repair{arc.repairCount > 1 ? "s" : ""}
                          </span>
                        )}
                      </div>
                      <div className="flex items-center gap-2 shrink-0">
                        {arc.durationLabel && (
                          <span className="text-[10px] text-gray-500">{arc.durationLabel}</span>
                        )}
                        {isActive && (
                          <span className="rounded bg-cyan-900/50 px-1.5 py-0.5 text-[10px] text-cyan-300">
                            running
                          </span>
                        )}
                      </div>
                    </div>
                    {arc.contextDetail && (
                      <p className="mt-1 text-[10px] text-gray-500">{arc.contextDetail}</p>
                    )}
                    {arc.repairSummaries.length > 0 && (
                      <ul className="mt-1.5 space-y-0.5">
                        {arc.repairSummaries.map((s, i) => (
                          <li key={i} className="flex items-start gap-1.5 text-[10px] text-amber-300/80">
                            <span className="shrink-0">↺</span>
                            <span className="whitespace-pre-wrap">{s}</span>
                          </li>
                        ))}
                      </ul>
                    )}
                  </div>
                );
              })}
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
