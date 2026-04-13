import { useState, useEffect } from "react";
import { useAgentStore } from "../../store/useAgentStore";
import { useProjectStore } from "../../store/useProjectStore";

function StatusPill({ running, success }: { running: boolean; success?: boolean }) {
  if (running) {
    return (
      <span className="rounded px-1.5 py-0.5 text-[9px] font-medium bg-purple-700 text-purple-200 animate-pulse">
        Running
      </span>
    );
  }
  if (success === false) {
    return (
      <span className="rounded px-1.5 py-0.5 text-[9px] font-medium bg-red-900/50 text-red-300">
        Failed
      </span>
    );
  }
  if (success === true) {
    return (
      <span className="rounded px-1.5 py-0.5 text-[9px] font-medium bg-green-900/40 text-green-300">
        Done
      </span>
    );
  }
  return null;
}

interface PieceRowProps {
  pieceId: string;
  name: string;
  onSelect: (id: string) => void;
}

function PieceRow({ pieceId, name, onSelect }: PieceRowProps) {
  const run = useAgentStore((s) => s.runs[pieceId]);

  return (
    <button
      onClick={() => onSelect(pieceId)}
      className="w-full text-left px-3 py-2 hover:bg-gray-800 transition-colors border-b border-gray-800/60 last:border-b-0"
    >
      <div className="flex items-center justify-between gap-2">
        <span className="text-[11px] text-gray-200 truncate">{name}</span>
        {run && (
          <StatusPill running={run.running} success={run.success} />
        )}
      </div>
      {run && !run.running && (
        <div className="flex flex-wrap gap-x-2 mt-0.5">
          {run.usage && (run.usage.input + run.usage.output) > 0 && (
            <span className="text-[10px] text-gray-500">
              {(run.usage.input + run.usage.output).toLocaleString()} tok
            </span>
          )}
          {run.exitCode !== undefined && (
            <span className="text-[10px] text-gray-500">exit {run.exitCode}</span>
          )}
          {run.gitBranch && (
            <span className="text-[10px] text-gray-500 font-mono truncate max-w-[120px]">
              {run.gitBranch}
            </span>
          )}
          {run.validation && !run.validation.passed && (
            <span className="text-[10px] text-red-400">validation failed</span>
          )}
        </div>
      )}
    </button>
  );
}

interface SectionProps {
  title: string;
  pieces: { id: string; name: string }[];
  onSelect: (id: string) => void;
  defaultCollapsed?: boolean;
}

function Section({ title, pieces, onSelect, defaultCollapsed = false }: SectionProps) {
  const [collapsed, setCollapsed] = useState(defaultCollapsed);

  if (pieces.length === 0) return null;

  return (
    <div>
      <button
        onClick={() => setCollapsed(!collapsed)}
        className="w-full flex items-center justify-between px-3 py-1.5 text-[10px] font-medium text-gray-400 uppercase tracking-wide hover:text-gray-200 transition-colors"
      >
        <span>{title}</span>
        <span className="flex items-center gap-1">
          <span className="text-gray-600">{pieces.length}</span>
          <span>{collapsed ? "▶" : "▼"}</span>
        </span>
      </button>
      {!collapsed && (
        <div>
          {pieces.map((p) => (
            <PieceRow key={p.id} pieceId={p.id} name={p.name} onSelect={onSelect} />
          ))}
        </div>
      )}
    </div>
  );
}

export function AgentsPanel() {
  const pieces = useProjectStore((s) => s.pieces);
  const selectPiece = useProjectStore((s) => s.selectPiece);
  const runs = useAgentStore((s) => s.runs);

  // Restore run state for all pieces on mount so the panel is populated
  // without requiring the user to open each piece editor individually.
  useEffect(() => {
    if (pieces.length === 0) return;

    const restore = async () => {
      const { getAgentHistory } = await import("../../api/leaderApi");
      await Promise.allSettled(
        pieces.map(async (piece) => {
          const existing = useAgentStore.getState().runs[piece.id];
          if (existing?.running) return;
          try {
            const history = await getAgentHistory(piece.id);
            const latest = history[0];
            if (!latest) return;
            const meta = latest.metadata ?? {};
            useAgentStore.getState().restoreRun(piece.id, {
              running: false,
              output: latest.outputText,
              usage: meta.usage ?? { input: 0, output: 0 },
              success: meta.success ?? true,
              exitCode: meta.exitCode ?? undefined,
              phaseProposal: meta.phaseProposal ?? undefined,
              phaseChanged: meta.phaseChanged ?? undefined,
              gitBranch: meta.gitBranch ?? undefined,
              gitCommitSha: meta.gitCommitSha ?? undefined,
              gitDiffStat: meta.gitDiffStat ?? undefined,
              iterationCount: 1,
              validation: meta.validation ?? undefined,
              validationOutput: meta.validation?.output ?? "",
            });
          } catch {
            // Non-fatal — best-effort restore.
          }
        }),
      );
    };

    void restore();
  // Re-run whenever the piece list changes (e.g. new piece created by CTO).
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pieces.map((p) => p.id).join(",")]);

  const running = pieces.filter((p) => runs[p.id]?.running === true);
  const failed = pieces.filter(
    (p) => runs[p.id]?.running === false && runs[p.id]?.success === false,
  );
  const completed = pieces.filter(
    (p) => runs[p.id]?.running === false && runs[p.id]?.success === true,
  );

  const hasAnyRuns = running.length + failed.length + completed.length > 0;

  const handleSelect = (id: string) => {
    selectPiece(id);
  };

  return (
    <div className="flex flex-col h-full overflow-y-auto">
      <div className="px-3 py-2 border-b border-gray-800">
        <p className="text-[11px] text-gray-400">Per-piece agent activity</p>
      </div>

      {!hasAnyRuns ? (
        <div className="flex flex-1 items-center justify-center px-4">
          <p className="text-[11px] text-gray-500 text-center">
            No agent runs yet. Run an agent from a piece to see activity here.
          </p>
        </div>
      ) : (
        <div className="flex flex-col">
          <Section title="Running now" pieces={running} onSelect={handleSelect} />
          <Section title="Recently failed" pieces={failed} onSelect={handleSelect} />
          <Section title="Recently completed" pieces={completed} onSelect={handleSelect} />
        </div>
      )}
    </div>
  );
}
