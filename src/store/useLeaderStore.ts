import { create } from "zustand";
import type { WorkPlan, PlanTask, TaskStatus, Phase, MergeProgressEvent, MergeSummary, ConflictInfo } from "../types";
import { useAgentStore } from "./useAgentStore";
import { useProjectStore } from "./useProjectStore";
import { useToastStore } from "./useToastStore";
import { devLog } from "../utils/devLog";

let runAllCancelled = false;

async function loadLeaderApi() {
  return import("../api/leaderApi");
}

type RunAllStatus = "idle" | "running" | "complete" | "failed" | "cancelled";
type MergeStatus = "idle" | "merging" | "complete" | "conflict" | "failed";
type ReviewStatus = "idle" | "running" | "complete" | "failed";

interface LeaderStore {
  projectId: string | null;
  currentPlan: WorkPlan | null;
  plans: WorkPlan[];
  generating: boolean;
  streamOutput: string;
  runningAll: boolean;
  runAllProgress: string;
  runAllStatus: RunAllStatus;
  runAllError: string | null;

  // Merge state
  merging: boolean;
  mergeStatus: MergeStatus;
  mergeError: string | null;
  mergeProgress: MergeProgressEvent[];
  mergeSummary: MergeSummary | null;
  conflictInfo: ConflictInfo | null;
  resolvingConflict: boolean;

  // Integration review state
  reviewStreaming: boolean;
  reviewStatus: ReviewStatus;
  reviewError: string | null;
  reviewOutput: string;

  generatePlan: (projectId: string, guidance: string) => Promise<void>;
  loadPlans: (projectId: string) => Promise<void>;
  approvePlan: (planId: string) => Promise<void>;
  rejectPlan: (planId: string) => Promise<void>;
  updateTaskStatus: (
    planId: string,
    taskId: string,
    status: TaskStatus,
  ) => Promise<void>;
  runTask: (planId: string, task: PlanTask) => Promise<boolean>;
  runAllTasks: (planId: string) => Promise<void>;
  cancelRunAll: () => void;
  mergeBranches: (planId: string) => Promise<void>;
  resolveConflict: (planId: string, pieceId: string) => Promise<void>;
  runReview: (planId: string) => Promise<void>;
  appendChunk: (chunk: string) => void;
  completeGeneration: (plan: WorkPlan) => void;
  reset: () => void;
}

export const useLeaderStore = create<LeaderStore>((set, get) => ({
  projectId: null,
  currentPlan: null,
  plans: [],
  generating: false,
  streamOutput: "",
  runningAll: false,
  runAllProgress: "",
  runAllStatus: "idle",
  runAllError: null,
  merging: false,
  mergeStatus: "idle",
  mergeError: null,
  mergeProgress: [],
  mergeSummary: null,
  conflictInfo: null,
  resolvingConflict: false,
  reviewStreaming: false,
  reviewStatus: "idle",
  reviewError: null,
  reviewOutput: "",

  generatePlan: async (projectId, guidance) => {
    devLog("info", "Store:Leader", `Generating plan for project ${projectId}`, { guidance: guidance.slice(0, 100) });
    set({ projectId, generating: true, streamOutput: "", currentPlan: null });
    try {
      const api = await loadLeaderApi();
      const plan = await api.generateWorkPlan(projectId, guidance);
      if (get().projectId !== projectId) {
        devLog("debug", "Store:Leader", `Discarding stale generated plan for project ${projectId}`);
        return;
      }
      set({ currentPlan: plan, generating: false });
      devLog("info", "Store:Leader", `Plan generated: ${plan.tasks.length} tasks`, { planId: plan.id });
    } catch (e) {
      if (get().projectId !== projectId) return;
      set({ generating: false });
      devLog("error", "Store:Leader", `Plan generation failed`, e);
      useToastStore.getState().addToast(`Leader agent error: ${e}`);
    }
  },

  loadPlans: async (projectId) => {
    set({ projectId });
    try {
      const api = await loadLeaderApi();
      const plans = await api.listWorkPlans(projectId);
      if (get().projectId !== projectId) {
        devLog("debug", "Store:Leader", `Discarding stale plans for project ${projectId}`);
        return;
      }
      set({
        plans,
        currentPlan: plans.length > 0 ? plans[0] : null,
      });
    } catch (e) {
      useToastStore.getState().addToast(`Failed to load plans: ${e}`);
    }
  },

  approvePlan: async (planId) => {
    devLog("info", "Store:Leader", `Approving plan ${planId}`);
    try {
      const api = await loadLeaderApi();
      const plan = await api.updatePlanStatus(planId, "approved");
      set({ currentPlan: plan });
      const plans = get().plans.map((p) => (p.id === planId ? plan : p));
      set({ plans });
    } catch (e) {
      devLog("error", "Store:Leader", `Failed to approve plan`, e);
      useToastStore.getState().addToast(`Failed to approve plan: ${e}`);
    }
  },

  rejectPlan: async (planId) => {
    devLog("info", "Store:Leader", `Rejecting plan ${planId}`);
    try {
      const api = await loadLeaderApi();
      const plan = await api.updatePlanStatus(planId, "rejected");
      set({ currentPlan: plan });
      const plans = get().plans.map((p) => (p.id === planId ? plan : p));
      set({ plans });
    } catch (e) {
      devLog("error", "Store:Leader", `Failed to reject plan`, e);
      useToastStore.getState().addToast(`Failed to reject plan: ${e}`);
    }
  },

  updateTaskStatus: async (planId, taskId, status) => {
    try {
      const api = await loadLeaderApi();
      const plan = await api.updatePlanTaskStatus(planId, taskId, status);
      set({ currentPlan: plan });
      const plans = get().plans.map((p) => (p.id === planId ? plan : p));
      set({ plans });
    } catch (e) {
      useToastStore.getState().addToast(`Failed to update task: ${e}`);
    }
  },

  runTask: async (planId, task): Promise<boolean> => {
    return new Promise(async (resolve) => {
      devLog("info", "Store:Leader", `Running task "${task.title}" for piece ${task.pieceId}`, { planId, taskId: task.id });
      const agentStore = useAgentStore.getState();
      agentStore.startRun(task.pieceId);
      const api = await loadLeaderApi();

      // Update task status to in-progress
      try {
        const plan = await api.updatePlanTaskStatus(planId, task.id, "in-progress");
        set({ currentPlan: plan });
        const plans = get().plans.map((p) => (p.id === planId ? plan : p));
        set({ plans });
      } catch (e) {
        useToastStore.getState().addToast(`Failed to start task: ${e}`);
        agentStore.completeRun(task.pieceId, { usage: { input: 0, output: 0 } });
        resolve(false);
        return;
      }

      // Apply suggested phase before running (so phase-aware prompt matches leader's intent)
      if (task.suggestedPhase && task.pieceId) {
        const projStore = useProjectStore.getState();
        const piece = projStore.pieces.find((p) => p.id === task.pieceId);
        if (piece && piece.phase !== task.suggestedPhase) {
          try {
            await projStore.updatePiece(task.pieceId, { phase: task.suggestedPhase as Phase });
          } catch {
            // Non-fatal: continue with current phase
          }
        }
      }

      // Set up chunk listener before calling run
      const unlisten = await api.onAgentOutputChunk((payload) => {
        if (payload.pieceId !== task.pieceId) return;
        const store = useAgentStore.getState();
        if (payload.done) {
          const success = payload.success ?? (payload.exitCode ?? 0) === 0;
          store.completeRun(task.pieceId, {
            usage: payload.usage ?? { input: 0, output: 0 },
            success,
            exitCode: payload.exitCode,
            phaseProposal: payload.phaseProposal,
            phaseChanged: payload.phaseChanged,
            gitBranch: payload.gitBranch,
            gitCommitSha: payload.gitCommitSha,
            gitDiffStat: payload.gitDiffStat,
            validation: payload.validation,
          });
          unlisten();
          if (success) {
            // Mark task as complete
            api.updatePlanTaskStatus(planId, task.id, "complete")
              .then((plan) => {
                set({ currentPlan: plan });
                const plans = get().plans.map((p) => (p.id === planId ? plan : p));
                set({ plans });
              })
              .catch((e: unknown) => devLog("error", "Store:Leader", `Failed to mark task complete`, e));
            devLog("info", "Store:Leader", `Task "${task.title}" completed successfully`);
            resolve(true);
          } else {
            api.updatePlanTaskStatus(planId, task.id, "pending")
              .then((plan) => {
                set({ currentPlan: plan });
                const plans = get().plans.map((p) => (p.id === planId ? plan : p));
                set({ plans });
              })
              .catch((e: unknown) => devLog("error", "Store:Leader", `Failed to revert task status`, e));
            resolve(false);
          }
        } else {
          if (payload.streamKind === "validation") {
            store.appendValidationChunk(task.pieceId, payload.chunk);
          } else {
            store.appendChunk(task.pieceId, payload.chunk);
          }
        }
      });

      try {
        await api.runPieceAgent(task.pieceId);
      } catch (e) {
        useToastStore.getState().addToast(`Agent error: ${e}`);
        agentStore.completeRun(task.pieceId, { usage: { input: 0, output: 0 } });
        unlisten();
        // Revert task status
        api.updatePlanTaskStatus(planId, task.id, "pending")
          .then((plan) => {
            set({ currentPlan: plan });
            const plans = get().plans.map((p) => (p.id === planId ? plan : p));
            set({ plans });
          })
          .catch((revertErr: unknown) => devLog("error", "Store:Leader", `Failed to revert task status`, revertErr));
        devLog("error", "Store:Leader", `Task "${task.title}" failed`, e);
        resolve(false);
      }
    });
  },

  runAllTasks: async (planId) => {
    const plan = get().currentPlan;
    if (!plan) return;
    runAllCancelled = false;

    const tasks = [...plan.tasks]
      .filter((t) => t.status === "pending" && t.pieceId)
      .sort((a, b) => a.order - b.order);

    if (tasks.length === 0) {
      set({ runAllStatus: "idle", runAllError: null });
      useToastStore.getState().addToast("No pending tasks to run");
      return;
    }

    devLog("info", "Store:Leader", `Running all ${tasks.length} pending tasks`, { planId });
    set({
      runningAll: true,
      runAllProgress: `0/${tasks.length}`,
      runAllStatus: "running",
      runAllError: null,
    });

    let completed = 0;
    let failedMessage: string | null = null;
    let cancelled = false;
    for (const task of tasks) {
      if (runAllCancelled) {
        cancelled = true;
        useToastStore.getState().addToast("Run All cancelled");
        break;
      }
      set({ runAllProgress: `${completed + 1}/${tasks.length}` });
      const success = await get().runTask(planId, task);
      if (!success) {
        failedMessage = `Run All stopped: task "${task.title}" failed`;
        set({
          runAllStatus: "failed",
          runAllError: failedMessage,
        });
        useToastStore.getState().addToast(failedMessage);
        break;
      }
      completed++;
    }

    set({
      runningAll: false,
      runAllProgress: "",
      runAllStatus: cancelled
        ? "cancelled"
        : failedMessage
          ? "failed"
          : "complete",
      runAllError: failedMessage,
    });

    devLog("info", "Store:Leader", `Run All complete: ${completed}/${tasks.length} tasks`);

    // Auto-trigger merge if all tasks completed successfully
    if (completed === tasks.length && !runAllCancelled && !failedMessage) {
      get().mergeBranches(planId);
    }
  },

  cancelRunAll: () => {
    runAllCancelled = true;
  },

  mergeBranches: async (planId) => {
    devLog("info", "Store:Leader", `Starting branch merge`, { planId });
    set({
      merging: true,
      mergeStatus: "merging",
      mergeError: null,
      mergeProgress: [],
      mergeSummary: null,
      conflictInfo: null,
      reviewStreaming: false,
      reviewStatus: "idle",
      reviewError: null,
      reviewOutput: "",
    });

    const api = await loadLeaderApi();
    const unlisten = await api.onMergeProgress((payload) => {
      if (payload.planId !== planId) return;
      set((state) => ({
        mergeProgress: [...state.mergeProgress.filter(
          (p) => p.branch !== payload.branch
        ), payload],
      }));
    });

    try {
      const summary = await api.mergePlanBranches(planId);
      set({ mergeSummary: summary, conflictInfo: summary.conflict });
      unlisten();

      if (!summary.conflict) {
        set({ mergeStatus: "complete", mergeError: null });
        useToastStore.getState().addToast(
          `Merged ${summary.merged.length} branch${summary.merged.length !== 1 ? "es" : ""} to main`,
          "info",
        );
        // Auto-trigger integration review
        get().runReview(planId);
      } else {
        set({ mergeStatus: "conflict", mergeError: null });
        useToastStore.getState().addToast(
          `Merge conflict in ${summary.conflict.pieceName}`,
          "warning",
        );
      }
    } catch (e) {
      devLog("error", "Store:Leader", `Merge failed`, e);
      const message = `Merge failed: ${e}`;
      set({ mergeStatus: "failed", mergeError: message });
      useToastStore.getState().addToast(message);
      unlisten();
    } finally {
      set({ merging: false });
    }
  },

  resolveConflict: async (planId, pieceId) => {
    devLog("info", "Store:Leader", `Resolving conflict for piece ${pieceId}`);
    set({ resolvingConflict: true, mergeStatus: "merging", mergeError: null });
    try {
      const api = await loadLeaderApi();
      await api.resolveMergeConflict(planId, pieceId);
      set({ conflictInfo: null, resolvingConflict: false });
      useToastStore.getState().addToast("Conflict resolved — resuming merge", "info");
      // Resume merging remaining branches
      get().mergeBranches(planId);
    } catch (e) {
      set({ resolvingConflict: false });
      devLog("error", "Store:Leader", `Conflict resolution failed`, e);
      const message = `Conflict resolution failed: ${e}`;
      set({ mergeStatus: "failed", mergeError: message });
      useToastStore.getState().addToast(message);
    }
  },

  runReview: async (planId) => {
    devLog("info", "Store:Leader", `Starting integration review`, { planId });
    set({
      reviewStreaming: true,
      reviewStatus: "running",
      reviewError: null,
      reviewOutput: "",
    });

    const api = await loadLeaderApi();
    const unlisten = await api.onIntegrationReviewChunk((payload) => {
      if (payload.planId !== planId) return;
      if (payload.done) {
        set({ reviewStreaming: false, reviewStatus: "complete", reviewError: null });
        unlisten();
      } else {
        set((state) => ({ reviewOutput: state.reviewOutput + payload.chunk }));
      }
    });

    try {
      await api.runIntegrationReview(planId);
    } catch (e) {
      const message = `Integration review error: ${e}`;
      set({ reviewStreaming: false, reviewStatus: "failed", reviewError: message });
      unlisten();
      devLog("error", "Store:Leader", `Integration review failed`, e);
      useToastStore.getState().addToast(message);
    }
  },

  appendChunk: (chunk) => {
    set({ streamOutput: get().streamOutput + chunk });
  },

  completeGeneration: (plan) => {
    set({ currentPlan: plan, generating: false });
  },

  reset: () => {
    set({
      projectId: null,
      currentPlan: null,
      plans: [],
      generating: false,
      streamOutput: "",
      runningAll: false,
      runAllProgress: "",
      runAllStatus: "idle",
      runAllError: null,
      merging: false,
      mergeStatus: "idle",
      mergeError: null,
      mergeProgress: [],
      mergeSummary: null,
      conflictInfo: null,
      resolvingConflict: false,
      reviewStreaming: false,
      reviewStatus: "idle",
      reviewError: null,
      reviewOutput: "",
    });
  },
}));
