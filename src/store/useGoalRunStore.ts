import { create } from "zustand";
import type {
  GoalRun,
  GoalRunDeliverySnapshot,
  GoalRunEvent,
  LiveActivity,
  PhaseActivity,
  ProjectRuntimeStatus,
  RuntimeLogTail,
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
  runtimeLogsUpdatedAt: string | null;
  loading: boolean;
  orchestrating: boolean;
  lastError: string | null;
  /** Live activity during Implementation phase — updated from events at sub-2s cadence. */
  liveActivity: LiveActivity | null;
  /** Rolling breadcrumb for the current phase (any phase). Updated from
   *  `phase-progress` events so the UI can show "Merging branch X" or
   *  "Running shell verify" instead of a frozen spinner. */
  phaseActivity: PhaseActivity | null;
  loadGoalRuns: (projectId: string) => Promise<void>;
  loadGoalRunEvents: (goalRunId: string) => Promise<void>;
  refreshDeliverySnapshot: (goalRunId: string) => Promise<void>;
  selectGoalRun: (goalRunId: string) => Promise<void>;
  beginPromptRun: (projectId: string, prompt: string) => Promise<GoalRun>;
  continueAutopilot: (goalRunId: string) => Promise<void>;
  continueAutopilotWithRepair: (goalRunId: string) => Promise<void>;
  retryGoalRun: (goalRunId: string) => Promise<void>;
  stopGoalRun: (goalRunId: string) => Promise<void>;
  pauseGoalRun: (goalRunId: string) => Promise<void>;
  cancelGoalRun: (goalRunId: string) => Promise<void>;
  rerunVerification: (goalRunId: string) => Promise<void>;
  refreshRuntimeStatus: (projectId?: string) => Promise<void>;
  startRuntime: (projectId?: string) => Promise<void>;
  stopRuntime: (projectId?: string) => Promise<void>;
  reset: () => void;
}

let pollTimer: ReturnType<typeof window.setInterval> | null = null;
let polledGoalRunId: string | null = null;
let unlistenProgress: (() => void) | null = null;
let unlistenPhaseProgress: (() => void) | null = null;
let runtimeLogRefreshBurstToken = 0;
// Monotonic session counter. Each `stopPolling` / `ensurePolling` bumps this so
// async listener registrations that resolve after their session ended can self-
// cancel instead of leaking a subscription that mutates stale state.
let pollSession = 0;

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

function mergeRuntimeLogs(
  currentLogs: string[],
  currentUpdatedAt: string | null,
  runtimeStatus: ProjectRuntimeStatus | null,
  tail: RuntimeLogTail | null,
  refreshedAt: string,
) {
  if (tail && tail.lines.length > 0) {
    return {
      runtimeLogs: tail.lines,
      runtimeLogsUpdatedAt: refreshedAt,
    };
  }

  const snapshotLogs = runtimeStatus?.session?.recentLogs ?? [];
  if (snapshotLogs.length > 0) {
    return {
      runtimeLogs: snapshotLogs,
      runtimeLogsUpdatedAt: runtimeStatus?.session?.updatedAt ?? refreshedAt,
    };
  }

  return {
    runtimeLogs: currentLogs,
    runtimeLogsUpdatedAt: currentUpdatedAt,
  };
}

function projectMismatch(activeProjectId: string) {
  return Boolean(useGoalRunStore.getState().projectId && useGoalRunStore.getState().projectId !== activeProjectId);
}

async function syncRuntimeEvidence(activeProjectId: string, refreshedAt = new Date().toISOString()) {
  const [runtimeStatus, logs] = await Promise.all([
    runtimeApi.getRuntimeStatus(activeProjectId),
    runtimeApi.tailRuntimeLogs(activeProjectId, 120).catch(() => ({
      path: null,
      lines: [],
    })),
  ]);

  if (projectMismatch(activeProjectId)) return;

  useGoalRunStore.setState((state) => {
    const merged = mergeRuntimeLogs(
      state.runtimeLogs,
      state.runtimeLogsUpdatedAt,
      runtimeStatus,
      logs,
      refreshedAt,
    );
    return {
      runtimeStatus,
      runtimeLogs: merged.runtimeLogs,
      runtimeLogsUpdatedAt: merged.runtimeLogsUpdatedAt,
    };
  });
}

function cancelRuntimeLogRefreshBurst() {
  runtimeLogRefreshBurstToken += 1;
}

function scheduleRuntimeLogRefreshBurst(activeProjectId: string) {
  cancelRuntimeLogRefreshBurst();
  const token = runtimeLogRefreshBurstToken;
  for (const delay of [700, 1600, 2800]) {
    window.setTimeout(() => {
      if (token !== runtimeLogRefreshBurstToken) return;
      if (projectMismatch(activeProjectId)) return;
      void syncRuntimeEvidence(activeProjectId).catch((error) => {
        devLog("warn", "Store:GoalRun", "Failed to refresh post-start runtime logs", error);
      });
    }, delay);
  }
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
    ...mergeRuntimeLogs(
      state.runtimeLogs,
      state.runtimeLogsUpdatedAt,
      snapshot.runtimeStatus ?? state.runtimeStatus,
      { path: null, lines: [] },
      snapshot.runtimeStatus?.session?.updatedAt ?? new Date().toISOString(),
    ),
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
  pollSession += 1;
  cancelRuntimeLogRefreshBurst();
  if (pollTimer) {
    window.clearInterval(pollTimer);
    pollTimer = null;
  }
  if (unlistenProgress) {
    unlistenProgress();
    unlistenProgress = null;
  }
  if (unlistenPhaseProgress) {
    unlistenPhaseProgress();
    unlistenPhaseProgress = null;
  }
  polledGoalRunId = null;
  useGoalRunStore.setState({ liveActivity: null, phaseActivity: null });
}

function ensurePolling(goalRunId: string) {
  if (polledGoalRunId === goalRunId && pollTimer) {
    return;
  }
  stopPolling();
  const mySession = ++pollSession;
  polledGoalRunId = goalRunId;

  // Register event listener for sub-2s live activity updates. Registration is
  // async, so a rapid re-poll can resolve it after `stopPolling` has already
  // advanced `pollSession` — in that case we bail / unlisten.
  void goalRunApi.onImplementationProgress((event) => {
    if (mySession !== pollSession) return;
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
    if (mySession !== pollSession) {
      unlisten();
      return;
    }
    unlistenProgress = unlisten;
  }).catch((error) => {
    devLog("warn", "Store:GoalRun", "Failed to register implementation-progress listener", error);
  });

  // Broader breadcrumb coverage: any phase can emit phase-progress.
  void goalRunApi.onPhaseProgress((event) => {
    if (mySession !== pollSession) return;
    if (event.goalRunId !== goalRunId) return;
    if (event.status === "completed" || event.status === "failed") {
      // Terminal states: clear so we don't pin a stale "Merging branch X" after
      // the phase moves on. Next `step`/`started` will repopulate.
      useGoalRunStore.setState({ phaseActivity: null });
      return;
    }
    useGoalRunStore.setState({
      phaseActivity: {
        phase: event.phase,
        status: event.status,
        message: event.message,
        pieceId: event.pieceId ?? null,
        pieceName: event.pieceName ?? null,
        stepIndex: event.stepIndex ?? null,
        stepTotal: event.stepTotal ?? null,
        updatedAt: new Date().toISOString(),
      },
    });
  }).then((unlisten) => {
    if (mySession !== pollSession) {
      unlisten();
      return;
    }
    unlistenPhaseProgress = unlisten;
  }).catch((error) => {
    devLog("warn", "Store:GoalRun", "Failed to register phase-progress listener", error);
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
  runtimeLogsUpdatedAt: null,
  loading: false,
  orchestrating: false,
  lastError: null,
  liveActivity: null,
  phaseActivity: null,

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
        ...mergeRuntimeLogs(
          [],
          null,
          snapshot?.runtimeStatus ?? runtimeStatus,
          { path: null, lines: [] },
          snapshot?.runtimeStatus?.session?.updatedAt ?? new Date().toISOString(),
        ),
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
        ...(state.currentGoalRun?.id === goalRunId
          ? mergeRuntimeLogs(
              state.runtimeLogs,
              state.runtimeLogsUpdatedAt,
              snapshot.runtimeStatus,
              { path: null, lines: [] },
              snapshot.runtimeStatus?.session?.updatedAt ?? new Date().toISOString(),
            )
          : {
              runtimeLogs: state.runtimeLogs,
              runtimeLogsUpdatedAt: state.runtimeLogsUpdatedAt,
            }),
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

  continueAutopilotWithRepair: async (goalRunId) => {
    const run =
      get().currentGoalRun?.id === goalRunId
        ? get().currentGoalRun
        : get().goalRuns.find((item) => item.id === goalRunId) ?? null;
    if (!run) {
      throw new Error("Goal run not loaded");
    }

    set({ lastError: null });
    const resumed = await goalRunApi.resumeGoalRunWithRepair(goalRunId);
    syncGoalRunState(resumed);
    ensurePolling(goalRunId);
    toast("Repair requested", "info");
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

  pauseGoalRun: async (goalRunId) => {
    const paused = await goalRunApi.pauseGoalRun(goalRunId);
    syncGoalRunState(paused);
    await get().refreshDeliverySnapshot(goalRunId);
    stopPolling();
    toast("Autopilot paused — resume when ready", "info");
  },

  cancelGoalRun: async (goalRunId) => {
    const cancelled = await goalRunApi.cancelGoalRun(goalRunId);
    syncGoalRunState(cancelled);
    await get().refreshDeliverySnapshot(goalRunId);
    stopPolling();
    toast("Autopilot cancelled", "info");
  },

  rerunVerification: async (goalRunId) => {
    const run = await goalRunApi.rerunVerification(goalRunId);
    syncGoalRunState(run);
    ensurePolling(goalRunId);
    toast("Rerunning verification", "info");
  },

  refreshRuntimeStatus: async (projectId) => {
    const activeProjectId = projectId ?? get().projectId;
    if (!activeProjectId) return;
    try {
      await syncRuntimeEvidence(activeProjectId);
    } catch (error) {
      devLog("warn", "Store:GoalRun", "Failed to refresh runtime status", error);
    }
  },

  startRuntime: async (projectId) => {
    const activeProjectId = projectId ?? get().projectId;
    if (!activeProjectId) return;
    const runtimeStatus = await runtimeApi.startRuntime(activeProjectId);
    const logs = await runtimeApi.tailRuntimeLogs(activeProjectId, 120).catch(() => ({
      path: null,
      lines: [],
    }));
    if (projectMismatch(activeProjectId)) return;
    const refreshedAt = new Date().toISOString();
    useGoalRunStore.setState((state) => {
      const merged = mergeRuntimeLogs(
        state.runtimeLogs,
        state.runtimeLogsUpdatedAt,
        runtimeStatus,
        logs,
        refreshedAt,
      );
      return {
        runtimeStatus,
        runtimeLogs: merged.runtimeLogs,
        runtimeLogsUpdatedAt: merged.runtimeLogsUpdatedAt,
      };
    });
    scheduleRuntimeLogRefreshBurst(activeProjectId);
  },

  stopRuntime: async (projectId) => {
    const activeProjectId = projectId ?? get().projectId;
    if (!activeProjectId) return;
    const runtimeStatus = await runtimeApi.stopRuntime(activeProjectId);
    const logs = await runtimeApi.tailRuntimeLogs(activeProjectId, 120).catch(() => ({
      path: null,
      lines: [],
    }));
    if (projectMismatch(activeProjectId)) return;
    const refreshedAt = new Date().toISOString();
    useGoalRunStore.setState((state) => {
      const merged = mergeRuntimeLogs(
        state.runtimeLogs,
        state.runtimeLogsUpdatedAt,
        runtimeStatus,
        logs,
        refreshedAt,
      );
      return {
        runtimeStatus,
        runtimeLogs: merged.runtimeLogs,
        runtimeLogsUpdatedAt: merged.runtimeLogsUpdatedAt,
      };
    });
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
      runtimeLogsUpdatedAt: null,
      loading: false,
      orchestrating: false,
      lastError: null,
      liveActivity: null,
      phaseActivity: null,
    });
  },
}));
