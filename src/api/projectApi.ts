import type {
  Connection,
  ConnectionUpdate,
  Piece,
  PieceUpdate,
  Project,
  ProjectSettings,
  TeamBrief,
} from "../types";
import { loggedInvoke, listenToEvent } from "./runtime";

// ── Projects ──────────────────────────────────────────────

export async function createProject(
  name: string,
  description: string,
  parentDirectory?: string | null,
): Promise<Project> {
  return loggedInvoke("create_project", {
    name,
    description,
    parentDirectory: parentDirectory ?? null,
  });
}

export async function getProject(id: string): Promise<Project> {
  return loggedInvoke("get_project", { id });
}

export async function updateProject(
  id: string,
  name?: string,
  description?: string,
): Promise<Project> {
  return loggedInvoke("update_project", { id, name, description });
}

export async function listProjects(): Promise<Project[]> {
  return loggedInvoke("list_projects");
}

export async function deleteProject(id: string): Promise<void> {
  return loggedInvoke("delete_project", { id });
}

export async function saveProjectToFile(
  id: string,
  path: string,
): Promise<void> {
  return loggedInvoke("save_project_to_file", { id, path });
}

export async function loadProjectFromFile(path: string): Promise<Project> {
  return loggedInvoke("load_project_from_file", { path });
}

// ── Pieces ────────────────────────────────────────────────

export async function createPiece(
  projectId: string,
  parentId: string | null,
  name: string,
  positionX: number,
  positionY: number,
  updates?: PieceUpdate | null,
): Promise<Piece> {
  return loggedInvoke("create_piece", {
    projectId,
    parentId,
    name,
    positionX,
    positionY,
    updates: updates ?? null,
  });
}

export async function getPiece(id: string): Promise<Piece> {
  return loggedInvoke("get_piece", { id });
}

export async function updatePiece(
  id: string,
  updates: PieceUpdate,
): Promise<Piece> {
  return loggedInvoke("update_piece", { id, updates });
}

export async function deletePiece(id: string): Promise<void> {
  return loggedInvoke("delete_piece", { id });
}

export async function listPieces(projectId: string): Promise<Piece[]> {
  return loggedInvoke("list_pieces", { projectId });
}

export async function listChildren(pieceId: string): Promise<Piece[]> {
  return loggedInvoke("list_children", { pieceId });
}

/// Distinct team tags across a project's pieces. Powers the PieceEditor
/// team datalist and the ProjectStatusBar teams chip.
export async function listTeamsForProject(projectId: string): Promise<string[]> {
  return loggedInvoke("list_teams_for_project", { projectId });
}

/// Every team brief for the project, newest first. Used by the debug report
/// and the ProjectStatusBar popover.
export async function listTeamBriefs(projectId: string): Promise<TeamBrief[]> {
  return loggedInvoke("list_team_briefs", { projectId });
}

// ── Connections ───────────────────────────────────────────

export async function createConnection(
  projectId: string,
  sourcePieceId: string,
  targetPieceId: string,
  label: string,
): Promise<Connection> {
  return loggedInvoke("create_connection", {
    projectId,
    sourcePieceId,
    targetPieceId,
    label,
  });
}

export async function getConnection(id: string): Promise<Connection> {
  return loggedInvoke("get_connection", { id });
}

export async function updateConnection(
  id: string,
  updates: ConnectionUpdate,
): Promise<Connection> {
  return loggedInvoke("update_connection", { id, updates });
}

export async function deleteConnection(id: string): Promise<void> {
  return loggedInvoke("delete_connection", { id });
}

export async function listConnections(projectId: string): Promise<Connection[]> {
  return loggedInvoke("list_connections", { projectId });
}

// ── Settings / Keyring ───────────────────────────────────

export async function getApiKey(provider: string): Promise<string | null> {
  return loggedInvoke("get_api_key", { provider });
}

export async function setApiKey(provider: string, key: string): Promise<void> {
  return loggedInvoke("set_api_key", { provider, key });
}

export async function deleteApiKey(provider: string): Promise<void> {
  return loggedInvoke("delete_api_key", { provider });
}

export async function updateProjectSettings(
  id: string,
  settings: ProjectSettings,
): Promise<Project> {
  return loggedInvoke("update_project_settings", { id, settings });
}

export async function validateWorkingDirectory(
  path: string,
): Promise<boolean> {
  return loggedInvoke("validate_working_directory", { path });
}

// ── Misc Events ──────────────────────────────────────────

export interface PhaseWarning {
  pieceId: string;
  warning: string;
}

export function onPhaseWarning(
  callback: (payload: PhaseWarning) => void,
): Promise<import("@tauri-apps/api/event").UnlistenFn> {
  return listenToEvent<PhaseWarning>("phase-warning", callback);
}
