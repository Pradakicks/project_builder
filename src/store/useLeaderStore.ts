import { create } from "zustand";
import type { WorkPlan, PlanTask, TaskStatus, Phase, MergeProgressEvent, MergeSummary, ConflictInfo } from "../types";
import {
  generateWorkPlan,
  listWorkPlans,
  updatePlanStatus,
  updatePlanTaskStatus,
  runPieceAgent,
  onAgentOutputChunk,
  mergePlanBranches,
  resolveMergeConflict,
  runIntegrationReview,
  onMergeProgress,
  onIntegrationReviewChunk,
} from "../api/tauriApi";
import { useAgentStore } from "./useAgentStore";
import { useProjectStore } from "./useProjectStore";
import { useToastStore } from "./useToastStore";
import { devLog } from "../utils/devLog";

let runAllCancelled = false;

interface LeaderStore {
  currentPlan: WorkPlan | null;
  plans: WorkPlan[];
  generating: boolean;
  streamOutput: string;
  runningAll: boolean;
  runAllProgress: string;

  // Merge state
  merging: boolean;
  mergeProgress: MergeProgressEvent[];
  mergeSummary: MergeSummary | null;
  conflictInfo: ConflictInfo | null;
  resolvingConflict: boolean;

  // Integration review state
  reviewStreaming: boolean;
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
  currentPlan: null,
  plans: [],
  generating: false,
  streamOutput: "",
  runningAll: false,
  runAllProgress: "",
  merging: false,
  mergeProgress: [],
  mergeSummary: null,
  conflictInfo: null,
  resolvingConflict: false,
  reviewStreaming: false,
  reviewOutput: "",

  generatePlan: async (projectId, guidance) => {
    devLog("info", "Store:Leader", `Generating plan for project ${projectId}`, { guidance: guidance.slice(0, 100) });
    set({ generating: true, streamOutput: "", currentPlan: null });
    try {
      const plan = await generateWorkPlan(projectId, guidance);
      set({ currentPlan: plan, generating: false });
      devLog("info", "Store:Leader", `Plan generated: ${plan.tasks.length} tasks`, { planId: plan.id });
    } catch (e) {
      set({ generating: false });
      devLog("error", "Store:Leader", `Plan generation failed`, e);
      useToastStore.getState().addToast(`Leader agent error: ${e}`);
    }
  },

  loadPlans: async (projectId) => {
    try {
      const plans = await listWorkPlans(projectId);
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
      const plan = await updatePlanStatus(planId, "approved");
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
      const plan = await updatePlanStatus(planId, "rejected");
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
      const plan = await updatePlanTaskStatus(planId, taskId, status);
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

      // Update task status to in-progress
      try {
        const plan = await updatePlanTaskStatus(planId, task.id, "in-progress");
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
      const unlisten = await onAgentOutputChunk((payload) => {
        if (payload.pieceId !== task.pieceId) return;
        const store = useAgentStore.getState();
        if (payload.done) {
          store.completeRun(task.pieceId, {
            usage: payload.usage ?? { input: 0, output: 0 },
            exitCode: payload.exitCode,
            phaseProposal: payload.phaseProposal,
            phaseChanged: payload.phaseChanged,
            gitBranch: payload.gitBranch,
            gitCommitSha: payload.gitCommitSha,
            gitDiffStat: payload.gitDiffStat,
          });
          unlisten();
          // Mark task as complete
          updatePlanTaskStatus(planId, task.id, "complete")
            .then((plan) => {
              set({ currentPlan: plan });
              const plans = get().plans.map((p) => (p.id === planId ? plan : p));
              set({ plans });
            })
            .catch((e: unknown) => devLog("error", "Store:Leader", `Failed to mark task complete`, e));
          devLog("info", "Store:Leader", `Task "${task.title}" completed successfully`);
          resolve(true);
        } else {
          store.appendChunk(task.pieceId, payload.chunk);
        }
      });

      try {
        await runPieceAgent(task.pieceId);
      } catch (e) {
        useToastStore.getState().addToast(`Agent error: ${e}`);
        agentStore.completeRun(task.pieceId, { usage: { input: 0, output: 0 } });
        unlisten();
        // Revert task status
        updatePlanTaskStatus(planId, task.id, "pending")
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
      useToastStore.getState().addToast("No pending tasks to run");
      return;
    }

    devLog("info", "Store:Leader", `Running all ${tasks.length} pending tasks`, { planId });
    set({ runningAll: true, runAllProgress: `0/${tasks.length}` });

    let completed = 0;
    for (const task of tasks) {
      if (runAllCancelled) {
        useToastStore.getState().addToast("Run All cancelled");
        break;
      }
      set({ runAllProgress: `${completed + 1}/${tasks.length}` });
      const success = await get().runTask(planId, task);
      if (!success) {
        useToastStore.getState().addToast(`Run All stopped: task "${task.title}" failed`);
        break;
      }
      completed++;
    }

    set({ runningAll: false, runAllProgress: "" });

    devLog("info", "Store:Leader", `Run All complete: ${completed}/${tasks.length} tasks`);

    // Auto-trigger merge if all tasks completed successfully
    if (completed === tasks.length && !runAllCancelled) {
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
      mergeProgress: [],
      mergeSummary: null,
      conflictInfo: null,
      reviewStreaming: false,
      reviewOutput: "",
    });

    const unlisten = await onMergeProgress((payload) => {
      if (payload.planId !== planId) return;
      set((state) => ({
        mergeProgress: [...state.mergeProgress.filter(
          (p) => p.branch !== payload.branch
        ), payload],
      }));
    });

    try {
      const summary = await mergePlanBranches(planId);
      set({ mergeSummary: summary, conflictInfo: summary.conflict });
      unlisten();

      if (!summary.conflict) {
        useToastStore.getState().addToast(
          `Merged ${summary.merged.length} branch${summary.merged.length !== 1 ? "es" : ""} to main`,
          "info",
        );
        // Auto-trigger integration review
        get().runReview(planId);
      } else {
        useToastStore.getState().addToast(
          `Merge conflict in ${summary.conflict.pieceName}`,
          "warning",
        );
      }
    } catch (e) {
      devLog("error", "Store:Leader", `Merge failed`, e);
      useToastStore.getState().addToast(`Merge failed: ${e}`);
      unlisten();
    } finally {
      set({ merging: false });
    }
  },

  resolveConflict: async (planId, pieceId) => {
    devLog("info", "Store:Leader", `Resolving conflict for piece ${pieceId}`);
    set({ resolvingConflict: true });
    try {
      await resolveMergeConflict(planId, pieceId);
      set({ conflictInfo: null, resolvingConflict: false });
      useToastStore.getState().addToast("Conflict resolved — resuming merge", "info");
      // Resume merging remaining branches
      get().mergeBranches(planId);
    } catch (e) {
      set({ resolvingConflict: false });
      devLog("error", "Store:Leader", `Conflict resolution failed`, e);
      useToastStore.getState().addToast(`Conflict resolution failed: ${e}`);
    }
  },

  runReview: async (planId) => {
    devLog("info", "Store:Leader", `Starting integration review`, { planId });
    set({ reviewStreaming: true, reviewOutput: "" });

    const unlisten = await onIntegrationReviewChunk((payload) => {
      if (payload.planId !== planId) return;
      if (payload.done) {
        set({ reviewStreaming: false });
        unlisten();
      } else {
        set((state) => ({ reviewOutput: state.reviewOutput + payload.chunk }));
      }
    });

    try {
      await runIntegrationReview(planId);
    } catch (e) {
      set({ reviewStreaming: false });
      unlisten();
      devLog("error", "Store:Leader", `Integration review failed`, e);
      useToastStore.getState().addToast(`Integration review error: ${e}`);
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
      currentPlan: null,
      plans: [],
      generating: false,
      streamOutput: "",
      runningAll: false,
      runAllProgress: "",
      merging: false,
      mergeProgress: [],
      mergeSummary: null,
      conflictInfo: null,
      resolvingConflict: false,
      reviewStreaming: false,
      reviewOutput: "",
    });
  },
}));
