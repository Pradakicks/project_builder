import { useState } from "react";
import { ReactFlowProvider } from "@xyflow/react";
import { DiagramCanvas } from "../canvas/DiagramCanvas";
import { PieceEditor } from "../editor/PieceEditor";
import { ConnectionEditor } from "../editor/ConnectionEditor";
import { ChatPanel } from "../chat/ChatPanel";
import { Toolbar } from "./Toolbar";
import { Breadcrumbs } from "./Breadcrumbs";
import { useProjectStore } from "../../store/useProjectStore";

export function AppLayout() {
  const { project, selectedPieceId, selectedConnectionId } = useProjectStore();
  const [chatOpen, setChatOpen] = useState(false);

  return (
    <div className="flex h-full flex-col bg-gray-950 text-gray-100">
      <Toolbar />
      <Breadcrumbs />
      <div className="relative flex flex-1 overflow-hidden">
        <ChatPanel open={chatOpen} onToggle={() => setChatOpen(!chatOpen)} />
        <div className="flex-1">
          <ReactFlowProvider>
            <DiagramCanvas />
          </ReactFlowProvider>
        </div>
        {selectedPieceId && (
          <div className="w-96 shrink-0 border-l border-gray-800 overflow-y-auto">
            <PieceEditor pieceId={selectedPieceId} />
          </div>
        )}
        {selectedConnectionId && !selectedPieceId && (
          <div className="w-96 shrink-0 border-l border-gray-800 overflow-y-auto">
            <ConnectionEditor connectionId={selectedConnectionId} />
          </div>
        )}
      </div>
      {!project && (
        <div className="absolute inset-0 flex items-center justify-center bg-gray-950/80">
          <p className="text-gray-400">Loading project...</p>
        </div>
      )}
    </div>
  );
}
