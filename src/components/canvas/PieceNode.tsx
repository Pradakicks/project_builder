import { Handle, Position, type NodeProps } from "@xyflow/react";
import type { Phase, PieceInterface } from "../../types";
import { useAgentStore } from "../../store/useAgentStore";

interface PieceNodeData {
  label: string;
  pieceType: string;
  phase: Phase;
  color: string | null;
  interfaces: PieceInterface[];
  activeAgents?: string[];
  team?: string | null;
  [key: string]: unknown;
}

const ROLE_LABELS: Record<string, string> = {
  implementation: "impl",
  testing: "test",
  review: "rev",
};

/// Stable color per team name. Deterministic hash so the same team always
/// gets the same hue across sessions. Tailwind-compatible.
const TEAM_PALETTE: Array<{ bg: string; text: string; border: string }> = [
  { bg: "bg-sky-900/40", text: "text-sky-300", border: "border-sky-800/60" },
  { bg: "bg-emerald-900/40", text: "text-emerald-300", border: "border-emerald-800/60" },
  { bg: "bg-amber-900/40", text: "text-amber-300", border: "border-amber-800/60" },
  { bg: "bg-violet-900/40", text: "text-violet-300", border: "border-violet-800/60" },
  { bg: "bg-rose-900/40", text: "text-rose-300", border: "border-rose-800/60" },
  { bg: "bg-teal-900/40", text: "text-teal-300", border: "border-teal-800/60" },
  { bg: "bg-pink-900/40", text: "text-pink-300", border: "border-pink-800/60" },
  { bg: "bg-lime-900/40", text: "text-lime-300", border: "border-lime-800/60" },
];

function teamColor(team: string): { bg: string; text: string; border: string } {
  let hash = 0;
  for (let i = 0; i < team.length; i += 1) {
    hash = (hash * 31 + team.charCodeAt(i)) >>> 0;
  }
  return TEAM_PALETTE[hash % TEAM_PALETTE.length]!;
}

const phaseColors: Record<string, string> = {
  design: "bg-yellow-500/20 text-yellow-400",
  review: "bg-purple-500/20 text-purple-400",
  approved: "bg-green-500/20 text-green-400",
  implementing: "bg-blue-500/20 text-blue-400",
};

export function PieceNode({ id, data, selected }: NodeProps) {
  const nodeData = data as unknown as PieceNodeData;
  const running = useAgentStore((s) => s.runs[id]?.running);
  const success = useAgentStore((s) => s.runs[id]?.success);
  const validationFailed = useAgentStore(
    (s) => s.runs[id]?.validation?.passed === false,
  );
  const borderColor = nodeData.color ?? "#3b82f6";
  const interfaces = nodeData.interfaces ?? [];
  const inPorts = interfaces.filter((i) => i.direction === "in");
  const outPorts = interfaces.filter((i) => i.direction === "out");
  const hasCustomPorts = interfaces.length > 0;

  return (
    <div
      className={`min-w-[140px] rounded-lg border-2 bg-gray-900 px-3 py-2 shadow-lg transition-shadow ${
        selected ? "shadow-blue-500/30 ring-1 ring-blue-500" : ""
      }`}
      style={{ borderColor }}
    >
      {/* Default handles when no interfaces defined */}
      {!hasCustomPorts && (
        <>
          <Handle type="target" position={Position.Top} className="!bg-gray-500 !w-2.5 !h-2.5 !border-gray-700" />
          <Handle type="source" position={Position.Bottom} className="!bg-gray-500 !w-2.5 !h-2.5 !border-gray-700" />
        </>
      )}

      {/* In-ports on left side */}
      {inPorts.map((port, i) => (
        <Handle
          key={`in-${i}`}
          type="target"
          position={Position.Left}
          id={`in-${port.name || i}`}
          style={{ top: `${((i + 1) / (inPorts.length + 1)) * 100}%` }}
          className="!bg-green-500 !w-2.5 !h-2.5 !border-green-700"
          title={`${port.name}${port.description ? `: ${port.description}` : ""}`}
        />
      ))}

      {/* Out-ports on right side */}
      {outPorts.map((port, i) => (
        <Handle
          key={`out-${i}`}
          type="source"
          position={Position.Right}
          id={`out-${port.name || i}`}
          style={{ top: `${((i + 1) / (outPorts.length + 1)) * 100}%` }}
          className="!bg-orange-500 !w-2.5 !h-2.5 !border-orange-700"
          title={`${port.name}${port.description ? `: ${port.description}` : ""}`}
        />
      ))}

      <div className="flex flex-col gap-1">
        <div className="text-xs font-semibold text-gray-100 truncate max-w-[160px]">
          {nodeData.label || "Untitled"}
        </div>
        {nodeData.pieceType && (
          <div className="text-[10px] text-gray-500 truncate">
            {nodeData.pieceType}
          </div>
        )}
        <div className="flex items-center gap-1 flex-wrap">
          <div className={`inline-flex rounded px-1.5 py-0.5 text-[10px] font-medium ${phaseColors[nodeData.phase] ?? phaseColors.design}`}>
            {nodeData.phase.charAt(0).toUpperCase() + nodeData.phase.slice(1)}
          </div>
          {running && (
            <div className="inline-flex rounded px-1.5 py-0.5 text-[9px] font-medium bg-purple-700 text-purple-200 animate-pulse">
              ●
            </div>
          )}
          {!running && success === true && !validationFailed && (
            <div className="inline-flex rounded px-1.5 py-0.5 text-[9px] font-medium bg-green-900/40 text-green-300">
              ✓
            </div>
          )}
          {!running && success === false && (
            <div className="inline-flex rounded px-1.5 py-0.5 text-[9px] font-medium bg-red-900/50 text-red-300">
              ✗
            </div>
          )}
          {!running && success === true && validationFailed && (
            <div className="inline-flex rounded px-1.5 py-0.5 text-[9px] font-medium bg-red-900/50 text-red-300">
              !
            </div>
          )}
        </div>

        {/* Team badge, visible only when the piece is tagged with a team. */}
        {nodeData.team && (
          (() => {
            const colors = teamColor(nodeData.team);
            return (
              <div
                className={`inline-flex w-fit rounded border px-1.5 py-0.5 text-[9px] font-medium ${colors.bg} ${colors.text} ${colors.border}`}
                title={`team: ${nodeData.team}`}
              >
                {nodeData.team}
              </div>
            );
          })()
        )}

        {/* Per-role dots, visible only when the piece has extra roles configured. */}
        {nodeData.activeAgents && nodeData.activeAgents.length > 1 && (
          <div className="flex items-center gap-1 text-[9px] text-gray-500">
            {["implementation", "testing", "review"].map((role) => {
              const active = nodeData.activeAgents!.some(
                (r) => r.toLowerCase() === role,
              );
              return (
                <span
                  key={role}
                  className={`inline-flex items-center gap-0.5 rounded px-1 py-0.5 ${
                    active
                      ? "bg-gray-800 text-gray-300"
                      : "bg-gray-900 text-gray-700 opacity-40"
                  }`}
                  title={`${ROLE_LABELS[role] ?? role} ${active ? "enabled" : "disabled"}`}
                >
                  <span
                    className={`inline-block h-1 w-1 rounded-full ${active ? "bg-emerald-400" : "bg-gray-700"}`}
                  />
                  {ROLE_LABELS[role] ?? role}
                </span>
              );
            })}
          </div>
        )}

        {/* Interface port indicators */}
        {hasCustomPorts && (
          <div className="flex gap-1 mt-0.5 flex-wrap">
            {interfaces.map((port, i) => (
              <span
                key={i}
                className={`inline-block w-1.5 h-1.5 rounded-full ${
                  port.direction === "in" ? "bg-green-500" : "bg-orange-500"
                }`}
                title={`${port.direction === "in" ? "←" : "→"} ${port.name}`}
              />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
