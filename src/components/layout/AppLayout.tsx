import { useState } from "react";
import { ReactFlowProvider } from "@xyflow/react";
import { DiagramCanvas } from "../canvas/DiagramCanvas";
import { PieceEditor } from "../editor/PieceEditor";
import { ConnectionEditor } from "../editor/ConnectionEditor";
import { ChatPanel } from "../chat/ChatPanel";
import { LeaderPanel } from "../leader/LeaderPanel";
import { Toolbar } from "./Toolbar";
import { Breadcrumbs } from "./Breadcrumbs";
import { useProjectStore } from "../../store/useProjectStore";

type LeftTab = "chat" | "plan";

export function AppLayout() {
  const { project, selectedPieceId, selectedConnectionId } = useProjectStore();
  const [leftOpen, setLeftOpen] = useState(false);
  const [leftTab, setLeftTab] = useState<LeftTab>("chat");

  const togglePanel = () => setLeftOpen(!leftOpen);

  // When the panel is closed, show a combined toggle button
  if (!leftOpen) {
    return (
      <div className="flex h-full flex-col bg-gray-950 text-gray-100">
        <Toolbar />
        <Breadcrumbs />
        <div className="relative flex flex-1 overflow-hidden">
          {/* Collapsed: show tab-aware toggle */}
          <div className="absolute left-2 top-14 z-10 flex flex-col gap-1">
            <button
              onClick={() => {
                setLeftTab("chat");
                setLeftOpen(true);
              }}
              className="rounded-lg border border-gray-700 bg-gray-900 p-2 text-gray-400 hover:text-gray-200 shadow-lg"
              title="Open CTO Chat"
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
                <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" />
              </svg>
            </button>
            <button
              onClick={() => {
                setLeftTab("plan");
                setLeftOpen(true);
              }}
              className="rounded-lg border border-gray-700 bg-gray-900 p-2 text-gray-400 hover:text-gray-200 shadow-lg"
              title="Open Work Plan"
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
          </div>
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
            <div className="flex items-center gap-2 text-gray-400">
              <svg className="h-5 w-5 animate-spin" viewBox="0 0 24 24" fill="none"><circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" /><path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" /></svg>
              <p>Loading project...</p>
            </div>
          </div>
        )}
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col bg-gray-950 text-gray-100">
      <Toolbar />
      <Breadcrumbs />
      <div className="relative flex flex-1 overflow-hidden">
        {/* Left panel with tab switcher */}
        <div className="flex w-72 shrink-0 flex-col border-r border-gray-800 bg-gray-900">
          {/* Tab bar */}
          <div className="flex border-b border-gray-800">
            <button
              onClick={() => setLeftTab("chat")}
              className={`flex-1 px-3 py-1.5 text-[11px] font-medium transition-colors ${
                leftTab === "chat"
                  ? "text-blue-400 border-b-2 border-blue-400"
                  : "text-gray-500 hover:text-gray-300"
              }`}
            >
              CTO Chat
            </button>
            <button
              onClick={() => setLeftTab("plan")}
              className={`flex-1 px-3 py-1.5 text-[11px] font-medium transition-colors ${
                leftTab === "plan"
                  ? "text-purple-400 border-b-2 border-purple-400"
                  : "text-gray-500 hover:text-gray-300"
              }`}
            >
              Work Plan
            </button>
          </div>

          {/* Panel content — render both but show/hide so state persists */}
          <div className={leftTab === "chat" ? "flex flex-col flex-1 min-h-0" : "hidden"}>
            <ChatPanel open={true} onToggle={togglePanel} embedded />
          </div>
          <div className={leftTab === "plan" ? "flex flex-col flex-1 min-h-0" : "hidden"}>
            <LeaderPanel open={true} onToggle={togglePanel} embedded />
          </div>
        </div>

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
          <div className="flex items-center gap-2 text-gray-400">
            <svg className="h-5 w-5 animate-spin" viewBox="0 0 24 24" fill="none"><circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" /><path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" /></svg>
            <p>Loading project...</p>
          </div>
        </div>
      )}
    </div>
  );
}
