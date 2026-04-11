import { useState, useCallback, useEffect } from "react";
import type { PlanTask, TaskStatus } from "../../types";
import { useProjectStore } from "../../store/useProjectStore";
import { useLeaderStore } from "../../store/useLeaderStore";
import { useAgentStore } from "../../store/useAgentStore";

const priorityColors: Record<string, string> = {
  critical: "bg-red-600 text-white",
  high: "bg-orange-600 text-white",
  medium: "bg-yellow-600 text-white",
  low: "bg-gray-600 text-gray-200",
};

const statusLabels: Record<string, string> = {
  pending: "Pending",
  "in-progress": "In Progress",
  complete: "Complete",
  skipped: "Skipped",
};

const nextStatus: Record<string, TaskStatus> = {
  pending: "in-progress",
  "in-progress": "complete",
  complete: "pending",
  skipped: "pending",
};

export function PlanTaskCard({
  task,
  approved,
  planId,
  onStatusChange,
  runningAll,
}: {
  task: PlanTask;
  approved: boolean;
  planId: string;
  onStatusChange: (taskId: string, status: TaskStatus) => void;
  runningAll?: boolean;
}) {
  const [expanded, setExpanded] = useState(false);
  const [showOutput, setShowOutput] = useState(true);
  const selectPiece = useProjectStore((s) => s.selectPiece);
  const pieces = useProjectStore((s) => s.pieces);
  const runTask = useLeaderStore((s) => s.runTask);
  const agentRun = useAgentStore((s) => s.runs[task.pieceId]);
  const currentPiece = pieces.find((p) => p.id === task.pieceId);
  const phaseMatches = !task.suggestedPhase || currentPiece?.phase === task.suggestedPhase;

  const isRunning = agentRun?.running ?? false;
  const hasOutput = !!agentRun?.output;
  const canRun = approved && !!task.pieceId && !isRunning && task.status !== "complete" && !runningAll;

  useEffect(() => {
    let cancelled = false;
    const loadHistory = async () => {
      if (!task.pieceId) return;
      const existing = useAgentStore.getState().runs[task.pieceId];
      if (existing?.running || existing?.output) return;

      const { getAgentHistory } = await import("../../api/tauriApi");
      try {
        const history = await getAgentHistory(task.pieceId);
        const latest = history[0];
        if (!latest || cancelled) return;
        const metadata = latest.metadata ?? {};
        const validation = metadata.validation ?? undefined;
        useAgentStore.getState().restoreRun(task.pieceId, {
          running: false,
          output: latest.outputText,
          usage: metadata.usage ?? { input: 0, output: 0 },
          success: metadata.success ?? true,
          exitCode: metadata.exitCode ?? undefined,
          phaseProposal: metadata.phaseProposal ?? undefined,
          phaseChanged: metadata.phaseChanged ?? undefined,
          gitBranch: metadata.gitBranch ?? undefined,
          gitCommitSha: metadata.gitCommitSha ?? undefined,
          gitDiffStat: metadata.gitDiffStat ?? undefined,
          iterationCount: 1,
          validation,
          validationOutput: validation?.output ?? "",
        });
      } catch {
        // Best-effort recovery only.
      }
    };

    loadHistory();
    return () => {
      cancelled = true;
    };
  }, [task.pieceId]);

  const handleRun = () => {
    runTask(planId, task);
  };

  const outputRef = useCallback(
    (el: HTMLPreElement | null) => {
      if (el) el.scrollTop = el.scrollHeight;
    },
    [agentRun?.output],
  );

  return (
    <div className="rounded border border-gray-700 bg-gray-800/50 p-2">
      <div className="flex items-start gap-1.5">
        <span
          className={`mt-0.5 shrink-0 rounded px-1 py-0.5 text-[9px] font-bold uppercase ${priorityColors[task.priority] ?? priorityColors.medium}`}
        >
          {task.priority}
        </span>
        <div className="flex-1 min-w-0">
          <button
            onClick={() => setExpanded(!expanded)}
            className="text-left text-xs font-medium text-gray-100 hover:text-white w-full"
          >
            {task.title}
          </button>
          <button
            onClick={() => task.pieceId && selectPiece(task.pieceId)}
            className="text-[10px] text-blue-400 hover:text-blue-300 block truncate"
            title={`Select "${task.pieceName}" on canvas`}
          >
            {task.pieceName}
          </button>
        </div>
        <div className="flex items-center gap-1 shrink-0">
          {canRun && (
            <button
              onClick={handleRun}
              className="rounded bg-purple-600 px-1.5 py-0.5 text-[9px] font-medium text-white hover:bg-purple-500 transition-colors"
              title="Run agent for this task"
            >
              Run ▶
            </button>
          )}
          {isRunning && (
            <span className="rounded bg-purple-700 px-1.5 py-0.5 text-[9px] font-medium text-purple-200 animate-pulse">
              Running...
            </span>
          )}
          {approved && !isRunning && (
            <button
              onClick={() => onStatusChange(task.id, nextStatus[task.status] ?? "pending")}
              className={`rounded px-1.5 py-0.5 text-[9px] font-medium ${
                task.status === "complete"
                  ? "bg-green-700 text-green-100"
                  : task.status === "in-progress"
                    ? "bg-blue-700 text-blue-100"
                    : task.status === "skipped"
                      ? "bg-gray-600 text-gray-300 line-through"
                      : "bg-gray-700 text-gray-300"
              }`}
              title="Click to cycle status"
            >
              {task.status === "complete" ? "✓ Complete" : (statusLabels[task.status] ?? task.status)}
            </button>
          )}
        </div>
      </div>

      {task.suggestedPhase && (
        <span
          className={`mt-1 inline-block rounded px-1 py-0.5 text-[9px] ${
            phaseMatches
              ? "bg-green-900/50 text-green-400"
              : "bg-amber-900/50 text-amber-400"
          }`}
          title={
            phaseMatches
              ? `Piece is already in ${task.suggestedPhase} phase`
              : `Running this task will set the piece to ${task.suggestedPhase}`
          }
        >
          Phase: {task.suggestedPhase}
        </span>
      )}

      {expanded && (
        <div className="mt-1.5 text-[11px] text-gray-400 leading-relaxed">
          {task.description}
          {task.dependencies.length > 0 && (
            <div className="mt-1 text-[10px] text-gray-500">
              Depends on: {task.dependencies.join(", ")}
            </div>
          )}
        </div>
      )}

      {/* Inline agent output */}
      {hasOutput && (
        <div className="mt-1.5 border-t border-gray-700 pt-1.5">
          <button
            onClick={() => setShowOutput(!showOutput)}
            className="text-[9px] text-gray-500 hover:text-gray-400 mb-1"
          >
            {showOutput ? "▾ Hide output" : "▸ Show output"}
          </button>
          {showOutput && (
            <pre
              ref={outputRef}
              className="max-h-32 overflow-y-auto rounded bg-gray-900 p-1.5 text-[10px] text-gray-300 font-mono whitespace-pre-wrap break-words leading-relaxed"
            >
              {agentRun.output}
            </pre>
          )}
          {agentRun.usage && !isRunning && (
            <p className="text-[9px] text-gray-600 mt-0.5">
              Tokens: {agentRun.usage.input} in / {agentRun.usage.output} out
            </p>
          )}
          {agentRun.validation && !isRunning && (
            <p className={`text-[9px] mt-0.5 ${agentRun.validation.passed ? "text-green-500" : "text-red-400"}`}>
              Validation: {agentRun.validation.passed ? "passed" : `failed (exit ${agentRun.validation.exitCode})`}
            </p>
          )}
        </div>
      )}
    </div>
  );
}
