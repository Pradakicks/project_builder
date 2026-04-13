import { Suspense, lazy, useEffect, useState } from "react";
import { ReactFlowProvider } from "@xyflow/react";
import { Toolbar } from "./Toolbar";
import { Breadcrumbs } from "./Breadcrumbs";
import { useProjectStore } from "../../store/useProjectStore";
import { useToastStore } from "../../store/useToastStore";
import { useGoalRunStore } from "../../store/useGoalRunStore";
import { onPhaseWarning } from "../../api/projectApi";

type LeftTab = "chat" | "plan" | "agents";

const DiagramCanvas = lazy(() =>
  import("../canvas/DiagramCanvas").then((module) => ({
    default: module.DiagramCanvas,
  })),
);
const PieceEditor = lazy(() =>
  import("../editor/PieceEditor").then((module) => ({
    default: module.PieceEditor,
  })),
);
const ConnectionEditor = lazy(() =>
  import("../editor/ConnectionEditor").then((module) => ({
    default: module.ConnectionEditor,
  })),
);
const ChatPanel = lazy(() =>
  import("../chat/ChatPanel").then((module) => ({
    default: module.ChatPanel,
  })),
);
const LeaderPanel = lazy(() =>
  import("../leader/LeaderPanel").then((module) => ({
    default: module.LeaderPanel,
  })),
);
const AgentsPanel = lazy(() =>
  import("../agents/AgentsPanel").then((module) => ({
    default: module.AgentsPanel,
  })),
);

function LoadingPane({ label }: { label: string }) {
  return (
    <div className="flex h-full items-center justify-center text-xs text-gray-500">
      <div className="flex items-center gap-2">
        <svg
          className="h-4 w-4 animate-spin"
          viewBox="0 0 24 24"
          fill="none"
        >
          <circle
            className="opacity-25"
            cx="12"
            cy="12"
            r="10"
            stroke="currentColor"
            strokeWidth="4"
          />
          <path
            className="opacity-75"
            fill="currentColor"
            d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"
          />
        </svg>
        <span>{label}</span>
      </div>
    </div>
  );
}

export function AppLayout() {
  const { project, selectedPieceId, selectedConnectionId } = useProjectStore();
  const [leftOpen, setLeftOpen] = useState(false);
  const [leftTab, setLeftTab] = useState<LeftTab>("chat");
  const addToast = useToastStore((s) => s.addToast);

  const togglePanel = () => setLeftOpen(!leftOpen);

  useEffect(() => {
    const unlisten = onPhaseWarning((payload) => {
      addToast(payload.warning, "warning");
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [addToast]);

  useEffect(() => {
    if (project?.id) {
      void useGoalRunStore.getState().loadGoalRuns(project.id);
    } else {
      useGoalRunStore.getState().reset();
    }
  }, [project?.id]);

  const renderCanvas = () => (
    <Suspense fallback={<LoadingPane label="Loading canvas..." />}>
      <ReactFlowProvider>
        <DiagramCanvas />
      </ReactFlowProvider>
    </Suspense>
  );

  const renderPieceEditor = () => (
    <Suspense fallback={<LoadingPane label="Loading piece editor..." />}>
      <PieceEditor pieceId={selectedPieceId ?? ""} />
    </Suspense>
  );

  const renderConnectionEditor = () => (
    <Suspense fallback={<LoadingPane label="Loading connection editor..." />}>
      <ConnectionEditor connectionId={selectedConnectionId ?? ""} />
    </Suspense>
  );

  const renderChatPanel = () => (
    <Suspense fallback={<LoadingPane label="Loading CTO chat..." />}>
      <ChatPanel
        open={true}
        onToggle={togglePanel}
        embedded
        onSwitchTab={(tab) => setLeftTab(tab as LeftTab)}
      />
    </Suspense>
  );

  const renderLeaderPanel = () => (
    <Suspense fallback={<LoadingPane label="Loading work plan..." />}>
      <LeaderPanel open={true} onToggle={togglePanel} embedded />
    </Suspense>
  );

  const renderAgentsPanel = () => (
    <Suspense fallback={<LoadingPane label="Loading agents..." />}>
      <AgentsPanel />
    </Suspense>
  );

  if (!leftOpen) {
    return (
      <div className="flex h-full flex-col bg-gray-950 text-gray-100">
        <Toolbar />
        <Breadcrumbs />
        <div className="relative flex flex-1 overflow-hidden">
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
            <button
              onClick={() => {
                setLeftTab("agents");
                setLeftOpen(true);
              }}
              className="rounded-lg border border-gray-700 bg-gray-900 p-2 text-gray-400 hover:text-gray-200 shadow-lg"
              title="Open Agents"
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
                <circle cx="12" cy="12" r="3" />
                <path d="M12 2v3M12 19v3M4.22 4.22l2.12 2.12M17.66 17.66l2.12 2.12M2 12h3M19 12h3M4.22 19.78l2.12-2.12M17.66 6.34l2.12-2.12" />
              </svg>
            </button>
          </div>
          <div className="flex-1">{renderCanvas()}</div>
          {selectedPieceId && (
            <div className="w-96 shrink-0 overflow-y-auto border-l border-gray-800">
              {renderPieceEditor()}
            </div>
          )}
          {selectedConnectionId && !selectedPieceId && (
            <div className="w-96 shrink-0 overflow-y-auto border-l border-gray-800">
              {renderConnectionEditor()}
            </div>
          )}
        </div>
        {!project && (
          <div className="absolute inset-0 flex items-center justify-center bg-gray-950/80">
            <div className="flex items-center gap-2 text-gray-400">
              <svg
                className="h-5 w-5 animate-spin"
                viewBox="0 0 24 24"
                fill="none"
              >
                <circle
                  className="opacity-25"
                  cx="12"
                  cy="12"
                  r="10"
                  stroke="currentColor"
                  strokeWidth="4"
                />
                <path
                  className="opacity-75"
                  fill="currentColor"
                  d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"
                />
              </svg>
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
        <div className="flex w-72 shrink-0 flex-col border-r border-gray-800 bg-gray-900">
          <div className="flex border-b border-gray-800">
            <button
              onClick={() => setLeftTab("chat")}
              className={`flex-1 px-3 py-1.5 text-[11px] font-medium transition-colors ${
                leftTab === "chat"
                  ? "border-b-2 border-blue-400 text-blue-400"
                  : "text-gray-500 hover:text-gray-300"
              }`}
            >
              CTO Chat
            </button>
            <button
              onClick={() => setLeftTab("plan")}
              className={`flex-1 px-3 py-1.5 text-[11px] font-medium transition-colors ${
                leftTab === "plan"
                  ? "border-b-2 border-purple-400 text-purple-400"
                  : "text-gray-500 hover:text-gray-300"
              }`}
            >
              Work Plan
            </button>
            <button
              onClick={() => setLeftTab("agents")}
              className={`flex-1 px-3 py-1.5 text-[11px] font-medium transition-colors ${
                leftTab === "agents"
                  ? "border-b-2 border-emerald-400 text-emerald-400"
                  : "text-gray-500 hover:text-gray-300"
              }`}
            >
              Agents
            </button>
          </div>

          <div className={leftTab === "chat" ? "flex flex-1 min-h-0 flex-col" : "hidden"}>
            {renderChatPanel()}
          </div>
          <div className={leftTab === "plan" ? "flex flex-1 min-h-0 flex-col" : "hidden"}>
            {renderLeaderPanel()}
          </div>
          <div className={leftTab === "agents" ? "flex flex-1 min-h-0 flex-col" : "hidden"}>
            {renderAgentsPanel()}
          </div>
        </div>

        <div className="flex-1">{renderCanvas()}</div>
        {selectedPieceId && (
          <div className="w-96 shrink-0 overflow-y-auto border-l border-gray-800">
            {renderPieceEditor()}
          </div>
        )}
        {selectedConnectionId && !selectedPieceId && (
          <div className="w-96 shrink-0 overflow-y-auto border-l border-gray-800">
            {renderConnectionEditor()}
          </div>
        )}
      </div>
      {!project && (
        <div className="absolute inset-0 flex items-center justify-center bg-gray-950/80">
          <div className="flex items-center gap-2 text-gray-400">
            <svg
              className="h-5 w-5 animate-spin"
              viewBox="0 0 24 24"
              fill="none"
            >
              <circle
                className="opacity-25"
                cx="12"
                cy="12"
                r="10"
                stroke="currentColor"
                strokeWidth="4"
              />
              <path
                className="opacity-75"
                fill="currentColor"
                d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"
              />
            </svg>
            <p>Loading project...</p>
          </div>
        </div>
      )}
    </div>
  );
}
