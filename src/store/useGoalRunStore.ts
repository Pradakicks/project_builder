import { create } from "zustand";
import type { GoalRun, GoalRunEvent, ProjectRuntimeStatus } from "../types";
import { devLog } from "../utils/devLog";
import * as goalRunApi from "../api/goalRunApi";
import * as runtimeApi from "../api/runtimeApi";
import { useToastStore } from "./useToastStore";

interface GoalRunStore {
  projectId: string | null;
  goalRuns: GoalRun[];
  currentGoalRun: GoalRun | null;
  goalRunEvents: GoalRunEvent[];
  runtimeStatus: ProjectRuntimeStatus | null;
  runtimeLogs: string[];
  loading: boolean;
  orchestrating: boolean;
  lastError: string | null;
  loadGoalRuns: (projectId: string) => Promise<void>;
  loadGoalRunEvents: (goalRunId: string) => Promise<void>;
  selectGoalRun: (goalRunId: string) => Promise<void>;
  beginPromptRun: (projectId: string, prompt: string) => Promise<GoalRun>;
  continueAutopilot: (goalRunId: string) => Promise<void>;
  retryGoalRun: (goalRunId: string) => Promise<void>;
  stopGoalRun: (goalRunId: string) => Promise<void>;
  refreshRuntimeStatus: (projectId?: string) => Promise<void>;
  startRuntime: (projectId?: string) => Promise<void>;
  stopRuntime: (projectId?: string) => Promise<void>;
  reset: () => void;
}

let pollTimer: ReturnType<typeof window.setInterval> | null = null;
let polledGoalRunId: string | null = null;

function toast(message: string, kind: "info" | "warning" = "warning") {
  useToastStore.getState().addToast(message, kind);
}

function syncGoalRunState(goalRun: GoalRun) {
  useGoalRunStore.setState((state) => ({
    currentGoalRun: goalRun,
    goalRuns: [goalRun, ...state.goalRuns.filter((run) => run.id !== goalRun.id)],
    orchestrating: goalRun.status === "running",
  }));
}

async function refreshGoalRunState(goalRunId: string) {
  const store = useGoalRunStore.getState();
  const activeProjectId = store.projectId;
  const goalRun = await goalRunApi.getGoalRun(goalRunId);
  const [events, runtimeStatus, logs] = await Promise.all([
    goalRunApi.getGoalRunEvents(goalRunId).catch(() => []),
    activeProjectId
      ? runtimeApi.getRuntimeStatus(activeProjectId).catch(() => null)
      : Promise.resolve(null),
    activeProjectId
      ? runtimeApi.tailRuntimeLogs(activeProjectId, 120).catch(() => ({
          path: null,
          lines: [],
        }))
      : Promise.resolve({ path: null, lines: [] }),
  ]);

  useGoalRunStore.setState((state) => ({
    currentGoalRun:
      state.currentGoalRun?.id === goalRun.id ? goalRun : state.currentGoalRun,
    goalRuns: [goalRun, ...state.goalRuns.filter((run) => run.id !== goalRun.id)],
    goalRunEvents: state.currentGoalRun?.id === goalRun.id ? events : state.goalRunEvents,
    runtimeStatus: runtimeStatus ?? state.runtimeStatus,
    runtimeLogs: logs.lines,
    orchestrating: goalRun.status === "running",
    lastError:
      goalRun.status === "failed" || goalRun.status === "blocked"
        ? goalRun.lastFailureSummary ?? goalRun.blockerReason ?? state.lastError
        : null,
  }));

  if (goalRun.status !== "running") {
    stopPolling();
  }
}

function stopPolling() {
  if (pollTimer) {
    window.clearInterval(pollTimer);
    pollTimer = null;
  }
  polledGoalRunId = null;
}

function ensurePolling(goalRunId: string) {
  if (polledGoalRunId === goalRunId && pollTimer) {
    return;
  }
  stopPolling();
  polledGoalRunId = goalRunId;
  pollTimer = window.setInterval(() => {
    void refreshGoalRunState(goalRunId).catch((error) => {
      devLog("warn", "Store:GoalRun", "Failed to refresh running goal run", error);
    });
  }, 2000);
  void refreshGoalRunState(goalRunId).catch((error) => {
    devLog("warn", "Store:GoalRun", "Failed to prime running goal run", error);
  });
}

export const useGoalRunStore = create<GoalRunStore>((set, get) => ({
  projectId: null,
  goalRuns: [],
  currentGoalRun: null,
  goalRunEvents: [],
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
      const currentGoalRun = goalRuns[0] ?? null;
      const goalRunEvents = currentGoalRun
        ? await goalRunApi.getGoalRunEvents(currentGoalRun.id).catch(() => [])
        : [];
      set({
        projectId,
        goalRuns,
        currentGoalRun,
        goalRunEvents,
        runtimeStatus,
        loading: false,
        orchestrating: currentGoalRun?.status === "running",
      });
      if (currentGoalRun?.status === "running") {
        ensurePolling(currentGoalRun.id);
      } else {
        stopPolling();
      }
    } catch (error) {
      const message = `Failed to load goal runs: ${error}`;
      set({ loading: false, lastError: message });
      devLog("error", "Store:GoalRun", message);
    }
  },

  loadGoalRunEvents: async (goalRunId) => {
    try {
      const events = await goalRunApi.getGoalRunEvents(goalRunId);
      set((state) => ({
        goalRunEvents:
          state.currentGoalRun?.id === goalRunId ? events : state.goalRunEvents,
      }));
    } catch (error) {
      devLog("warn", "Store:GoalRun", "Failed to load goal run events", error);
    }
  },

  selectGoalRun: async (goalRunId) => {
    const current =
      get().goalRuns.find((run) => run.id === goalRunId) ??
      (await goalRunApi.getGoalRun(goalRunId));
    syncGoalRunState(current);
    const events = await goalRunApi.getGoalRunEvents(goalRunId).catch(() => []);
    set({ goalRunEvents: events });
    if (current.status === "running") {
      ensurePolling(goalRunId);
    }
  },

  beginPromptRun: async (projectId, prompt) => {
    const goalRun = await goalRunApi.createGoalRun(projectId, prompt);
    set({ projectId, lastError: null });
    syncGoalRunState(goalRun);
    set({ goalRunEvents: [] });
    return goalRun;
  },

  continueAutopilot: async (goalRunId) => {
    const run =
      get().currentGoalRun?.id === goalRunId
        ? get().currentGoalRun
        : get().goalRuns.find((item) => item.id === goalRunId) ?? null;
    if (!run) {
      throw new Error("Goal run not loaded");
    }

    set({ lastError: null });
    const resumed = await goalRunApi.resumeGoalRun(goalRunId);
    syncGoalRunState(resumed);
    ensurePolling(goalRunId);
    toast("Autopilot resumed", "info");
  },

  retryGoalRun: async (goalRunId) => {
    const run =
      get().currentGoalRun?.id === goalRunId
        ? get().currentGoalRun
        : get().goalRuns.find((item) => item.id === goalRunId) ?? null;
    if (!run) return;

    await goalRunApi.updateGoalRun(goalRunId, {
      retryCount: run.retryCount + 1,
      blockerReason: null,
      lastFailureSummary: null,
      attentionRequired: false,
      stopRequested: false,
    });
    await get().continueAutopilot(goalRunId);
  },

  stopGoalRun: async (goalRunId) => {
    const stopped = await goalRunApi.stopGoalRun(goalRunId);
    syncGoalRunState(stopped);
    await get().loadGoalRunEvents(goalRunId);
    stopPolling();
    toast("Autopilot stopped", "info");
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

  reset: () => {
    stopPolling();
    set({
      projectId: null,
      goalRuns: [],
      currentGoalRun: null,
      goalRunEvents: [],
      runtimeStatus: null,
      runtimeLogs: [],
      loading: false,
      orchestrating: false,
      lastError: null,
    });
  },
}));
