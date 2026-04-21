import { useEffect, useMemo, useState } from "react";
import { useProjectStore } from "../../store/useProjectStore";
import { useGoalRunStore } from "../../store/useGoalRunStore";
import { useAgentStore } from "../../store/useAgentStore";
import type { TeamBrief } from "../../types";

/// Compact sticky status strip for the editor view. Pure aggregation of
/// state already in the app — no new backend calls. Hides when there's no
/// project loaded. Each chip is a clickable affordance that opens the
/// relevant panel.

function chipClass(tone: "neutral" | "ok" | "warn" | "bad" | "info") {
  switch (tone) {
    case "ok":
      return "border-emerald-800/60 bg-emerald-950/40 text-emerald-300";
    case "warn":
      return "border-amber-800/60 bg-amber-950/40 text-amber-300";
    case "bad":
      return "border-red-800/60 bg-red-950/40 text-red-300";
    case "info":
      return "border-cyan-800/60 bg-cyan-950/40 text-cyan-300";
    default:
      return "border-gray-700 bg-gray-900 text-gray-300";
  }
}

function runtimeTone(status: string | undefined): "neutral" | "ok" | "warn" | "bad" {
  switch (status) {
    case "running":
      return "ok";
    case "starting":
    case "stopping":
      return "warn";
    case "failed":
      return "bad";
    default:
      return "neutral";
  }
}

function goalRunTone(status: string | undefined): "neutral" | "ok" | "warn" | "bad" | "info" {
  switch (status) {
    case "running":
    case "retrying":
      return "info";
    case "completed":
      return "ok";
    case "paused":
      return "warn";
    case "blocked":
    case "failed":
    case "interrupted":
      return "bad";
    default:
      return "neutral";
  }
}

export function ProjectStatusBar({
  onOpenTab,
}: {
  onOpenTab?: (tab: "delivery" | "activity" | "agents") => void;
}) {
  const project = useProjectStore((s) => s.project);
  const runtimeStatus = useGoalRunStore((s) => s.runtimeStatus);
  const currentGoalRun = useGoalRunStore((s) => s.currentGoalRun);
  const phaseActivity = useGoalRunStore((s) => s.phaseActivity);
  const goalRuns = useGoalRunStore((s) => s.goalRuns);
  const runs = useAgentStore((s) => s.runs);

  const counts = useMemo(() => {
    let running = 0;
    let success = 0;
    let failed = 0;
    for (const r of Object.values(runs)) {
      if (r.running) running += 1;
      else if (r.success === true) success += 1;
      else if (r.success === false) failed += 1;
    }
    return { running, success, failed };
  }, [runs]);

  const attentionCount = useMemo(() => {
    return goalRuns.filter((r) => r.attentionRequired).length;
  }, [goalRuns]);

  const [teamBriefs, setTeamBriefs] = useState<TeamBrief[]>([]);
  const [teamsOpen, setTeamsOpen] = useState(false);
  useEffect(() => {
    if (!project) {
      setTeamBriefs([]);
      return;
    }
    let cancelled = false;
    void (async () => {
      try {
        const { listTeamBriefs } = await import("../../api/projectApi");
        const briefs = await listTeamBriefs(project.id);
        if (!cancelled) setTeamBriefs(briefs);
      } catch {
        // Best-effort — teams chip just won't render.
      }
    })();
    return () => {
      cancelled = true;
    };
    // Refresh whenever the project changes or a run finishes (goalRuns
    // updates as runs complete), so the chip reflects fresh briefs.
  }, [project, goalRuns.length, phaseActivity?.updatedAt]);

  if (!project) return null;

  const runtimeLabel = runtimeStatus?.session?.status ?? "idle";
  const goalLabel = currentGoalRun
    ? `${currentGoalRun.status} · ${currentGoalRun.phase}`
    : "no run";

  return (
    <div className="flex shrink-0 items-center gap-2 border-b border-gray-800 bg-gray-950 px-3 py-1.5 text-[11px]">
      <button
        type="button"
        onClick={() => onOpenTab?.("delivery")}
        className={`rounded border px-2 py-0.5 ${chipClass(runtimeTone(runtimeLabel))} hover:brightness-125`}
        title="Jump to runtime section"
      >
        runtime: {runtimeLabel}
      </button>

      <button
        type="button"
        onClick={() => onOpenTab?.("delivery")}
        className={`rounded border px-2 py-0.5 ${chipClass(goalRunTone(currentGoalRun?.status))} hover:brightness-125`}
        title="Jump to delivery"
      >
        run: {goalLabel}
      </button>

      {phaseActivity && (
        <span
          className="truncate rounded border border-cyan-900/50 bg-cyan-950/20 px-2 py-0.5 text-cyan-200"
          title={phaseActivity.message}
        >
          {phaseActivity.status === "step" ? "›" : "●"} {phaseActivity.message}
          {phaseActivity.stepIndex != null && phaseActivity.stepTotal != null
            ? ` (${phaseActivity.stepIndex}/${phaseActivity.stepTotal})`
            : ""}
        </span>
      )}

      {attentionCount > 0 && (
        <button
          type="button"
          onClick={() => onOpenTab?.("delivery")}
          className={`rounded border px-2 py-0.5 ${chipClass("bad")} hover:brightness-125`}
          title="Runs requiring attention"
        >
          ⚠ attention {attentionCount}
        </button>
      )}

      <button
        type="button"
        onClick={() => onOpenTab?.("agents")}
        className={`rounded border px-2 py-0.5 ${chipClass("neutral")} hover:brightness-125`}
        title="Per-piece agent runs"
      >
        pieces · {counts.running} running · {counts.success} ok · {counts.failed} failed
      </button>

      {teamBriefs.length > 0 && (
        <div className="relative">
          <button
            type="button"
            onClick={() => setTeamsOpen((v) => !v)}
            className={`rounded border px-2 py-0.5 ${chipClass("neutral")} hover:brightness-125`}
            title="Cross-team briefs — click to inspect"
          >
            teams · {teamBriefs.length}
          </button>
          {teamsOpen && (
            <div className="absolute left-0 top-full z-50 mt-1 max-h-96 w-80 overflow-y-auto rounded border border-gray-700 bg-gray-950 p-2 text-[11px] shadow-lg">
              {teamBriefs.map((brief) => (
                <div
                  key={brief.team}
                  className="mb-2 rounded border border-gray-800 bg-gray-900/60 p-2 last:mb-0"
                >
                  <div className="mb-1 flex items-center justify-between">
                    <span className="font-medium text-gray-200">{brief.team}</span>
                    <span className="text-[9px] text-gray-500">
                      updated {new Date(brief.updatedAt).toLocaleString()}
                    </span>
                  </div>
                  <pre className="max-h-40 overflow-y-auto whitespace-pre-wrap font-mono text-[10px] text-gray-400">
                    {brief.content}
                  </pre>
                </div>
              ))}
            </div>
          )}
        </div>
      )}

      <button
        type="button"
        onClick={() => onOpenTab?.("activity")}
        className="ml-auto rounded border border-gray-800 px-2 py-0.5 text-gray-400 hover:bg-gray-900 hover:text-gray-200"
        title="Open activity feed"
      >
        Activity →
      </button>
    </div>
  );
}
