import { Handle, Position, type NodeProps } from "@xyflow/react";
import type { Phase, PieceInterface } from "../../types";

interface PieceNodeData {
  label: string;
  pieceType: string;
  phase: Phase;
  color: string | null;
  interfaces: PieceInterface[];
  [key: string]: unknown;
}

const phaseColors: Record<string, string> = {
  design: "bg-yellow-500/20 text-yellow-400",
  review: "bg-purple-500/20 text-purple-400",
  approved: "bg-green-500/20 text-green-400",
  implementing: "bg-blue-500/20 text-blue-400",
};

export function PieceNode({ data, selected }: NodeProps) {
  const nodeData = data as unknown as PieceNodeData;
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
        <div className={`inline-flex self-start rounded px-1.5 py-0.5 text-[10px] font-medium ${phaseColors[nodeData.phase] ?? phaseColors.design}`}>
          {nodeData.phase.charAt(0).toUpperCase() + nodeData.phase.slice(1)}
        </div>

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
