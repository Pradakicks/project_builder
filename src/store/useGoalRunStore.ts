import { create } from "zustand";
import type {
  GoalRun,
  GoalRunDeliverySnapshot,
  GoalRunEvent,
  LiveActivity,
  ProjectRuntimeStatus,
} from "../types";
import { devLog } from "../utils/devLog";
import * as goalRunApi from "../api/goalRunApi";
import * as runtimeApi from "../api/runtimeApi";
import { useToastStore } from "./useToastStore";

interface GoalRunStore {
  projectId: string | null;
  goalRuns: GoalRun[];
  currentGoalRun: GoalRun | null;
  deliverySnapshot: GoalRunDeliverySnapshot | null;
  goalRunEvents: GoalRunEvent[];
  runtimeStatus: ProjectRuntimeStatus | null;
  runtimeLogs: string[];
  loading: boolean;
  orchestrating: boolean;
  lastError: string | null;
  /** Live activity during Implementation phase — updated from events at sub-2s cadence. */
  liveActivity: LiveActivity | null;
  loadGoalRuns: (projectId: string) => Promise<void>;
  loadGoalRunEvents: (goalRunId: string) => Promise<void>;
  refreshDeliverySnapshot: (goalRunId: string) => Promise<void>;
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
let unlistenProgress: (() => void) | null = null;

function toast(message: string, kind: "info" | "warning" = "warning") {
  useToastStore.getState().addToast(message, kind);
}

function syncGoalRunState(goalRun: GoalRun) {
  useGoalRunStore.setState((state) => ({
    currentGoalRun: goalRun,
    goalRuns: [goalRun, ...state.goalRuns.filter((run) => run.id !== goalRun.id)],
    orchestrating:
      goalRun.status === "running" || goalRun.status === "retrying",
  }));
}

async function refreshGoalRunState(goalRunId: string) {
  const snapshot = await goalRunApi.getGoalRunDeliverySnapshot(goalRunId);
  const goalRun = snapshot.goalRun;

  useGoalRunStore.setState((state) => ({
    currentGoalRun:
      state.currentGoalRun?.id === goalRun.id ? goalRun : state.currentGoalRun,
    goalRuns: [goalRun, ...state.goalRuns.filter((run) => run.id !== goalRun.id)],
    deliverySnapshot:
      state.currentGoalRun?.id === goalRun.id ? snapshot : state.deliverySnapshot,
    goalRunEvents:
      state.currentGoalRun?.id === goalRun.id
        ? snapshot.recentEvents
        : state.goalRunEvents,
    runtimeStatus: snapshot.runtimeStatus ?? state.runtimeStatus,
    runtimeLogs: snapshot.runtimeStatus?.session?.recentLogs ?? [],
    orchestrating:
      goalRun.status === "running" || goalRun.status === "retrying",
    // Reconcile liveActivity from snapshot if the event-driven state is stale
    liveActivity:
      state.currentGoalRun?.id === goalRun.id
        ? (snapshot.liveActivity ?? state.liveActivity)
        : state.liveActivity,
    lastError:
      goalRun.status === "failed" ||
      goalRun.status === "blocked" ||
      goalRun.status === "retrying"
        ? goalRun.lastFailureSummary ?? goalRun.blockerReason ?? state.lastError
        : null,
  }));

  if (goalRun.status !== "running" && goalRun.status !== "retrying") {
    stopPolling();
  }
}

function stopPolling() {
  if (pollTimer) {
    window.clearInterval(pollTimer);
    pollTimer = null;
  }
  if (unlistenProgress) {
    unlistenProgress();
    unlistenProgress = null;
  }
  polledGoalRunId = null;
  useGoalRunStore.setState({ liveActivity: null });
}

function ensurePolling(goalRunId: string) {
  if (polledGoalRunId === goalRunId && pollTimer) {
    return;
  }
  stopPolling();
  polledGoalRunId = goalRunId;

  // Register event listener for sub-2s live activity updates
  void goalRunApi.onImplementationProgress((event) => {
    if (event.goalRunId !== goalRunId) return;
    if (event.status === "started") {
      useGoalRunStore.setState({
        liveActivity: {
          pieceId: event.pieceId,
          pieceName: event.pieceName,
          taskId: event.taskId,
          taskTitle: event.taskTitle,
          engine: event.engine,
          currentIndex: event.current,
          total: event.total,
        },
      });
    } else {
      // completed or failed — clear live activity (next poll will reconcile)
      useGoalRunStore.setState({ liveActivity: null });
    }
  }).then((unlisten) => {
    unlistenProgress = unlisten;
  }).catch((error) => {
    devLog("warn", "Store:GoalRun", "Failed to register implementation-progress listener", error);
  });

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
  deliverySnapshot: null,
  goalRunEvents: [],
  runtimeStatus: null,
  runtimeLogs: [],
  loading: false,
  orchestrating: false,
  lastError: null,
  liveActivity: null,

  loadGoalRuns: async (projectId) => {
    set({ projectId, loading: true, lastError: null });
    try {
      const [goalRuns, runtimeStatus] = await Promise.all([
        goalRunApi.listGoalRuns(projectId),
        runtimeApi.getRuntimeStatus(projectId).catch(() => null),
      ]);
      const currentGoalRun = goalRuns[0] ?? null;
      const snapshot = currentGoalRun
        ? await goalRunApi.getGoalRunDeliverySnapshot(currentGoalRun.id).catch(() => null)
        : null;
      set({
        projectId,
        goalRuns,
        currentGoalRun,
        deliverySnapshot: snapshot,
        goalRunEvents: snapshot?.recentEvents ?? [],
        runtimeStatus: snapshot?.runtimeStatus ?? runtimeStatus,
        runtimeLogs: snapshot?.runtimeStatus?.session?.recentLogs ?? [],
        loading: false,
        orchestrating:
          currentGoalRun?.status === "running" ||
          currentGoalRun?.status === "retrying",
      });
      if (
        currentGoalRun?.status === "running" ||
        currentGoalRun?.status === "retrying"
      ) {
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
      const snapshot = await goalRunApi.getGoalRunDeliverySnapshot(goalRunId);
      set((state) => ({
        deliverySnapshot:
          state.currentGoalRun?.id === goalRunId ? snapshot : state.deliverySnapshot,
        goalRunEvents:
          state.currentGoalRun?.id === goalRunId
            ? snapshot.recentEvents
            : state.goalRunEvents,
        runtimeStatus:
          state.currentGoalRun?.id === goalRunId
            ? snapshot.runtimeStatus
            : state.runtimeStatus,
        runtimeLogs:
          state.currentGoalRun?.id === goalRunId
            ? snapshot.runtimeStatus?.session?.recentLogs ?? []
            : state.runtimeLogs,
      }));
    } catch (error) {
      devLog("warn", "Store:GoalRun", "Failed to load goal run events", error);
    }
  },

  refreshDeliverySnapshot: async (goalRunId) => {
    await refreshGoalRunState(goalRunId);
  },

  selectGoalRun: async (goalRunId) => {
    const current =
      get().goalRuns.find((run) => run.id === goalRunId) ??
      (await goalRunApi.getGoalRun(goalRunId));
    syncGoalRunState(current);
    await get().refreshDeliverySnapshot(goalRunId);
    if (current.status === "running" || current.status === "retrying") {
      ensurePolling(goalRunId);
    }
  },

  beginPromptRun: async (projectId, prompt) => {
    const goalRun = await goalRunApi.createGoalRun(projectId, prompt);
    set({ projectId, lastError: null });
    syncGoalRunState(goalRun);
    set({ deliverySnapshot: null, goalRunEvents: [] });
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
    await get().refreshDeliverySnapshot(goalRunId);
  },

  stopGoalRun: async (goalRunId) => {
    const stopped = await goalRunApi.stopGoalRun(goalRunId);
    syncGoalRunState(stopped);
    await get().refreshDeliverySnapshot(goalRunId);
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
      deliverySnapshot: null,
      goalRunEvents: [],
      runtimeStatus: null,
      runtimeLogs: [],
      loading: false,
      orchestrating: false,
      lastError: null,
      liveActivity: null,
    });
  },
}));
