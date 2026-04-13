import type {
  GoalRun,
  GoalRunDeliverySnapshot,
  GoalRunEvent,
  GoalRunUpdate,
} from "../types";
import { loggedInvoke } from "./runtime";

export async function createGoalRun(
  projectId: string,
  prompt: string,
): Promise<GoalRun> {
  return loggedInvoke("create_goal_run", { projectId, prompt });
}

export async function startGoalRun(
  projectId: string,
  prompt: string,
): Promise<GoalRun> {
  return loggedInvoke("start_goal_run", { projectId, prompt });
}

export async function resumeGoalRun(goalRunId: string): Promise<GoalRun> {
  return loggedInvoke("resume_goal_run", { goalRunId });
}

export async function stopGoalRun(goalRunId: string): Promise<GoalRun> {
  return loggedInvoke("stop_goal_run", { goalRunId });
}

export async function getGoalRun(goalRunId: string): Promise<GoalRun> {
  return loggedInvoke("get_goal_run", { goalRunId });
}

export async function listGoalRuns(projectId: string): Promise<GoalRun[]> {
  return loggedInvoke("list_goal_runs", { projectId });
}

export async function getGoalRunEvents(goalRunId: string): Promise<GoalRunEvent[]> {
  return loggedInvoke("get_goal_run_events", { goalRunId });
}

export async function getGoalRunDeliverySnapshot(
  goalRunId: string,
): Promise<GoalRunDeliverySnapshot> {
  return loggedInvoke("get_goal_run_delivery_snapshot", { goalRunId });
}

export async function updateGoalRun(
  goalRunId: string,
  updates: GoalRunUpdate,
): Promise<GoalRun> {
  const normalized = {
    ...updates,
    blockerReason:
      updates.blockerReason !== undefined ? updates.blockerReason : undefined,
    currentPlanId:
      updates.currentPlanId !== undefined ? updates.currentPlanId : undefined,
    runtimeStatusSummary:
      updates.runtimeStatusSummary !== undefined
        ? updates.runtimeStatusSummary
        : undefined,
    verificationSummary:
      updates.verificationSummary !== undefined
        ? updates.verificationSummary
        : undefined,
    lastFailureSummary:
      updates.lastFailureSummary !== undefined
        ? updates.lastFailureSummary
        : undefined,
    stopRequested:
      updates.stopRequested !== undefined ? updates.stopRequested : undefined,
    currentPieceId:
      updates.currentPieceId !== undefined ? updates.currentPieceId : undefined,
    currentTaskId:
      updates.currentTaskId !== undefined ? updates.currentTaskId : undefined,
    retryBackoffUntil:
      updates.retryBackoffUntil !== undefined
        ? updates.retryBackoffUntil
        : undefined,
    lastFailureFingerprint:
      updates.lastFailureFingerprint !== undefined
        ? updates.lastFailureFingerprint
        : undefined,
    attentionRequired:
      updates.attentionRequired !== undefined
        ? updates.attentionRequired
        : undefined,
  };

  return loggedInvoke("update_goal_run", { goalRunId, updates: normalized });
}
