import { create } from "zustand";
import type {
  Project,
  Piece,
  Connection,
  PieceUpdate,
  ConnectionUpdate,
} from "../types";
import * as api from "../api/projectApi";
import { useToastStore } from "./useToastStore";
import { devLog } from "../utils/devLog";

const toast = (msg: string) => useToastStore.getState().addToast(msg);

interface ProjectStore {
  // State
  project: Project | null;
  pieces: Piece[];
  connections: Connection[];
  selectedPieceId: string | null;
  selectedConnectionId: string | null;
  currentParentId: string | null;
  breadcrumbs: { id: string; name: string }[];

  // Project actions
  loadProject: (id: string) => Promise<void>;
  createProject: (
    name: string,
    description: string,
    parentDirectory?: string | null,
  ) => Promise<Project>;
  updateProject: (name?: string, description?: string) => Promise<void>;

  // Piece actions
  addPiece: (name: string, positionX: number, positionY: number) => Promise<Piece>;
  updatePiece: (id: string, updates: PieceUpdate) => Promise<void>;
  deletePiece: (id: string) => Promise<void>;
  selectPiece: (id: string | null) => void;

  // Connection actions
  addConnection: (sourcePieceId: string, targetPieceId: string, label: string) => Promise<Connection>;
  updateConnection: (id: string, updates: ConnectionUpdate) => Promise<void>;
  deleteConnection: (id: string) => Promise<void>;
  selectConnection: (id: string | null) => void;

  // Navigation
  drillInto: (pieceId: string) => Promise<void>;
  navigateTo: (index: number) => Promise<void>;

  // File I/O
  saveToFile: (path: string) => Promise<void>;
  loadFromFile: (path: string) => Promise<Project>;
  reset: () => void;
}

const emptyProjectState = {
  project: null,
  pieces: [],
  connections: [],
  selectedPieceId: null,
  selectedConnectionId: null,
  currentParentId: null,
  breadcrumbs: [],
};

let projectLoadRequestId = 0;

export const useProjectStore = create<ProjectStore>((set, get) => ({
  ...emptyProjectState,

  loadProject: async (id: string) => {
    devLog("info", "Store:Project", `Loading project ${id}`);
    const requestId = ++projectLoadRequestId;
    set({
      ...emptyProjectState,
      project: get().project?.id === id ? get().project : null,
    });
    try {
      const project = await api.getProject(id);
      const pieces = await api.listPieces(id);
      const connections = await api.listConnections(id);
      if (requestId !== projectLoadRequestId) {
        devLog("debug", "Store:Project", `Discarding stale load for project ${id}`);
        return;
      }
      const topLevelPieces = pieces.filter((p) => p.parentId === null);
      set({
        project,
        pieces: topLevelPieces,
        connections,
        currentParentId: null,
        breadcrumbs: [{ id: project.id, name: project.name }],
      });
      devLog("info", "Store:Project", `Loaded project "${project.name}" — ${pieces.length} pieces, ${connections.length} connections`);
    } catch (e) {
      if (requestId !== projectLoadRequestId) return;
      devLog("error", "Store:Project", `Failed to load project ${id}`, e);
      toast(`Failed to load project: ${e}`);
    }
  },

  createProject: async (
    name: string,
    description: string,
    parentDirectory?: string | null,
  ) => {
    devLog("info", "Store:Project", `Creating project "${name}"`);
    try {
      const project = await api.createProject(
        name,
        description,
        parentDirectory,
      );
      set({
        ...emptyProjectState,
        project,
        breadcrumbs: [{ id: project.id, name: project.name }],
      });
      devLog("info", "Store:Project", `Created project "${name}" (${project.id})`);
      return project;
    } catch (e) {
      devLog("error", "Store:Project", `Failed to create project`, e);
      toast(`Failed to create project: ${e}`);
      throw e;
    }
  },

  updateProject: async (name?: string, description?: string) => {
    const { project } = get();
    if (!project) return;
    try {
      const updated = await api.updateProject(project.id, name, description);
      set({ project: updated });
    } catch (e) {
      toast(`Failed to update project: ${e}`);
    }
  },

  addPiece: async (name: string, positionX: number, positionY: number) => {
    const { project, currentParentId, pieces } = get();
    if (!project) throw new Error("No project loaded");
    devLog("debug", "Store:Project", `Creating piece "${name}"`, { positionX, positionY, parentId: currentParentId });
    try {
      const piece = await api.createPiece(project.id, currentParentId, name, positionX, positionY);
      set({ pieces: [...pieces, piece] });
      devLog("info", "Store:Project", `Created piece "${name}" (${piece.id})`);
      return piece;
    } catch (e) {
      devLog("error", "Store:Project", `Failed to create piece`, e);
      toast(`Failed to add piece: ${e}`);
      throw e;
    }
  },

  updatePiece: async (id: string, updates: PieceUpdate) => {
    devLog("debug", "Store:Project", `Updating piece ${id}`, { fields: Object.keys(updates) });
    try {
      const updated = await api.updatePiece(id, updates);
      set({
        pieces: get().pieces.map((p) => (p.id === id ? updated : p)),
      });
    } catch (e) {
      devLog("error", "Store:Project", `Failed to update piece ${id}`, e);
      toast(`Failed to update piece: ${e}`);
    }
  },

  deletePiece: async (id: string) => {
    devLog("info", "Store:Project", `Deleting piece ${id}`);
    try {
      await api.deletePiece(id);
      const { pieces, connections, selectedPieceId } = get();
      set({
        pieces: pieces.filter((p) => p.id !== id),
        connections: connections.filter(
          (c) => c.sourcePieceId !== id && c.targetPieceId !== id,
        ),
        selectedPieceId: selectedPieceId === id ? null : selectedPieceId,
      });
    } catch (e) {
      devLog("error", "Store:Project", `Failed to delete piece ${id}`, e);
      toast(`Failed to delete piece: ${e}`);
    }
  },

  selectPiece: (id: string | null) => {
    set({ selectedPieceId: id, selectedConnectionId: null });
  },

  addConnection: async (sourcePieceId: string, targetPieceId: string, label: string) => {
    const { project, connections } = get();
    if (!project) throw new Error("No project loaded");
    try {
      const connection = await api.createConnection(project.id, sourcePieceId, targetPieceId, label);
      set({ connections: [...connections, connection] });
      return connection;
    } catch (e) {
      toast(`Failed to add connection: ${e}`);
      throw e;
    }
  },

  updateConnection: async (id: string, updates: ConnectionUpdate) => {
    try {
      const updated = await api.updateConnection(id, updates);
      set({
        connections: get().connections.map((c) => (c.id === id ? updated : c)),
      });
    } catch (e) {
      toast(`Failed to update connection: ${e}`);
    }
  },

  deleteConnection: async (id: string) => {
    try {
      await api.deleteConnection(id);
      const { connections, selectedConnectionId } = get();
      set({
        connections: connections.filter((c) => c.id !== id),
        selectedConnectionId: selectedConnectionId === id ? null : selectedConnectionId,
      });
    } catch (e) {
      toast(`Failed to delete connection: ${e}`);
    }
  },

  selectConnection: (id: string | null) => {
    set({ selectedConnectionId: id, selectedPieceId: null });
  },

  drillInto: async (pieceId: string) => {
    try {
      const { project, breadcrumbs } = get();
      if (!project) return;
      const piece = get().pieces.find((p) => p.id === pieceId);
      if (!piece) return;
      const children = await api.listChildren(pieceId);
      const allConnections = await api.listConnections(project.id);
      const childIds = new Set(children.map((c) => c.id));
      const childConnections = allConnections.filter(
        (c) => childIds.has(c.sourcePieceId) && childIds.has(c.targetPieceId),
      );
      set({
        pieces: children,
        connections: childConnections,
        currentParentId: pieceId,
        breadcrumbs: [...breadcrumbs, { id: pieceId, name: piece.name }],
        selectedPieceId: null,
        selectedConnectionId: null,
      });
    } catch (e) {
      toast(`Failed to navigate: ${e}`);
    }
  },

  navigateTo: async (index: number) => {
    try {
      const { project, breadcrumbs } = get();
      if (!project) return;
      const newBreadcrumbs = breadcrumbs.slice(0, index + 1);
      const target = newBreadcrumbs[newBreadcrumbs.length - 1];

      if (index === 0) {
        const allPieces = await api.listPieces(project.id);
        const topLevel = allPieces.filter((p) => p.parentId === null);
        const allConnections = await api.listConnections(project.id);
        const topIds = new Set(topLevel.map((p) => p.id));
        const topConnections = allConnections.filter(
          (c) => topIds.has(c.sourcePieceId) && topIds.has(c.targetPieceId),
        );
        set({
          pieces: topLevel,
          connections: topConnections,
          currentParentId: null,
          breadcrumbs: newBreadcrumbs,
          selectedPieceId: null,
          selectedConnectionId: null,
        });
      } else {
        const children = await api.listChildren(target.id);
        const allConnections = await api.listConnections(project.id);
        const childIds = new Set(children.map((c) => c.id));
        const childConnections = allConnections.filter(
          (c) => childIds.has(c.sourcePieceId) && childIds.has(c.targetPieceId),
        );
        set({
          pieces: children,
          connections: childConnections,
          currentParentId: target.id,
          breadcrumbs: newBreadcrumbs,
          selectedPieceId: null,
          selectedConnectionId: null,
        });
      }
    } catch (e) {
      toast(`Failed to navigate: ${e}`);
    }
  },

  saveToFile: async (path: string) => {
    const { project } = get();
    if (!project) return;
    try {
      await api.saveProjectToFile(project.id, path);
    } catch (e) {
      toast(`Failed to save: ${e}`);
    }
  },

  loadFromFile: async (path: string) => {
    const requestId = ++projectLoadRequestId;
    set(emptyProjectState);
    try {
      const project = await api.loadProjectFromFile(path);
      const pieces = await api.listPieces(project.id);
      const connections = await api.listConnections(project.id);
      if (requestId !== projectLoadRequestId) {
        devLog("debug", "Store:Project", `Discarding stale imported project ${project.id}`);
        return project;
      }
      const topLevelPieces = pieces.filter((p) => p.parentId === null);
      set({
        project,
        pieces: topLevelPieces,
        connections,
        currentParentId: null,
        breadcrumbs: [{ id: project.id, name: project.name }],
      });
      return project;
    } catch (e) {
      toast(`Failed to load file: ${e}`);
      throw e;
    }
  },

  reset: () => {
    projectLoadRequestId++;
    devLog("debug", "Store:Project", "Resetting active project state");
    set(emptyProjectState);
  },
}));
