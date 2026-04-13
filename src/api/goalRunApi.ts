import type { GoalRun, GoalRunUpdate } from "../types";
import { loggedInvoke } from "./runtime";

export async function createGoalRun(
  projectId: string,
  prompt: string,
): Promise<GoalRun> {
  return loggedInvoke("create_goal_run", { projectId, prompt });
}

export async function getGoalRun(goalRunId: string): Promise<GoalRun> {
  return loggedInvoke("get_goal_run", { goalRunId });
}

export async function listGoalRuns(projectId: string): Promise<GoalRun[]> {
  return loggedInvoke("list_goal_runs", { projectId });
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
  };

  return loggedInvoke("update_goal_run", { goalRunId, updates: normalized });
}
