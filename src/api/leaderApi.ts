import type {
  Artifact,
  IntegrationReviewChunk,
  MergeProgressEvent,
  MergeSummary,
  PlanStatus,
  TaskStatus,
  WorkPlan,
} from "../types";
import { loggedInvoke, listenToEvent } from "./runtime";

export interface AgentHistoryEntry {
  id: string;
  agentId: string;
  action: string;
  inputText: string;
  outputText: string;
  metadata: import("../types").AgentHistoryMetadata;
  tokensUsed: number;
  createdAt: string;
}

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
  validation?: import("../types").ValidationResult;
  error?: string;
}

export interface GitStatusInfo {
  currentBranch: string;
  hasUncommittedChanges: boolean;
  lastCommitMessage: string | null;
  lastCommitSha: string | null;
}

export interface LeaderPlanChunk {
  projectId: string;
  planId: string;
  chunk: string;
  done: boolean;
}

export async function runPieceAgent(
  pieceId: string,
  feedback?: string,
): Promise<void> {
  return loggedInvoke("run_piece_agent", { pieceId, feedback: feedback ?? null });
}

export async function getAgentHistory(
  pieceId: string,
): Promise<AgentHistoryEntry[]> {
  return loggedInvoke("get_agent_history", { pieceId });
}

export function onAgentOutputChunk(
  callback: (payload: AgentOutputChunk) => void,
): Promise<import("@tauri-apps/api/event").UnlistenFn> {
  return listenToEvent<AgentOutputChunk>("agent-output-chunk", callback);
}

export async function getGitStatus(
  pieceId: string,
): Promise<GitStatusInfo | null> {
  return loggedInvoke("get_git_status", { pieceId });
}

export async function listArtifacts(pieceId: string): Promise<Artifact[]> {
  return loggedInvoke("list_artifacts", { pieceId });
}

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

export async function runAllPlanTasks(planId: string): Promise<void> {
  return loggedInvoke("run_all_plan_tasks", { planId });
}

export function onLeaderPlanChunk(
  callback: (payload: LeaderPlanChunk) => void,
): Promise<import("@tauri-apps/api/event").UnlistenFn> {
  return listenToEvent<LeaderPlanChunk>("leader-plan-chunk", callback);
}

export async function mergePlanBranches(planId: string): Promise<MergeSummary> {
  return loggedInvoke("merge_plan_branches", { planId });
}

export async function resolveMergeConflict(
  planId: string,
  pieceId: string,
): Promise<void> {
  return loggedInvoke("resolve_merge_conflict", { planId, pieceId });
}

export async function runIntegrationReview(planId: string): Promise<void> {
  return loggedInvoke("run_integration_review", { planId });
}

export function onMergeProgress(
  callback: (payload: MergeProgressEvent) => void,
): Promise<import("@tauri-apps/api/event").UnlistenFn> {
  return listenToEvent<MergeProgressEvent>("merge-progress", callback);
}

export function onIntegrationReviewChunk(
  callback: (payload: IntegrationReviewChunk) => void,
): Promise<import("@tauri-apps/api/event").UnlistenFn> {
  return listenToEvent<IntegrationReviewChunk>("integration-review-chunk", callback);
}
