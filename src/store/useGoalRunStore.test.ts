import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useGoalRunStore } from "./useGoalRunStore";
import * as runtimeApi from "../api/runtimeApi";

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
      loading: false,
      orchestrating: false,
      lastError: null,
      liveActivity: null,
      phaseActivity: null,
    });
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
});
