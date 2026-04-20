import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useGoalRunStore } from "./useGoalRunStore";
import * as runtimeApi from "../api/runtimeApi";
import type { GoalRun, GoalRunRetryState, ProjectRuntimeStatus } from "../types";
import {
  buildFailureView,
  selectRuntimeLogView,
} from "../components/delivery/DeliveryPanel";

vi.mock("../api/runtimeApi", () => ({
  getRuntimeStatus: vi.fn(),
  tailRuntimeLogs: vi.fn(),
  startRuntime: vi.fn(),
  stopRuntime: vi.fn(),
}));

vi.mock("../api/goalRunApi", () => ({
  getGoalRunDeliverySnapshot: vi.fn(),
  listGoalRuns: vi.fn(),
  getGoalRun: vi.fn(),
  createGoalRun: vi.fn(),
  resumeGoalRun: vi.fn(),
  resumeGoalRunWithRepair: vi.fn(),
  updateGoalRun: vi.fn(),
  stopGoalRun: vi.fn(),
  pauseGoalRun: vi.fn(),
  cancelGoalRun: vi.fn(),
  rerunVerification: vi.fn(),
  onImplementationProgress: vi.fn(),
  onPhaseProgress: vi.fn(),
}));

describe("useGoalRunStore runtime evidence", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-04-18T12:00:00.000Z"));
    useGoalRunStore.setState({
      projectId: "project-1",
      goalRuns: [],
      currentGoalRun: null,
      deliverySnapshot: null,
      goalRunEvents: [],
      runtimeStatus: null,
      runtimeLogs: [],
      runtimeLogsUpdatedAt: null,
      actionReceipts: [],
      loading: false,
      orchestrating: false,
      lastError: null,
      liveActivity: null,
      phaseActivity: null,
    });
    vi.clearAllMocks();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("prefers the fresh log tail over stale snapshot logs", async () => {
    vi.mocked(runtimeApi.getRuntimeStatus).mockResolvedValue({
      projectId: "project-1",
      spec: null,
      session: {
        sessionId: "runtime-session-1",
        status: "running",
        startedAt: "2026-04-18T11:55:00.000Z",
        updatedAt: "2026-04-18T11:58:00.000Z",
        endedAt: null,
        url: "http://127.0.0.1:3030",
        portHint: 3030,
        logPath: "/tmp/runtime.log",
        recentLogs: ["stale snapshot line"],
        lastError: null,
        exitCode: null,
        pid: 1234,
      },
    });
    vi.mocked(runtimeApi.tailRuntimeLogs).mockResolvedValue({
      path: "/tmp/runtime.log",
      lines: ["fresh line 1", "fresh line 2"],
    });

    await useGoalRunStore.getState().refreshRuntimeStatus("project-1");

    const state = useGoalRunStore.getState();
    expect(state.runtimeLogs).toEqual(["fresh line 1", "fresh line 2"]);
    expect(state.runtimeLogsUpdatedAt).toBe("2026-04-18T12:00:00.000Z");
    expect(state.runtimeStatus?.session?.recentLogs).toEqual(["stale snapshot line"]);
    expect(vi.mocked(runtimeApi.tailRuntimeLogs)).toHaveBeenCalledWith("project-1", 120);
  });

  it("records a persistent receipt when starting the runtime", async () => {
    vi.mocked(runtimeApi.startRuntime).mockResolvedValue({
      projectId: "project-1",
      spec: null,
      session: {
        sessionId: "runtime-session-1",
        status: "running",
        startedAt: "2026-04-18T11:55:00.000Z",
        updatedAt: "2026-04-18T12:00:01.000Z",
        endedAt: null,
        url: "http://127.0.0.1:3030",
        portHint: 3030,
        logPath: "/tmp/runtime.log",
        recentLogs: [],
        lastError: null,
        exitCode: null,
        pid: 1234,
      },
    } satisfies ProjectRuntimeStatus);
    vi.mocked(runtimeApi.tailRuntimeLogs).mockResolvedValue({
      path: "/tmp/runtime.log",
      lines: ["runtime booted"],
    });

    const action = useGoalRunStore.getState().startRuntime("project-1");

    expect(useGoalRunStore.getState().actionReceipts[0]).toMatchObject({
      action: "start-runtime",
      status: "pending",
      projectId: "project-1",
      summary: "Starting app",
      finishedAt: null,
    });

    await action;

    expect(useGoalRunStore.getState().actionReceipts[0]).toMatchObject({
      action: "start-runtime",
      status: "succeeded",
      projectId: "project-1",
      summary: "App started",
      detail: "Runtime session runtime-session-1 at http://127.0.0.1:3030",
      finishedAt: "2026-04-18T12:00:00.000Z",
    });
  });

  it("separates current blockers from historical failures and labels log provenance", () => {
    const runtimeView = selectRuntimeLogView(
      ["live line"],
      "2026-04-18T11:59:30.000Z",
      ["snapshot line"],
      "2026-04-18T11:58:00.000Z",
    );

    expect(runtimeView).toMatchObject({
      source: "live",
      sourceLabel: "live tail",
      lines: ["live line"],
      updatedAt: "2026-04-18T11:59:30.000Z",
    });

    const currentRun = makeGoalRun({
      status: "blocked",
      blockerReason: "Current blocker text",
      lastFailureSummary: "Current blocker text",
      updatedAt: "2026-04-18T11:59:00.000Z",
    });
    const retryState: GoalRunRetryState = {
      retryCount: 3,
      stopRequested: false,
      retryBackoffUntil: null,
      lastFailureSummary: "Historical failure text",
      lastFailureFingerprint: "abc123",
      attentionRequired: true,
      operatorRepairRequested: false,
    };

    const failureView = buildFailureView(currentRun, retryState, {
      passed: false,
      checks: [],
      startedAt: "2026-04-18T11:57:00.000Z",
      finishedAt: "2026-04-18T11:59:00.000Z",
      message: "Verification failed",
    });

    expect(failureView.currentBlocker).toMatchObject({
      text: "Current blocker text",
      freshnessLabel: "current",
    });
    expect(failureView.previousFailures).toEqual([
      expect.objectContaining({
        label: "Retry failure",
        text: "Historical failure text",
        freshnessLabel: "historical",
      }),
      expect.objectContaining({
        label: "Verification",
        text: "Verification failed",
        freshnessLabel: "historical",
      }),
    ]);
  });
});

function makeGoalRun(overrides: Partial<GoalRun> = {}): GoalRun {
  return {
    id: "goal-run-1",
    projectId: "project-1",
    prompt: "Build the smoke test",
    phase: "verification",
    status: "running",
    blockerReason: null,
    currentPlanId: null,
    runtimeStatusSummary: null,
    verificationSummary: null,
    retryCount: 0,
    lastFailureSummary: null,
    stopRequested: false,
    currentPieceId: null,
    currentTaskId: null,
    retryBackoffUntil: null,
    lastFailureFingerprint: null,
    attentionRequired: false,
    lastHeartbeatAt: null,
    operatorRepairRequested: false,
    createdAt: "2026-04-18T11:50:00.000Z",
    updatedAt: "2026-04-18T11:55:00.000Z",
    ...overrides,
  };
}
