import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  generateWorkPlan: vi.fn(),
  leaderGeneratePlan: vi.fn(),
  createPiece: vi.fn(),
  runPieceAgent: vi.fn(),
  onAgentOutputChunk: vi.fn(),
  appState: {
    activeProjectId: "project-1",
  },
  projectState: {
    project: { id: "project-1" } as { id: string } | null,
  },
}));

vi.mock("../api/projectApi", () => ({
  listPieces: vi.fn(),
  createPiece: mocks.createPiece,
}));

vi.mock("../api/tauriApi", async () => {
  const actual = await vi.importActual<typeof import("../api/tauriApi")>("../api/tauriApi");
  return {
    ...actual,
    createPiece: mocks.createPiece,
  };
});

vi.mock("../store/useAppStore", () => ({
  useAppStore: {
    getState: () => mocks.appState,
  },
}));

vi.mock("../store/useProjectStore", () => ({
  useProjectStore: {
    getState: () => mocks.projectState,
  },
}));

vi.mock("../store/useLeaderStore", () => ({
  useLeaderStore: {
    getState: () => ({
      generatePlan: mocks.leaderGeneratePlan,
    }),
  },
}));

vi.mock("../api/leaderApi", () => ({
  generateWorkPlan: mocks.generateWorkPlan,
  runPieceAgent: mocks.runPieceAgent,
  onAgentOutputChunk: mocks.onAgentOutputChunk,
}));

import { describeAction, executeActions, reviewActions } from "./ctoActions";

beforeEach(() => {
  vi.clearAllMocks();
});

describe("CTO action parsing", () => {
  it("rejects malformed generatePlan blocks with a stable validation error", () => {
    const review = reviewActions([
      "Let's create a simple todo web app",
      "```action",
      '`{"action":"generatePlan","guidance":"Develop the todo web app based on the established component connections."}',
      "```",
    ].join("\n"));

    expect(review.actions).toHaveLength(0);
    expect(review.validationErrors).toHaveLength(1);
    expect(review.validationErrors[0]).toContain("Action 1: invalid JSON");
    expect(review.cleanedContent).toBe("Let's create a simple todo web app");
  });

  it("parses and describes a valid generatePlan block", () => {
    const review = reviewActions([
      "We can generate the plan now.",
      "```action",
      '{"action":"generatePlan","guidance":"Develop the todo web app"}',
      "```",
    ].join("\n"));

    expect(review.validationErrors).toHaveLength(0);
    expect(review.actions).toEqual([
      {
        action: "generatePlan",
        guidance: "Develop the todo web app",
      },
    ]);
    expect(describeAction(review.actions[0])).toBe(
      'Generate work plan: "Develop the todo web app"',
    );
  });

  it("parses a create-and-run piece sequence with execution settings", () => {
    const review = reviewActions([
      "Create the frontend app and run it.",
      "```action",
      '{"action":"createPiece","ref":"frontend","name":"Todo App","pieceType":"web-app","responsibilities":"Build the todo UI","agentPrompt":"Create a minimal todo app in the repo","outputMode":"code-only","executionEngine":"codex"}',
      "```",
      "```action",
      '{"action":"runPiece","pieceRef":"frontend"}',
      "```",
    ].join("\n"));

    expect(review.validationErrors).toEqual([]);
    expect(review.actions).toEqual([
      {
        action: "createPiece",
        ref: "frontend",
        name: "Todo App",
        pieceType: "web-app",
        responsibilities: "Build the todo UI",
        agentPrompt: "Create a minimal todo app in the repo",
        outputMode: "code-only",
        executionEngine: "codex",
      },
      {
        action: "runPiece",
        pieceRef: "frontend",
      },
    ]);
    expect(describeAction(review.actions[1])).toBe('Run piece "frontend"');
  });
});

describe("CTO action execution", () => {
  it("creates a configured piece and runs it immediately", async () => {
    mocks.createPiece.mockResolvedValueOnce({ id: "piece-1", name: "Todo App" });
    mocks.onAgentOutputChunk.mockImplementationOnce(async (callback: (payload: unknown) => void) => {
      mocks.runPieceAgent.mockImplementationOnce(async () => {
        callback({
          pieceId: "piece-1",
          chunk: "",
          done: true,
          success: true,
          usage: { input: 0, output: 0 },
        });
      });
      return () => undefined;
    });

    const result = await executeActions(
      [
        {
          action: "createPiece",
          ref: "frontend",
          name: "Todo App",
          pieceType: "web-app",
          responsibilities: "Build the todo UI",
          agentPrompt: "Create the app files",
          outputMode: "code-only",
          executionEngine: "codex",
        },
        {
          action: "runPiece",
          pieceRef: "frontend",
        },
      ],
      "project-1",
    );

    expect(mocks.createPiece).toHaveBeenCalledWith(
      "project-1",
      null,
      "Todo App",
      expect.any(Number),
      expect.any(Number),
      {
        pieceType: "web-app",
        responsibilities: "Build the todo UI",
        agentPrompt: "Create the app files",
        outputMode: "code-only",
        agentConfig: {
          executionEngine: "codex",
        },
      },
    );
    expect(mocks.runPieceAgent).toHaveBeenCalledWith("piece-1", undefined);
    expect(result.executed).toBe(2);
    expect(result.errors).toEqual([]);
  });

  it("routes generatePlan through the leader store for the active project", async () => {
    mocks.generateWorkPlan.mockResolvedValue(undefined);
    mocks.leaderGeneratePlan.mockResolvedValue(undefined);

    const result = await executeActions(
      [
        {
          action: "generatePlan",
          guidance: "Develop the todo web app",
        },
      ],
      "project-1",
    );

    expect(mocks.leaderGeneratePlan).toHaveBeenCalledTimes(1);
    expect(mocks.leaderGeneratePlan).toHaveBeenCalledWith(
      "project-1",
      "Develop the todo web app",
    );
    expect(mocks.generateWorkPlan).not.toHaveBeenCalled();
    expect(result.executed).toBe(1);
    expect(result.errors).toEqual([]);
    expect(result.switchToTab).toBe("plan");
    expect(result.reloadCurrentProject).toBe(false);
    expect(result.steps).toHaveLength(1);
    expect(result.steps[0]).toMatchObject({
      index: 0,
      action: "generatePlan",
      description: 'Generate work plan: "Develop the todo web app"',
      status: "executed",
    });
    expect(result.rollback.supported).toBe(false);
    expect(result.rollback.steps).toHaveLength(1);
  });
});
