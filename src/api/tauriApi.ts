import { invoke } from "@tauri-apps/api/core";
import type {
  Project,
  Piece,
  Connection,
  PieceUpdate,
  ConnectionUpdate,
} from "../types";

// ── Projects ──────────────────────────────────────────────

export async function createProject(
  name: string,
  description: string,
): Promise<Project> {
  return invoke("create_project", { name, description });
}

export async function getProject(id: string): Promise<Project> {
  return invoke("get_project", { id });
}

export async function updateProject(
  id: string,
  name?: string,
  description?: string,
): Promise<Project> {
  return invoke("update_project", { id, name, description });
}

export async function listProjects(): Promise<Project[]> {
  return invoke("list_projects");
}

export async function deleteProject(id: string): Promise<void> {
  return invoke("delete_project", { id });
}

export async function saveProjectToFile(
  id: string,
  path: string,
): Promise<void> {
  return invoke("save_project_to_file", { id, path });
}

export async function loadProjectFromFile(path: string): Promise<Project> {
  return invoke("load_project_from_file", { path });
}

// ── Pieces ────────────────────────────────────────────────

export async function createPiece(
  projectId: string,
  parentId: string | null,
  name: string,
  positionX: number,
  positionY: number,
): Promise<Piece> {
  return invoke("create_piece", {
    projectId,
    parentId,
    name,
    positionX,
    positionY,
  });
}

export async function getPiece(id: string): Promise<Piece> {
  return invoke("get_piece", { id });
}

export async function updatePiece(
  id: string,
  updates: PieceUpdate,
): Promise<Piece> {
  return invoke("update_piece", { id, updates });
}

export async function deletePiece(id: string): Promise<void> {
  return invoke("delete_piece", { id });
}

export async function listPieces(projectId: string): Promise<Piece[]> {
  return invoke("list_pieces", { projectId });
}

export async function listChildren(pieceId: string): Promise<Piece[]> {
  return invoke("list_children", { pieceId });
}

// ── Connections ───────────────────────────────────────────

export async function createConnection(
  projectId: string,
  sourcePieceId: string,
  targetPieceId: string,
  label: string,
): Promise<Connection> {
  return invoke("create_connection", {
    projectId,
    sourcePieceId,
    targetPieceId,
    label,
  });
}

export async function getConnection(id: string): Promise<Connection> {
  return invoke("get_connection", { id });
}

export async function updateConnection(
  id: string,
  updates: ConnectionUpdate,
): Promise<Connection> {
  return invoke("update_connection", { id, updates });
}

export async function deleteConnection(id: string): Promise<void> {
  return invoke("delete_connection", { id });
}

export async function listConnections(projectId: string): Promise<Connection[]> {
  return invoke("list_connections", { projectId });
}
