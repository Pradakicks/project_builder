import { describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  generateWorkPlan: vi.fn(),
  leaderGeneratePlan: vi.fn(),
  appState: {
    activeProjectId: "project-1",
  },
  projectState: {
    project: { id: "project-1" } as { id: string } | null,
  },
}));

vi.mock("../api/projectApi", () => ({
  listPieces: vi.fn(),
}));

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

import { describeAction, executeActions, reviewActions } from "./ctoActions";

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
});

describe("CTO action execution", () => {
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
