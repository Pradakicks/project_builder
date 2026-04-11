import { useState, useEffect, useRef } from "react";
import * as api from "../../api/tauriApi";
import type { Project } from "../../types";
import { useAppStore } from "../../store/useAppStore";
import { useProjectStore } from "../../store/useProjectStore";
import { useLeaderStore } from "../../store/useLeaderStore";
import { useDialogStore } from "../../store/useDialogStore";
import { useToastStore } from "../../store/useToastStore";

function slugifyProjectName(name: string): string {
  return name
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

export function ProjectsPage() {
  const [projects, setProjects] = useState<Project[]>([]);
  const [loading, setLoading] = useState(true);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editName, setEditName] = useState("");
  const [createOpen, setCreateOpen] = useState(false);
  const [createName, setCreateName] = useState("New Project");
  const [createDescription, setCreateDescription] = useState("");
  const [parentDirectory, setParentDirectory] = useState("");
  const [creating, setCreating] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const openProject = useAppStore((s) => s.openProject);
  const goToSettings = useAppStore((s) => s.goToSettings);
  const loadProject = useProjectStore((s) => s.loadProject);
  const resetProject = useProjectStore((s) => s.reset);
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
    resetProject();
    useLeaderStore.getState().reset();
    fetchProjects();
  }, [resetProject]);

  useEffect(() => {
    if (editingId) inputRef.current?.focus();
  }, [editingId]);

  const resetCreateForm = () => {
    setCreateName("New Project");
    setCreateDescription("");
    setParentDirectory("");
    setCreating(false);
  };

  const openCreateModal = () => {
    resetCreateForm();
    setCreateOpen(true);
  };

  const closeCreateModal = () => {
    if (creating) return;
    setCreateOpen(false);
  };

  const handleCreate = async () => {
    const trimmedName = createName.trim();
    const trimmedDescription = createDescription.trim();
    const trimmedParent = parentDirectory.trim();
    if (!trimmedName || !trimmedParent) return;

    setCreating(true);
    try {
      const project = await api.createProject(
        trimmedName,
        trimmedDescription,
        trimmedParent,
      );
      setProjects((prev) => [project, ...prev]);
      setCreateOpen(false);
      resetCreateForm();
      await loadProject(project.id);
      openProject(project.id);
    } catch (e) {
      addToast(`Failed to create project: ${e}`);
      setCreating(false);
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

  const folderName = slugifyProjectName(createName);
  const previewPath =
    parentDirectory.trim() && folderName
      ? `${parentDirectory.replace(/\/+$/, "")}/${folderName}`
      : "";

  return (
    <div className="relative flex h-full flex-col bg-gray-950 text-gray-100">
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
            onClick={openCreateModal}
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
              onClick={openCreateModal}
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

      {createOpen && (
        <div className="absolute inset-0 z-20 flex items-center justify-center bg-black/60 px-4">
          <div className="w-full max-w-xl rounded-xl border border-gray-800 bg-gray-900 shadow-2xl">
            <div className="border-b border-gray-800 px-5 py-4">
              <h2 className="text-base font-semibold text-gray-100">
                Create Project
              </h2>
              <p className="mt-1 text-xs text-gray-500">
                A new folder and git repository will be created for this
                project. You can change the working directory later in
                Settings.
              </p>
            </div>

            <div className="space-y-4 px-5 py-4">
              <div>
                <label className="mb-1 block text-sm text-gray-400">
                  Project Name
                </label>
                <input
                  value={createName}
                  onChange={(e) => setCreateName(e.target.value)}
                  placeholder="Simple Web App"
                  className="w-full rounded border border-gray-700 bg-gray-800 px-3 py-2 text-sm text-gray-100 focus:border-blue-500 focus:outline-none"
                />
              </div>

              <div>
                <label className="mb-1 block text-sm text-gray-400">
                  Description
                </label>
                <textarea
                  value={createDescription}
                  onChange={(e) => setCreateDescription(e.target.value)}
                  rows={3}
                  placeholder="Optional project description"
                  className="w-full resize-none rounded border border-gray-700 bg-gray-800 px-3 py-2 text-sm text-gray-100 focus:border-blue-500 focus:outline-none"
                />
              </div>

              <div>
                <label className="mb-1 block text-sm text-gray-400">
                  Parent Folder
                </label>
                <div className="flex gap-2">
                  <input
                    value={parentDirectory}
                    onChange={(e) => setParentDirectory(e.target.value)}
                    placeholder="/Users/adrianth/Projects"
                    className="flex-1 rounded border border-gray-700 bg-gray-800 px-3 py-2 font-mono text-sm text-gray-100 focus:border-blue-500 focus:outline-none"
                  />
                  <button
                    onClick={async () => {
                      try {
                        const { open } = await import("@tauri-apps/plugin-dialog");
                        const selected = await open({
                          directory: true,
                          multiple: false,
                        });
                        if (selected && typeof selected === "string") {
                          setParentDirectory(selected);
                        }
                      } catch (e) {
                        addToast(`Browse failed: ${e}`);
                      }
                    }}
                    className="rounded border border-gray-700 px-3 py-2 text-xs text-gray-300 hover:bg-gray-800"
                  >
                    Browse
                  </button>
                </div>
              </div>

              <div className="rounded border border-gray-800 bg-gray-950/70 px-3 py-2">
                <p className="text-[10px] uppercase tracking-wide text-gray-500">
                  Working Directory Preview
                </p>
                <p className="mt-1 font-mono text-xs text-gray-300">
                  {previewPath || "Choose a parent folder and project name"}
                </p>
              </div>
            </div>

            <div className="flex justify-end gap-2 border-t border-gray-800 px-5 py-4">
              <button
                onClick={closeCreateModal}
                disabled={creating}
                className="rounded border border-gray-700 px-4 py-2 text-xs text-gray-300 hover:bg-gray-800 disabled:opacity-50"
              >
                Cancel
              </button>
              <button
                onClick={handleCreate}
                disabled={creating || !createName.trim() || !parentDirectory.trim() || !folderName}
                className="rounded bg-blue-600 px-4 py-2 text-xs font-medium text-white hover:bg-blue-500 disabled:opacity-50"
              >
                {creating ? "Creating..." : "Create Project"}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
