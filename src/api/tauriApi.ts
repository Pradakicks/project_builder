import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { devLog } from "../utils/devLog";
import type {
  Project,
  Piece,
  Connection,
  PieceUpdate,
  ConnectionUpdate,
  Artifact,
  CtoDecision,
  WorkPlan,
  PlanStatus,
  TaskStatus,
  AgentHistoryMetadata,
  ValidationResult,
} from "../types";

async function loggedInvoke<T>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T> {
  devLog("debug", "IPC", `→ ${cmd}`, args);
  const start = performance.now();
  try {
    const result = await invoke<T>(cmd, args);
    devLog(
      "debug",
      "IPC",
      `← ${cmd} (${(performance.now() - start).toFixed(0)}ms)`,
    );
    return result;
  } catch (e) {
    devLog(
      "error",
      "IPC",
      `✗ ${cmd} (${(performance.now() - start).toFixed(0)}ms)`,
      e,
    );
    throw e;
  }
}

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
): Promise<Piece> {
  return loggedInvoke("create_piece", {
    projectId,
    parentId,
    name,
    positionX,
    positionY,
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
  settings: import("../types").ProjectSettings,
): Promise<import("../types").Project> {
  return loggedInvoke("update_project_settings", { id, settings });
}

export async function validateWorkingDirectory(
  path: string,
): Promise<boolean> {
  return loggedInvoke("validate_working_directory", { path });
}

// ── Agent ─────────────────────────────────────────────────

export async function runPieceAgent(pieceId: string, feedback?: string): Promise<void> {
  return loggedInvoke("run_piece_agent", { pieceId, feedback: feedback ?? null });
}

export interface AgentHistoryEntry {
  id: string;
  agentId: string;
  action: string;
  inputText: string;
  outputText: string;
  metadata: AgentHistoryMetadata;
  tokensUsed: number;
  createdAt: string;
}

export async function getAgentHistory(
  pieceId: string,
): Promise<AgentHistoryEntry[]> {
  return loggedInvoke("get_agent_history", { pieceId });
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
  return loggedInvoke("chat_with_cto", { projectId, userMessage, conversation });
}

// ── Event Listeners ───────────────────────────────────────

export interface AgentOutputChunk {
  pieceId: string;
  chunk: string;
  done: boolean;
  streamKind?: "agent" | "validation";
  success?: boolean;
  exitCode?: number;
  usage?: { input: number; output: number };
  phaseProposal?: string;
  phaseChanged?: string;
  gitBranch?: string;
  gitCommitSha?: string;
  gitDiffStat?: string;
  validation?: ValidationResult;
  error?: string;
}

export function onAgentOutputChunk(
  callback: (payload: AgentOutputChunk) => void,
): Promise<UnlistenFn> {
  return listen<AgentOutputChunk>("agent-output-chunk", (event) => {
    callback(event.payload);
  });
}

export interface PhaseWarning {
  pieceId: string;
  warning: string;
}

export function onPhaseWarning(
  callback: (payload: PhaseWarning) => void,
): Promise<UnlistenFn> {
  return listen<PhaseWarning>("phase-warning", (event) => {
    callback(event.payload);
  });
}

// ── Git Status ───────────────────────────────────────────

export interface GitStatusInfo {
  currentBranch: string;
  hasUncommittedChanges: boolean;
  lastCommitMessage: string | null;
  lastCommitSha: string | null;
}

export async function getGitStatus(
  pieceId: string,
): Promise<GitStatusInfo | null> {
  return loggedInvoke("get_git_status", { pieceId });
}

// ── Artifacts ────────────────────────────────────────────

export async function listArtifacts(pieceId: string): Promise<Artifact[]> {
  return loggedInvoke("list_artifacts", { pieceId });
}

// ── CTO Decisions ───────────────────────────────────────

export async function logCtoDecision(
  projectId: string,
  summary: string,
  actionsJson: string,
): Promise<CtoDecision> {
  return loggedInvoke("log_cto_decision", { projectId, summary: summary, actionsJson });
}

export async function listCtoDecisions(
  projectId: string,
): Promise<CtoDecision[]> {
  return loggedInvoke("list_cto_decisions", { projectId });
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

// ── Work Plans ───────────────────────────────────────────

export async function generateWorkPlan(
  projectId: string,
  userGuidance: string,
): Promise<WorkPlan> {
  return loggedInvoke("generate_work_plan", { projectId, userGuidance });
}

export async function getWorkPlan(planId: string): Promise<WorkPlan> {
  return loggedInvoke("get_work_plan", { planId });
}

export async function listWorkPlans(projectId: string): Promise<WorkPlan[]> {
  return loggedInvoke("list_work_plans", { projectId });
}

export async function updatePlanStatus(
  planId: string,
  status: PlanStatus,
): Promise<WorkPlan> {
  return loggedInvoke("update_plan_status", { planId, status });
}

export async function updatePlanTaskStatus(
  planId: string,
  taskId: string,
  status: TaskStatus,
): Promise<WorkPlan> {
  return loggedInvoke("update_plan_task_status", { planId, taskId, status });
}

export interface LeaderPlanChunk {
  planId: string;
  chunk: string;
  done: boolean;
}

export function onLeaderPlanChunk(
  callback: (payload: LeaderPlanChunk) => void,
): Promise<UnlistenFn> {
  return listen<LeaderPlanChunk>("leader-plan-chunk", (event) => {
    callback(event.payload);
  });
}

// ── Branch Merging ──────────────────────────────────────

import type {
  MergeSummary,
  MergeProgressEvent,
  IntegrationReviewChunk,
} from "../types";

export async function mergePlanBranches(planId: string): Promise<MergeSummary> {
  return loggedInvoke("merge_plan_branches", { planId });
}

export async function resolveMergeConflict(planId: string, pieceId: string): Promise<void> {
  return loggedInvoke("resolve_merge_conflict", { planId, pieceId });
}

export async function runIntegrationReview(planId: string): Promise<void> {
  return loggedInvoke("run_integration_review", { planId });
}

export function onMergeProgress(
  callback: (payload: MergeProgressEvent) => void,
): Promise<UnlistenFn> {
  return listen<MergeProgressEvent>("merge-progress", (event) => {
    callback(event.payload);
  });
}

export function onIntegrationReviewChunk(
  callback: (payload: IntegrationReviewChunk) => void,
): Promise<UnlistenFn> {
  return listen<IntegrationReviewChunk>("integration-review-chunk", (event) => {
    callback(event.payload);
  });
}
