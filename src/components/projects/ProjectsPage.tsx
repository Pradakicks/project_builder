import { useState, useEffect, useRef } from "react";
import * as api from "../../api/tauriApi";
import type { Project } from "../../types";
import { useAppStore } from "../../store/useAppStore";
import { useProjectStore } from "../../store/useProjectStore";
import { useDialogStore } from "../../store/useDialogStore";
import { useToastStore } from "../../store/useToastStore";

export function ProjectsPage() {
  const [projects, setProjects] = useState<Project[]>([]);
  const [loading, setLoading] = useState(true);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editName, setEditName] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);
  const openProject = useAppStore((s) => s.openProject);
  const goToSettings = useAppStore((s) => s.goToSettings);
  const loadProject = useProjectStore((s) => s.loadProject);
  const showConfirm = useDialogStore((s) => s.showConfirm);
  const addToast = useToastStore((s) => s.addToast);

  const fetchProjects = async () => {
    try {
      const list = await api.listProjects();
      setProjects(list);
    } catch (e) {
      addToast(`Failed to load projects: ${e}`);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchProjects();
  }, []);

  useEffect(() => {
    if (editingId) inputRef.current?.focus();
  }, [editingId]);

  const handleCreate = async () => {
    try {
      const project = await api.createProject("New Project", "");
      setProjects((prev) => [project, ...prev]);
      await loadProject(project.id);
      openProject(project.id);
    } catch (e) {
      addToast(`Failed to create project: ${e}`);
    }
  };

  const handleOpen = async (id: string) => {
    await loadProject(id);
    openProject(id);
  };

  const handleRename = (project: Project) => {
    setEditingId(project.id);
    setEditName(project.name);
  };

  const commitRename = async () => {
    if (!editingId) return;
    const trimmed = editName.trim();
    if (trimmed) {
      try {
        const updated = await api.updateProject(editingId, trimmed);
        setProjects((prev) => prev.map((p) => (p.id === editingId ? updated : p)));
      } catch (e) {
        addToast(`Rename failed: ${e}`);
      }
    }
    setEditingId(null);
  };

  const handleDelete = (project: Project) => {
    showConfirm(`Delete "${project.name}"? This cannot be undone.`, async () => {
      try {
        await api.deleteProject(project.id);
        setProjects((prev) => prev.filter((p) => p.id !== project.id));
        addToast("Project deleted", "info");
      } catch (e) {
        addToast(`Delete failed: ${e}`);
      }
    });
  };

  const formatDate = (iso: string) => {
    try {
      return new Date(iso).toLocaleDateString(undefined, {
        month: "short",
        day: "numeric",
        year: "numeric",
      });
    } catch {
      return iso;
    }
  };

  return (
    <div className="flex h-full flex-col bg-gray-950 text-gray-100">
      {/* Header */}
      <div className="flex items-center justify-between border-b border-gray-800 bg-gray-900 px-6 py-3">
        <h1 className="text-lg font-semibold">Projects</h1>
        <div className="flex gap-2">
          <button
            onClick={goToSettings}
            className="rounded border border-gray-700 px-3 py-1.5 text-xs text-gray-400 hover:bg-gray-800 hover:text-gray-200 transition-colors"
          >
            Settings
          </button>
          <button
            onClick={handleCreate}
            className="rounded bg-blue-600 px-4 py-1.5 text-xs font-medium text-white hover:bg-blue-500 transition-colors"
          >
            + New Project
          </button>
        </div>
      </div>

      {/* Project list */}
      <div className="flex-1 overflow-y-auto p-6">
        {loading ? (
          <div className="flex items-center justify-center gap-2 text-gray-500 mt-20">
            <svg className="h-5 w-5 animate-spin" viewBox="0 0 24 24" fill="none"><circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" /><path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" /></svg>
            <p>Loading...</p>
          </div>
        ) : projects.length === 0 ? (
          <div className="text-center mt-20">
            <p className="text-gray-500 mb-4">No projects yet</p>
            <button
              onClick={handleCreate}
              className="rounded bg-blue-600 px-5 py-2 text-sm font-medium text-white hover:bg-blue-500 transition-colors"
            >
              Create your first project
            </button>
          </div>
        ) : (
          <div className="mx-auto max-w-3xl space-y-2">
            {projects.map((project) => (
              <div
                key={project.id}
                className="group flex items-center gap-4 rounded-lg border border-gray-800 bg-gray-900 px-5 py-3.5 hover:border-gray-700 transition-colors"
              >
                {/* Name / inline edit */}
                <div className="flex-1 min-w-0">
                  {editingId === project.id ? (
                    <input
                      ref={inputRef}
                      value={editName}
                      onChange={(e) => setEditName(e.target.value)}
                      onBlur={commitRename}
                      onKeyDown={(e) => {
                        if (e.key === "Enter") commitRename();
                        if (e.key === "Escape") setEditingId(null);
                      }}
                      className="w-full rounded border border-gray-600 bg-gray-800 px-2 py-0.5 text-sm text-gray-200 focus:border-blue-500 focus:outline-none"
                    />
                  ) : (
                    <button
                      onClick={() => handleOpen(project.id)}
                      className="text-left w-full"
                    >
                      <p className="text-sm font-medium text-gray-200 truncate">
                        {project.name}
                      </p>
                      {project.description && (
                        <p className="text-xs text-gray-500 truncate mt-0.5">
                          {project.description}
                        </p>
                      )}
                    </button>
                  )}
                </div>

                {/* Meta */}
                <span className="text-xs text-gray-600 shrink-0">
                  {formatDate(project.updatedAt)}
                </span>

                {/* Actions */}
                <div className="flex gap-1 shrink-0">
                  <button
                    onClick={() => handleRename(project)}
                    className="rounded px-2 py-1 text-xs text-gray-500 hover:bg-gray-800 hover:text-gray-200"
                    title="Rename"
                  >
                    Rename
                  </button>
                  <button
                    onClick={() => handleDelete(project)}
                    className="rounded px-2 py-1 text-xs text-red-400/60 hover:bg-gray-800 hover:text-red-300"
                    title="Delete"
                  >
                    Delete
                  </button>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
