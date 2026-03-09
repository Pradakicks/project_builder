import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
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

// ── Settings / Keyring ───────────────────────────────────

export async function getApiKey(provider: string): Promise<string | null> {
  return invoke("get_api_key", { provider });
}

export async function setApiKey(provider: string, key: string): Promise<void> {
  return invoke("set_api_key", { provider, key });
}

export async function deleteApiKey(provider: string): Promise<void> {
  return invoke("delete_api_key", { provider });
}

export async function updateProjectSettings(
  id: string,
  settings: import("../types").ProjectSettings,
): Promise<import("../types").Project> {
  return invoke("update_project_settings", { id, settings });
}

// ── Agent ─────────────────────────────────────────────────

export async function runPieceAgent(pieceId: string): Promise<void> {
  return invoke("run_piece_agent", { pieceId });
}

export interface AgentHistoryEntry {
  id: string;
  agentId: string;
  action: string;
  inputText: string;
  outputText: string;
  tokensUsed: number;
  createdAt: string;
}

export async function getAgentHistory(
  pieceId: string,
): Promise<AgentHistoryEntry[]> {
  return invoke("get_agent_history", { pieceId });
}

export interface LlmMessage {
  role: string;
  content: string;
}

export async function chatWithCto(
  projectId: string,
  userMessage: string,
  conversation: LlmMessage[],
): Promise<void> {
  return invoke("chat_with_cto", { projectId, userMessage, conversation });
}

// ── Event Listeners ───────────────────────────────────────

export interface AgentOutputChunk {
  pieceId: string;
  chunk: string;
  done: boolean;
  usage?: { input: number; output: number };
}

export function onAgentOutputChunk(
  callback: (payload: AgentOutputChunk) => void,
): Promise<UnlistenFn> {
  return listen<AgentOutputChunk>("agent-output-chunk", (event) => {
    callback(event.payload);
  });
}

export interface CtoChatChunk {
  chunk: string;
  done: boolean;
  usage?: { input: number; output: number };
}

export function onCtoChatChunk(
  callback: (payload: CtoChatChunk) => void,
): Promise<UnlistenFn> {
  return listen<CtoChatChunk>("cto-chat-chunk", (event) => {
    callback(event.payload);
  });
}
