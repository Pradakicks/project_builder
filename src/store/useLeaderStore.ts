import { create } from "zustand";
import type { WorkPlan, PlanTask, TaskStatus } from "../types";
import {
  generateWorkPlan,
  listWorkPlans,
  updatePlanStatus,
  updatePlanTaskStatus,
  runPieceAgent,
  onAgentOutputChunk,
} from "../api/tauriApi";
import { useAgentStore } from "./useAgentStore";
import { useToastStore } from "./useToastStore";

interface LeaderStore {
  currentPlan: WorkPlan | null;
  plans: WorkPlan[];
  generating: boolean;
  streamOutput: string;

  generatePlan: (projectId: string, guidance: string) => Promise<void>;
  loadPlans: (projectId: string) => Promise<void>;
  approvePlan: (planId: string) => Promise<void>;
  rejectPlan: (planId: string) => Promise<void>;
  updateTaskStatus: (
    planId: string,
    taskId: string,
    status: TaskStatus,
  ) => Promise<void>;
  runTask: (planId: string, task: PlanTask) => Promise<void>;
  appendChunk: (chunk: string) => void;
  completeGeneration: (plan: WorkPlan) => void;
  reset: () => void;
}

export const useLeaderStore = create<LeaderStore>((set, get) => ({
  currentPlan: null,
  plans: [],
  generating: false,
  streamOutput: "",

  generatePlan: async (projectId, guidance) => {
    set({ generating: true, streamOutput: "", currentPlan: null });
    try {
      const plan = await generateWorkPlan(projectId, guidance);
      set({ currentPlan: plan, generating: false });
    } catch (e) {
      set({ generating: false });
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
    try {
      const plan = await updatePlanStatus(planId, "approved");
      set({ currentPlan: plan });
      // Update in plans list
      const plans = get().plans.map((p) => (p.id === planId ? plan : p));
      set({ plans });
    } catch (e) {
      useToastStore.getState().addToast(`Failed to approve plan: ${e}`);
    }
  },

  rejectPlan: async (planId) => {
    try {
      const plan = await updatePlanStatus(planId, "rejected");
      set({ currentPlan: plan });
      const plans = get().plans.map((p) => (p.id === planId ? plan : p));
      set({ plans });
    } catch (e) {
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

  runTask: async (planId, task) => {
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
      agentStore.completeRun(task.pieceId, { input: 0, output: 0 });
      return;
    }

    // Set up chunk listener before calling run
    const unlisten = await onAgentOutputChunk((payload) => {
      if (payload.pieceId !== task.pieceId) return;
      const store = useAgentStore.getState();
      if (payload.done) {
        store.completeRun(task.pieceId, payload.usage ?? { input: 0, output: 0 });
        unlisten();
        // Mark task as complete
        updatePlanTaskStatus(planId, task.id, "complete")
          .then((plan) => {
            set({ currentPlan: plan });
            const plans = get().plans.map((p) => (p.id === planId ? plan : p));
            set({ plans });
          })
          .catch(() => {});
      } else {
        store.appendChunk(task.pieceId, payload.chunk);
      }
    });

    try {
      await runPieceAgent(task.pieceId);
    } catch (e) {
      useToastStore.getState().addToast(`Agent error: ${e}`);
      agentStore.completeRun(task.pieceId, { input: 0, output: 0 });
      unlisten();
      // Revert task status
      updatePlanTaskStatus(planId, task.id, "pending")
        .then((plan) => {
          set({ currentPlan: plan });
          const plans = get().plans.map((p) => (p.id === planId ? plan : p));
          set({ plans });
        })
        .catch(() => {});
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
    });
  },
}));
