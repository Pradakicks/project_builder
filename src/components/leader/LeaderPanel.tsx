import { useState, useRef, useEffect } from "react";
import { useLeaderStore } from "../../store/useLeaderStore";
import { useProjectStore } from "../../store/useProjectStore";
import { useGoalRunStore } from "../../store/useGoalRunStore";
import { onLeaderPlanChunk } from "../../api/leaderApi";
import { PlanTaskCard } from "./PlanTaskCard";
import { MergeSection } from "./MergeSection";
import { Markdown } from "../ui/Markdown";
import type { TaskStatus } from "../../types";

const statusBadge: Record<string, { bg: string; label: string }> = {
  generating: { bg: "bg-yellow-600", label: "Generating..." },
  draft: { bg: "bg-blue-600", label: "Draft" },
  approved: { bg: "bg-green-600", label: "Approved" },
  rejected: { bg: "bg-red-600", label: "Rejected" },
  superseded: { bg: "bg-gray-600", label: "Superseded" },
};

export function LeaderPanel({
  open,
  onToggle,
  embedded,
}: {
  open: boolean;
  onToggle: () => void;
  embedded?: boolean;
}) {
  const project = useProjectStore((s) => s.project);
  const autonomyMode = project?.settings.autonomyMode ?? "autopilot";
  const runtimeConfigured = Boolean(project?.settings.runtimeSpec?.runCommand?.trim());
  const currentGoalRun = useGoalRunStore((s) => s.currentGoalRun);
  const runtimeStatus = useGoalRunStore((s) => s.runtimeStatus);
  const runtimeLogs = useGoalRunStore((s) => s.runtimeLogs);
  const {
    currentPlan,
    generating,
    streamOutput,
    runningAll,
    runAllProgress,
    runAllStatus,
    runAllError,
    mergeStatus,
    mergeError,
    reviewStatus,
    reviewError,
    generatePlan,
    loadPlans,
    approvePlan,
    rejectPlan,
    updateTaskStatus,
    runAllTasks,
    cancelRunAll,
    appendChunk,
  } = useLeaderStore();

  const [guidance, setGuidance] = useState("");
  const scrollRef = useRef<HTMLDivElement>(null);

  // Load plans when project changes
  useEffect(() => {
    useLeaderStore.getState().reset();
    if (project) {
      loadPlans(project.id);
    }
  }, [project?.id]);

  // Set up streaming listener
  useEffect(() => {
    let unlistenFn: (() => void) | null = null;

    onLeaderPlanChunk((payload) => {
      if (!payload.done && payload.projectId === useLeaderStore.getState().projectId) {
        appendChunk(payload.chunk);
      }
    }).then((fn) => {
      unlistenFn = fn;
    });

    return () => {
      unlistenFn?.();
    };
  }, []);

  // Auto-scroll during generation
  useEffect(() => {
    scrollRef.current?.scrollTo(0, scrollRef.current.scrollHeight);
  }, [streamOutput, currentPlan?.tasks.length]);

  const handleGenerate = () => {
    if (!project || generating) return;
    generatePlan(project.id, guidance);
    setGuidance("");
  };

  const handleTaskStatusChange = (taskId: string, status: TaskStatus) => {
    if (currentPlan) {
      updateTaskStatus(currentPlan.id, taskId, status);
    }
  };

  if (!open) {
    return (
      <button
        onClick={onToggle}
        className="absolute left-2 top-14 z-10 rounded-lg border border-gray-700 bg-gray-900 p-2 text-gray-400 hover:text-gray-200 shadow-lg"
        title="Open Leader Agent"
      >
        <svg
          width="18"
          height="18"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
        >
          <path d="M9 5H7a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V7a2 2 0 0 0-2-2h-2" />
          <rect x="9" y="3" width="6" height="4" rx="1" />
          <path d="M9 14l2 2 4-4" />
        </svg>
      </button>
    );
  }

  const badge = currentPlan
    ? statusBadge[currentPlan.status] ?? statusBadge.draft
    : null;
  const taskCounts = currentPlan
    ? currentPlan.tasks.reduce(
        (acc, task) => {
          acc[task.status] += 1;
          return acc;
        },
        { pending: 0, "in-progress": 0, complete: 0, skipped: 0 } as Record<string, number>,
      )
    : null;

  const posture = (() => {
    if (!currentPlan) {
      return {
        bg: "bg-gray-600",
        label: "No plan loaded",
        message: "Create or open a plan to see task, merge, and review progress.",
      };
    }

    if (runningAll) {
      return {
        bg: "bg-purple-600",
        label: "Running tasks",
        message: `Batch execution is active${runAllProgress ? ` (${runAllProgress})` : ""}. Tasks stop on the first failure.`,
      };
    }

    if (runAllStatus === "failed" && runAllError) {
      return {
        bg: "bg-red-600",
        label: "Task run stopped",
        message: runAllError,
      };
    }

    if (runAllStatus === "cancelled") {
      return {
        bg: "bg-amber-600",
        label: "Run cancelled",
        message: "The batch was stopped before all pending tasks completed.",
      };
    }

    if (mergeStatus === "merging") {
      return {
        bg: "bg-emerald-600",
        label: "Merging branches",
        message: "Piece branches are being merged back to main in task order.",
      };
    }

    if (mergeStatus === "conflict") {
      return {
        bg: "bg-red-600",
        label: "Merge paused",
        message: mergeError ?? "A merge conflict needs operator attention before the plan can continue.",
      };
    }

    if (mergeStatus === "failed") {
      return {
        bg: "bg-red-600",
        label: "Merge failed",
        message: mergeError ?? "Merge failed before completion.",
      };
    }

    if (reviewStatus === "running") {
      return {
        bg: "bg-blue-600",
        label: "Reviewing",
        message: "Integration review is checking the merged result for compatibility issues.",
      };
    }

    if (reviewStatus === "failed") {
      return {
        bg: "bg-red-600",
        label: "Review failed",
        message: reviewError ?? "Integration review failed before completion.",
      };
    }

    if (reviewStatus === "complete") {
      return {
        bg: "bg-green-600",
        label: "Review complete",
        message: "The merged plan has been reviewed. Check the review output below for any follow-up work.",
      };
    }

    if (currentPlan.status === "approved" && (taskCounts?.pending ?? 0) > 0) {
      return {
        bg: "bg-blue-600",
        label: "Ready to run",
        message: "Approved tasks are queued. Balanced mode keeps execution visible and review-gated.",
      };
    }

    if (currentPlan.status === "draft") {
      return {
        bg: "bg-yellow-600",
        label: "Plan draft",
        message: "Review the plan, then approve it before task execution begins.",
      };
    }

    if (currentPlan.status === "rejected") {
      return {
        bg: "bg-red-600",
        label: "Plan rejected",
        message: "The current plan was rejected and will not execute until a new draft is generated.",
      };
    }

    return {
      bg: "bg-gray-600",
      label: "Idle",
      message: "Use the plan controls to move the project forward.",
    };
  })();

  const content = (
    <>
      {!embedded && (
        <div className="flex items-center justify-between border-b border-gray-800 px-3 py-2">
          <span className="text-xs font-semibold text-gray-300">
            Work Plan
          </span>
          <button
            onClick={onToggle}
            className="text-xs text-gray-500 hover:text-gray-300"
          >
            Collapse
          </button>
        </div>
      )}

      {/* Guidance input */}
      <div className="border-b border-gray-800 p-2 space-y-1.5">
        <textarea
          value={guidance}
          onChange={(e) => setGuidance(e.target.value)}
          placeholder="Optional guidance for the plan..."
          rows={2}
          disabled={generating}
          className="w-full rounded border border-gray-700 bg-gray-800 px-2 py-1.5 text-xs text-gray-100 placeholder-gray-600 focus:border-blue-500 focus:outline-none disabled:opacity-50 resize-none"
        />
        <button
          onClick={handleGenerate}
          disabled={generating || !project}
          className="w-full rounded bg-purple-600 px-2.5 py-1.5 text-xs font-medium text-white hover:bg-purple-500 disabled:opacity-50"
        >
          {generating ? "Generating..." : "Generate Plan"}
        </button>
      </div>

      {/* Scrollable content */}
      <div ref={scrollRef} className="flex-1 overflow-y-auto p-3 space-y-3">
        {/* Streaming output during generation */}
        {generating && streamOutput && (
          <div className="rounded border border-gray-700 bg-gray-800/50 p-2">
            <p className="text-[10px] text-gray-500 mb-1">Planning...</p>
            <Markdown content={streamOutput} />
          </div>
        )}

        {generating && !streamOutput && (
          <p className="text-[11px] text-gray-500 text-center mt-8">
            Analyzing project diagram...
          </p>
        )}

        {/* Plan display */}
        {currentPlan && !generating && (
          <>
            {/* Status badge + version */}
            <div className="flex items-center gap-2">
              {badge && (
                <span
                  className={`rounded px-1.5 py-0.5 text-[10px] font-medium text-white ${badge.bg}`}
                >
                  {badge.label}
                </span>
              )}
              <span className="text-[10px] text-gray-500">
                v{currentPlan.version}
              </span>
            </div>

            <div className="rounded border border-gray-700 bg-gray-800/50 p-2 space-y-1.5 text-[10px]">
              <div className="flex items-center justify-between gap-2">
                <span className="font-semibold uppercase tracking-wider text-gray-400">
                  Execution posture
                </span>
                <span className={`rounded px-1.5 py-0.5 font-medium text-white ${posture.bg}`}>
                  {posture.label}
                </span>
              </div>
              <p className="text-gray-300 leading-relaxed">{posture.message}</p>
              <div className="flex flex-wrap gap-1.5">
                <span
                  className={`rounded border border-gray-700 px-1.5 py-0.5 ${
                    autonomyMode === "autopilot"
                      ? "bg-emerald-950/40 text-emerald-300"
                      : autonomyMode === "guided"
                        ? "bg-blue-950/40 text-blue-300"
                        : "bg-gray-900 text-gray-400"
                  }`}
                >
                  Autonomy: {autonomyMode}
                </span>
                {currentGoalRun && (
                  <span className="rounded border border-gray-700 bg-gray-900 px-1.5 py-0.5 text-gray-400">
                    Goal run: {currentGoalRun.status} / {currentGoalRun.phase}
                  </span>
                )}
                {taskCounts && (
                  <span className="rounded border border-gray-700 bg-gray-900 px-1.5 py-0.5 text-gray-400">
                    Tasks: {taskCounts.complete + taskCounts.skipped}/{currentPlan.tasks.length} done
                  </span>
                )}
                <span className="rounded border border-gray-700 bg-gray-900 px-1.5 py-0.5 text-gray-400">
                  Merge: {mergeStatus === "idle" ? "waiting" : mergeStatus}
                </span>
                <span className="rounded border border-gray-700 bg-gray-900 px-1.5 py-0.5 text-gray-400">
                  Review: {reviewStatus === "idle" ? "waiting" : reviewStatus}
                </span>
                <span
                  className={`rounded border border-gray-700 px-1.5 py-0.5 ${
                    runtimeStatus?.session?.status === "running"
                      ? "bg-green-950/40 text-green-300"
                      : runtimeConfigured
                        ? "bg-gray-900 text-gray-400"
                        : "bg-red-950/40 text-red-300"
                  }`}
                >
                  Runtime: {runtimeStatus?.session?.status ?? (runtimeConfigured ? "ready" : "missing")}
                </span>
              </div>
              {(runAllError || mergeError || reviewError) && (
                <div className="space-y-0.5 rounded border border-gray-700 bg-gray-950/70 px-2 py-1 text-[10px]">
                  {runAllError && <p className="text-red-300">{runAllError}</p>}
                  {mergeError && <p className="text-red-300">{mergeError}</p>}
                  {reviewError && <p className="text-red-300">{reviewError}</p>}
                </div>
              )}
              {currentGoalRun?.runtimeStatusSummary && (
                <div className="rounded border border-gray-700 bg-gray-950/70 px-2 py-1 text-[10px] text-gray-300">
                  <p className="text-gray-500">Runtime</p>
                  <p>{currentGoalRun.runtimeStatusSummary}</p>
                </div>
              )}
              {currentGoalRun?.verificationSummary && (
                <div className="rounded border border-gray-700 bg-gray-950/70 px-2 py-1 text-[10px] text-gray-300">
                  <p className="text-gray-500">Verification</p>
                  <pre className="whitespace-pre-wrap">{currentGoalRun.verificationSummary}</pre>
                </div>
              )}
              {runtimeStatus?.session?.url && (
                <div className="rounded border border-gray-700 bg-gray-950/70 px-2 py-1 text-[10px] text-gray-300">
                  <p className="text-gray-500">App URL</p>
                  <p className="font-mono">{runtimeStatus.session.url}</p>
                </div>
              )}
              {runtimeLogs.length > 0 && (
                <div className="rounded border border-gray-700 bg-gray-950/70 px-2 py-1 text-[10px] text-gray-300">
                  <p className="text-gray-500">Runtime logs</p>
                  <pre className="max-h-32 overflow-y-auto whitespace-pre-wrap">{runtimeLogs.join("\n")}</pre>
                </div>
              )}
            </div>

            {/* Run All button for approved plans */}
            {currentPlan.status === "approved" && currentPlan.tasks.length > 0 && (
              <div className="flex items-center gap-2">
                {!runningAll ? (
                  <button
                    onClick={() => runAllTasks(currentPlan.id)}
                    className="rounded bg-purple-600 px-3 py-1 text-xs font-medium text-white hover:bg-purple-500 transition-colors"
                  >
                    Run All ▶
                  </button>
                ) : (
                  <>
                    <span className="text-xs text-purple-300 animate-pulse">
                      Running... ({runAllProgress})
                    </span>
                    <button
                      onClick={cancelRunAll}
                      className="rounded bg-red-700 px-2 py-0.5 text-xs text-white hover:bg-red-600 transition-colors"
                    >
                      Stop
                    </button>
                  </>
                )}
              </div>
            )}

            {/* Summary */}
            {currentPlan.summary && (
              <Markdown content={currentPlan.summary} />
            )}

            {/* Tasks */}
            {currentPlan.tasks.length > 0 && (
              <div className="space-y-1.5">
                <p className="text-[10px] font-semibold text-gray-400 uppercase tracking-wider">
                  Tasks ({currentPlan.tasks.length})
                </p>
                {[...currentPlan.tasks]
                  .sort((a, b) => a.order - b.order)
                  .map((task) => (
                    <PlanTaskCard
                      key={task.id}
                      task={task}
                      approved={currentPlan.status === "approved"}
                      planId={currentPlan.id}
                      onStatusChange={handleTaskStatusChange}
                      runningAll={runningAll}
                    />
                  ))}
              </div>
            )}

            {/* Merge & Review section — shown when all tasks are done */}
            {currentPlan.status === "approved" &&
              currentPlan.tasks.length > 0 &&
              currentPlan.tasks.every(
                (t) => t.status === "complete" || t.status === "skipped",
              ) && <MergeSection planId={currentPlan.id} />}

            {currentPlan.tasks.length === 0 && currentPlan.rawOutput && (
              <div className="rounded border border-gray-700 bg-gray-800/50 p-2">
                <p className="text-[10px] text-yellow-500 mb-1">
                  Could not parse structured tasks. Raw output:
                </p>
                <pre className="text-[11px] text-gray-300 whitespace-pre-wrap break-words font-mono leading-relaxed">
                  {currentPlan.rawOutput}
                </pre>
              </div>
            )}
          </>
        )}

        {/* Empty state */}
        {!currentPlan && !generating && (
          <p className="text-[11px] text-gray-600 text-center mt-8">
            Generate a work plan to get an AI-recommended task breakdown for your
            project.
          </p>
        )}
      </div>

      {/* Approve / Reject buttons */}
      {currentPlan && currentPlan.status === "draft" && !generating && (
        <div className="border-t border-gray-800 p-2 flex gap-1.5">
          <button
            onClick={() => approvePlan(currentPlan.id)}
            className="flex-1 rounded bg-green-600 px-2.5 py-1.5 text-xs font-medium text-white hover:bg-green-500"
          >
            Approve
          </button>
          <button
            onClick={() => rejectPlan(currentPlan.id)}
            className="flex-1 rounded bg-red-600/80 px-2.5 py-1.5 text-xs font-medium text-white hover:bg-red-500"
          >
            Reject
          </button>
        </div>
      )}
    </>
  );

  if (embedded) {
    return content;
  }

  return (
    <div className="flex w-72 shrink-0 flex-col border-r border-gray-800 bg-gray-900">
      {content}
    </div>
  );
}
