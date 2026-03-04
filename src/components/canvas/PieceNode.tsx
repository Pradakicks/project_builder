import { Handle, Position, type NodeProps } from "@xyflow/react";
import type { Phase } from "../../types";

interface PieceNodeData {
  label: string;
  pieceType: string;
  phase: Phase;
  color: string | null;
  [key: string]: unknown;
}

const phaseColors: Record<Phase, string> = {
  design: "bg-yellow-500/20 text-yellow-400",
  review: "bg-purple-500/20 text-purple-400",
  approved: "bg-green-500/20 text-green-400",
  implementing: "bg-blue-500/20 text-blue-400",
  done: "bg-gray-500/20 text-gray-400",
};

export function PieceNode({ data, selected }: NodeProps) {
  const nodeData = data as unknown as PieceNodeData;
  const borderColor = nodeData.color ?? "#3b82f6";

  return (
    <div
      className={`min-w-[140px] rounded-lg border-2 bg-gray-900 px-3 py-2 shadow-lg transition-shadow ${
        selected ? "shadow-blue-500/30 ring-1 ring-blue-500" : ""
      }`}
      style={{ borderColor }}
    >
      <Handle type="target" position={Position.Top} className="!bg-gray-500 !w-2.5 !h-2.5 !border-gray-700" />

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
          {nodeData.phase}
        </div>
      </div>

      <Handle type="source" position={Position.Bottom} className="!bg-gray-500 !w-2.5 !h-2.5 !border-gray-700" />
    </div>
  );
}
