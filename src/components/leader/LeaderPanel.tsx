import { useState, useRef, useEffect } from "react";
import { useLeaderStore } from "../../store/useLeaderStore";
import { useProjectStore } from "../../store/useProjectStore";
import { onLeaderPlanChunk } from "../../api/tauriApi";
import { PlanTaskCard } from "./PlanTaskCard";
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
  const {
    currentPlan,
    generating,
    streamOutput,
    generatePlan,
    loadPlans,
    approvePlan,
    rejectPlan,
    updateTaskStatus,
    appendChunk,
  } = useLeaderStore();

  const [guidance, setGuidance] = useState("");
  const scrollRef = useRef<HTMLDivElement>(null);

  // Load plans when project changes
  useEffect(() => {
    if (project) {
      loadPlans(project.id);
    }
  }, [project?.id]);

  // Set up streaming listener
  useEffect(() => {
    let unlistenFn: (() => void) | null = null;

    onLeaderPlanChunk((payload) => {
      if (!payload.done) {
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
                    />
                  ))}
              </div>
            )}

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
