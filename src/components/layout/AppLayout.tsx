import { useEffect, useState } from "react";
import { ReactFlowProvider } from "@xyflow/react";
import { DiagramCanvas } from "../canvas/DiagramCanvas";
import { PieceEditor } from "../editor/PieceEditor";
import { ConnectionEditor } from "../editor/ConnectionEditor";
import { ChatPanel } from "../chat/ChatPanel";
import { Toolbar } from "./Toolbar";
import { Breadcrumbs } from "./Breadcrumbs";
import { useProjectStore } from "../../store/useProjectStore";

export function AppLayout() {
  const { project, selectedPieceId, selectedConnectionId, createProject, loadProject } =
    useProjectStore();
  const [initialized, setInitialized] = useState(false);
  const [chatOpen, setChatOpen] = useState(false);

  useEffect(() => {
    if (initialized) return;
    setInitialized(true);

    (async () => {
      try {
        const { listProjects } = await import("../../api/tauriApi");
        const projects = await listProjects();
        if (projects.length > 0) {
          await loadProject(projects[0].id);
        } else {
          await createProject("My Project", "A new project");
        }
      } catch {
        console.log("Running outside Tauri, no backend available");
      }
    })();
  }, [initialized, createProject, loadProject]);

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
