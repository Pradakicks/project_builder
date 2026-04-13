import { create } from "zustand";
import type {
  GoalRun,
  GoalRunPhase,
  GoalRunStatus,
  ProjectRuntimeStatus,
} from "../types";
import { devLog } from "../utils/devLog";
import * as goalRunApi from "../api/goalRunApi";
import * as leaderApi from "../api/leaderApi";
import * as runtimeApi from "../api/runtimeApi";
import { useToastStore } from "./useToastStore";

interface GoalRunStore {
  projectId: string | null;
  goalRuns: GoalRun[];
  currentGoalRun: GoalRun | null;
  runtimeStatus: ProjectRuntimeStatus | null;
  runtimeLogs: string[];
  loading: boolean;
  orchestrating: boolean;
  lastError: string | null;
  loadGoalRuns: (projectId: string) => Promise<void>;
  beginPromptRun: (projectId: string, prompt: string) => Promise<GoalRun>;
  continueAutopilot: (goalRunId: string) => Promise<void>;
  retryGoalRun: (goalRunId: string) => Promise<void>;
  refreshRuntimeStatus: (projectId?: string) => Promise<void>;
  startRuntime: (projectId?: string) => Promise<void>;
  stopRuntime: (projectId?: string) => Promise<void>;
  reset: () => void;
}

function toast(message: string, kind: "info" | "warning" = "warning") {
  useToastStore.getState().addToast(message, kind);
}

async function setGoalRunPhase(
  goalRunId: string,
  phase: GoalRunPhase,
  status: GoalRunStatus,
  extras: Partial<GoalRun> = {},
): Promise<GoalRun> {
  return goalRunApi.updateGoalRun(goalRunId, {
    phase,
    status,
    blockerReason:
      extras.blockerReason !== undefined ? extras.blockerReason : undefined,
    currentPlanId:
      extras.currentPlanId !== undefined ? extras.currentPlanId : undefined,
    runtimeStatusSummary:
      extras.runtimeStatusSummary !== undefined
        ? extras.runtimeStatusSummary
        : undefined,
    verificationSummary:
      extras.verificationSummary !== undefined
        ? extras.verificationSummary
        : undefined,
    retryCount:
      extras.retryCount !== undefined ? extras.retryCount : undefined,
    lastFailureSummary:
      extras.lastFailureSummary !== undefined
        ? extras.lastFailureSummary
        : undefined,
  });
}

export const useGoalRunStore = create<GoalRunStore>((set, get) => ({
  projectId: null,
  goalRuns: [],
  currentGoalRun: null,
  runtimeStatus: null,
  runtimeLogs: [],
  loading: false,
  orchestrating: false,
  lastError: null,

  loadGoalRuns: async (projectId) => {
    set({ projectId, loading: true, lastError: null });
    try {
      const [goalRuns, runtimeStatus] = await Promise.all([
        goalRunApi.listGoalRuns(projectId),
        runtimeApi.getRuntimeStatus(projectId).catch(() => null),
      ]);
      set({
        projectId,
        goalRuns,
        currentGoalRun: goalRuns[0] ?? null,
        runtimeStatus,
        loading: false,
      });
    } catch (error) {
      const message = `Failed to load goal runs: ${error}`;
      set({ loading: false, lastError: message });
      devLog("error", "Store:GoalRun", message);
    }
  },

  beginPromptRun: async (projectId, prompt) => {
    const goalRun = await goalRunApi.createGoalRun(projectId, prompt);
    set((state) => ({
      projectId,
      goalRuns: [goalRun, ...state.goalRuns.filter((run) => run.id !== goalRun.id)],
      currentGoalRun: goalRun,
      lastError: null,
    }));
    return goalRun;
  },

  continueAutopilot: async (goalRunId) => {
    const state = get();
    if (state.orchestrating) return;
    const goalRun =
      state.currentGoalRun?.id === goalRunId
        ? state.currentGoalRun
        : state.goalRuns.find((run) => run.id === goalRunId) ?? null;
    if (!goalRun) {
      throw new Error("Goal run not loaded");
    }

    set({ orchestrating: true, lastError: null });
    try {
      let currentGoalRun = goalRun;

      devLog("info", "Store:GoalRun", "Autopilot planning started", {
        goalRunId,
        projectId: goalRun.projectId,
      });

      let plans = await leaderApi.listWorkPlans(goalRun.projectId);
      let plan = plans.find((item) => item.status === "approved") ?? plans[0] ?? null;
      if (!plan || plan.status === "rejected" || plan.status === "superseded") {
        currentGoalRun = await setGoalRunPhase(goalRun.id, "planning", "running");
        plan = await leaderApi.generateWorkPlan(goalRun.projectId, goalRun.prompt);
      }

      currentGoalRun = await setGoalRunPhase(goalRun.id, "planning", "running", {
        currentPlanId: plan.id,
        blockerReason: null,
      });

      if (plan.status === "draft") {
        plan = await leaderApi.updatePlanStatus(plan.id, "approved");
      }

      currentGoalRun = await setGoalRunPhase(goalRun.id, "implementation", "running", {
        currentPlanId: plan.id,
      });
      await leaderApi.runAllPlanTasks(plan.id);

      plans = await leaderApi.listWorkPlans(goalRun.projectId);
      const finalPlan =
        plans.find((item) => item.id === plan!.id) ??
        (await leaderApi.getWorkPlan(plan.id));

      currentGoalRun = await setGoalRunPhase(goalRun.id, "runtime-configuration", "running");
      let runtimeStatus = await runtimeApi.getRuntimeStatus(goalRun.projectId);
      if (!runtimeStatus.spec) {
        const detected = await runtimeApi.detectRuntime(goalRun.projectId);
        if (!detected) {
          currentGoalRun = await setGoalRunPhase(goalRun.id, "runtime-configuration", "blocked", {
            blockerReason: "Runtime could not be auto-detected from the generated project.",
            lastFailureSummary: "Runtime detection failed",
            runtimeStatusSummary: "runtime not configured",
          });
          set((store) => ({
            currentGoalRun,
            goalRuns: store.goalRuns.map((run) =>
              run.id === currentGoalRun.id ? currentGoalRun : run,
            ),
            runtimeStatus,
          }));
          return;
        }

        await runtimeApi.configureRuntime(goalRun.projectId, detected);
        runtimeStatus = await runtimeApi.getRuntimeStatus(goalRun.projectId);
      }

      currentGoalRun = await setGoalRunPhase(goalRun.id, "runtime-execution", "running", {
        runtimeStatusSummary: runtimeStatus.spec?.runCommand
          ? `configured: ${runtimeStatus.spec.runCommand}`
          : "runtime configured",
      });

      runtimeStatus = await runtimeApi.startRuntime(goalRun.projectId);
      const verificationSummary = await runtimeApi.verifyRuntime(goalRun.projectId);
      const runtimeLogs = await runtimeApi.tailRuntimeLogs(goalRun.projectId, 120);

      currentGoalRun = await setGoalRunPhase(goalRun.id, "verification", "completed", {
        blockerReason: null,
        lastFailureSummary: null,
        runtimeStatusSummary: runtimeStatus.session?.url ?? "runtime running",
        verificationSummary:
          finalPlan.integrationReview?.trim()
            ? `${verificationSummary}\n\nIntegration review:\n${finalPlan.integrationReview.trim()}`
            : verificationSummary,
      });

      set((store) => ({
        currentGoalRun,
        goalRuns: [currentGoalRun, ...store.goalRuns.filter((run) => run.id !== currentGoalRun.id)],
        runtimeStatus,
        runtimeLogs: runtimeLogs.lines,
        orchestrating: false,
        lastError: null,
      }));
      toast("Autopilot run completed", "info");
    } catch (error) {
      const current = get().currentGoalRun;
      if (current?.id === goalRunId) {
        const failure =
          error instanceof Error ? error.message : String(error);
        const phase = current.phase;
        const blocked =
          phase === "runtime-configuration" || failure.toLowerCase().includes("runtime");
        const updated = await setGoalRunPhase(
          goalRunId,
          phase,
          blocked ? "blocked" : "failed",
          {
            blockerReason: blocked ? failure : null,
            lastFailureSummary: failure,
          },
        );
        set((state) => ({
          currentGoalRun: updated,
          goalRuns: state.goalRuns.map((run) => (run.id === updated.id ? updated : run)),
          orchestrating: false,
          lastError: failure,
        }));
      } else {
        set({ orchestrating: false, lastError: String(error) });
      }
      devLog("error", "Store:GoalRun", "Autopilot failed", error);
      toast(`Autopilot failed: ${error}`);
    }
  },

  retryGoalRun: async (goalRunId) => {
    const run =
      get().currentGoalRun?.id === goalRunId
        ? get().currentGoalRun
        : get().goalRuns.find((item) => item.id === goalRunId) ?? null;
    if (!run) return;
    const updated = await goalRunApi.updateGoalRun(goalRunId, {
      retryCount: run.retryCount + 1,
      status: "running",
      blockerReason: null,
    });
    set((state) => ({
      currentGoalRun: updated,
      goalRuns: state.goalRuns.map((item) => (item.id === updated.id ? updated : item)),
    }));
    await get().continueAutopilot(goalRunId);
  },

  refreshRuntimeStatus: async (projectId) => {
    const activeProjectId = projectId ?? get().projectId;
    if (!activeProjectId) return;
    const [runtimeStatus, logs] = await Promise.all([
      runtimeApi.getRuntimeStatus(activeProjectId),
      runtimeApi.tailRuntimeLogs(activeProjectId, 120).catch(() => ({
        path: null,
        lines: [],
      })),
    ]);
    set({ runtimeStatus, runtimeLogs: logs.lines });
  },

  startRuntime: async (projectId) => {
    const activeProjectId = projectId ?? get().projectId;
    if (!activeProjectId) return;
    const runtimeStatus = await runtimeApi.startRuntime(activeProjectId);
    const logs = await runtimeApi.tailRuntimeLogs(activeProjectId, 120).catch(() => ({
      path: null,
      lines: [],
    }));
    set({ runtimeStatus, runtimeLogs: logs.lines });
  },

  stopRuntime: async (projectId) => {
    const activeProjectId = projectId ?? get().projectId;
    if (!activeProjectId) return;
    const runtimeStatus = await runtimeApi.stopRuntime(activeProjectId);
    const logs = await runtimeApi.tailRuntimeLogs(activeProjectId, 120).catch(() => ({
      path: null,
      lines: [],
    }));
    set({ runtimeStatus, runtimeLogs: logs.lines });
  },

  reset: () =>
    set({
      projectId: null,
      goalRuns: [],
      currentGoalRun: null,
      runtimeStatus: null,
      runtimeLogs: [],
      loading: false,
      orchestrating: false,
      lastError: null,
    }),
}));
