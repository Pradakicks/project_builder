import { useState, useRef, useEffect } from "react";
import { useProjectStore } from "../../store/useProjectStore";
import { useLeaderStore } from "../../store/useLeaderStore";
import { useAppStore } from "../../store/useAppStore";
import { useGoalRunStore } from "../../store/useGoalRunStore";
import { useToastStore } from "../../store/useToastStore";

export function Toolbar() {
  const { project, addPiece, updateProject, saveToFile, loadFromFile, reset } =
    useProjectStore();
  const goToProjects = useAppStore((s) => s.goToProjects);
  const openProject = useAppStore((s) => s.openProject);
  const goToSettings = useAppStore((s) => s.goToSettings);
  const addToast = useToastStore((s) => s.addToast);
  const currentGoalRun = useGoalRunStore((s) => s.currentGoalRun);
  const deliverySnapshot = useGoalRunStore((s) => s.deliverySnapshot);
  const orchestrating = useGoalRunStore((s) => s.orchestrating);
  const runtimeStatus = useGoalRunStore((s) => s.runtimeStatus);
  const retryGoalRun = useGoalRunStore((s) => s.retryGoalRun);
  const startRuntime = useGoalRunStore((s) => s.startRuntime);
  const stopRuntime = useGoalRunStore((s) => s.stopRuntime);
  const [editing, setEditing] = useState(false);
  const [editName, setEditName] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);
  const autonomyMode = project?.settings.autonomyMode ?? "autopilot";
  const runtimeSpec = project?.settings.runtimeSpec ?? null;
  const runtimeConfigured = Boolean(runtimeSpec?.runCommand?.trim());
  const attentionRequired = Boolean(
    deliverySnapshot?.retryState?.attentionRequired ?? currentGoalRun?.attentionRequired,
  );

  useEffect(() => {
    if (editing) inputRef.current?.focus();
  }, [editing]);

  const handleAddPiece = async () => {
    if (!project) return;
    const x = 200 + Math.random() * 400;
    const y = 100 + Math.random() * 300;
    await addPiece("New Piece", x, y);
  };

  const startEdit = () => {
    setEditName(project?.name ?? "");
    setEditing(true);
  };

  const commitEdit = async () => {
    setEditing(false);
    const trimmed = editName.trim();
    if (trimmed && trimmed !== project?.name) {
      await updateProject(trimmed);
    }
  };

  const handleSave = async () => {
    try {
      const { save } = await import("@tauri-apps/plugin-dialog");
      const path = await save({
        filters: [{ name: "JSON", extensions: ["json"] }],
        defaultPath: `${project?.name ?? "project"}.json`,
      });
      if (path) {
        await saveToFile(path);
        addToast("Project saved", "info");
      }
    } catch (e) {
      addToast(`Save failed: ${e}`);
    }
  };

  const handleLoad = async () => {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const path = await open({
        filters: [{ name: "JSON", extensions: ["json"] }],
        multiple: false,
        directory: false,
      });
      if (path) {
        const importedProject = await loadFromFile(path as string);
        openProject(importedProject.id);
        addToast("Project loaded", "info");
      }
    } catch (e) {
      addToast(`Load failed: ${e}`);
    }
  };

  const handleBackToProjects = () => {
    reset();
    useLeaderStore.getState().reset();
    useGoalRunStore.getState().reset();
    goToProjects();
  };

  return (
    <div className="flex items-center gap-3 border-b border-gray-800 bg-gray-900 px-4 py-2">
      <button
        onClick={handleBackToProjects}
        className="rounded px-2 py-1 text-xs text-gray-400 hover:bg-gray-800 hover:text-gray-200 transition-colors"
        title="Back to projects"
      >
        &larr; Projects
      </button>
      <div className="w-px h-4 bg-gray-700" />
      {project && (
        <span
          className={`rounded px-2 py-0.5 text-[10px] font-medium ${
            autonomyMode === "autopilot"
              ? "bg-emerald-900/60 text-emerald-300"
              : autonomyMode === "guided"
                ? "bg-blue-900/60 text-blue-300"
                : "bg-gray-800 text-gray-300"
          }`}
          title={`Autonomy mode: ${autonomyMode}`}
        >
          {autonomyMode}
        </span>
      )}
      {currentGoalRun && (
        <>
          <span
            className={`rounded px-2 py-0.5 text-[10px] font-medium ${
              currentGoalRun.status === "completed"
                ? "bg-green-900/60 text-green-300"
                : currentGoalRun.status === "running" || currentGoalRun.status === "retrying"
                  ? "bg-blue-900/60 text-blue-300"
                  : "bg-red-900/60 text-red-300"
            }`}
            title={`Goal run phase: ${currentGoalRun.phase}`}
          >
            {currentGoalRun.status} · {currentGoalRun.phase}
          </span>
          {attentionRequired ? (
            <span
              className="rounded px-2 py-0.5 text-[10px] font-medium bg-amber-900/60 text-amber-200"
              title="Goal run needs operator attention"
            >
              attention
            </span>
          ) : null}
          {runtimeStatus?.session?.status === "running" ? (
            <button
              onClick={() => void stopRuntime(project?.id ?? undefined)}
              className="rounded border border-red-700 px-2.5 py-1 text-xs text-red-300 hover:bg-red-950/40"
            >
              Stop App
            </button>
          ) : (
            <button
              onClick={() => void startRuntime(project?.id ?? undefined)}
              disabled={!runtimeConfigured}
              className="rounded border border-green-700 px-2.5 py-1 text-xs text-green-300 hover:bg-green-950/40"
              title={
                runtimeConfigured
                  ? "Start the configured app runtime"
                  : "Configure a runtime contract in Settings first"
              }
            >
              Run App
            </button>
          )}
          {currentGoalRun.status !== "completed" && (
            <button
              onClick={() => void retryGoalRun(currentGoalRun.id)}
              disabled={orchestrating}
              className="rounded border border-amber-700 px-2.5 py-1 text-xs text-amber-300 hover:bg-amber-950/40 disabled:opacity-50"
            >
              {orchestrating ? "Running…" : "Retry Goal"}
            </button>
          )}
        </>
      )}
      {project && (
        <span
          className={`rounded px-2 py-0.5 text-[10px] font-medium ${
            runtimeStatus?.session?.status === "running"
              ? "bg-green-900/60 text-green-300"
              : runtimeConfigured
                ? "bg-gray-800 text-gray-300"
                : "bg-red-900/60 text-red-300"
          }`}
          title={
            runtimeConfigured
              ? runtimeStatus?.session?.status === "running"
                ? "Runtime is running"
                : "Runtime contract configured"
              : "Runtime contract missing"
          }
        >
          {runtimeStatus?.session?.status === "running"
            ? "runtime:running"
            : runtimeConfigured
              ? "runtime:ready"
              : "runtime:missing"}
        </span>
      )}
      {editing ? (
        <input
          ref={inputRef}
          value={editName}
          onChange={(e) => setEditName(e.target.value)}
          onBlur={commitEdit}
          onKeyDown={(e) => {
            if (e.key === "Enter") commitEdit();
            if (e.key === "Escape") setEditing(false);
          }}
          className="rounded border border-gray-600 bg-gray-800 px-2 py-0.5 text-sm font-semibold text-gray-200 focus:border-blue-500 focus:outline-none"
        />
      ) : (
        <h1
          onClick={startEdit}
          className="text-sm font-semibold text-gray-300 cursor-pointer hover:text-gray-100 transition-colors"
          title="Click to rename"
        >
          {project?.name ?? "Project Builder"}
        </h1>
      )}
      <div className="flex-1" />
      <button
        onClick={handleSave}
        className="rounded border border-gray-700 px-2.5 py-1 text-xs text-gray-400 hover:bg-gray-800 hover:text-gray-200 transition-colors"
      >
        Save
      </button>
      <button
        onClick={handleLoad}
        className="rounded border border-gray-700 px-2.5 py-1 text-xs text-gray-400 hover:bg-gray-800 hover:text-gray-200 transition-colors"
      >
        Load
      </button>
      <button
        onClick={goToSettings}
        className="rounded border border-gray-700 px-2.5 py-1 text-xs text-gray-400 hover:bg-gray-800 hover:text-gray-200 transition-colors"
        title="Settings"
      >
        Settings
      </button>
      <button
        onClick={handleAddPiece}
        className="rounded bg-blue-600 px-3 py-1 text-xs font-medium text-white hover:bg-blue-500 transition-colors"
      >
        + Add Piece
      </button>
    </div>
  );
}
