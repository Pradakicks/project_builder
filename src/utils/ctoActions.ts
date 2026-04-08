import { useProjectStore } from "../store/useProjectStore";
import { useLeaderStore } from "../store/useLeaderStore";
import { devLog } from "./devLog";

export interface CtoAction {
  action: string;
  [key: string]: unknown;
}

/** Extract action blocks from CTO markdown response */
export function parseActions(markdown: string): CtoAction[] {
  const actions: CtoAction[] = [];
  const regex = /```action\s*\n([\s\S]*?)\n```/g;
  let match;
  while ((match = regex.exec(markdown)) !== null) {
    try {
      const parsed = JSON.parse(match[1]);
      if (parsed && typeof parsed.action === "string") {
        actions.push(parsed);
      }
    } catch (e) {
      devLog("warn", "CTO", `Failed to parse action block JSON`, e);
    }
  }
  return actions;
}

/** Remove action blocks from display text */
export function stripActionBlocks(markdown: string): string {
  return markdown.replace(/```action\s*\n[\s\S]*?\n```/g, "").trim();
}

/** Describe an action in human-readable form */
export function describeAction(action: CtoAction): string {
  switch (action.action) {
    case "updatePiece": {
      const updates = action.updates as Record<string, unknown> | undefined;
      const fields = updates ? Object.keys(updates).join(", ") : "fields";
      return `Update "${action.pieceId}" (${fields})`;
    }
    case "createPiece":
      return `Create "${action.name}"`;
    case "createConnection":
      return `Connect pieces`;
    case "updateConnection":
      return `Update connection`;
    case "generatePlan": {
      const guidance = action.guidance as string | undefined;
      return guidance ? `Generate work plan: "${guidance.slice(0, 60)}"` : "Generate work plan";
    }
    case "approvePlan":
      return "Approve plan";
    case "rejectPlan":
      return "Reject plan";
    case "runAllTasks":
      return "Run all plan tasks";
    case "mergeBranches":
      return "Merge all piece branches to main";
    default:
      return `Unknown action: ${action.action}`;
  }
}

/** Execute parsed CTO actions against the project store */
export async function executeActions(
  actions: CtoAction[],
  projectId: string,
): Promise<{ executed: number; errors: string[]; switchToTab?: string }> {
  const store = useProjectStore.getState();
  let executed = 0;
  const errors: string[] = [];
  let switchToTab: string | undefined;

  devLog("info", "CTO", `Executing ${actions.length} actions`, actions.map(a => a.action));
  for (const action of actions) {
    try {
      switch (action.action) {
        case "updatePiece": {
          const updates = action.updates as Record<string, unknown>;
          await store.updatePiece(action.pieceId as string, updates);
          executed++;
          break;
        }
        case "createPiece": {
          const randomX = 200 + Math.random() * 400;
          const randomY = 150 + Math.random() * 300;
          const piece = await store.addPiece(
            (action.name as string) || "New Component",
            randomX,
            randomY,
          );
          // Apply additional fields
          const extraUpdates: Record<string, unknown> = {};
          if (action.pieceType) extraUpdates.pieceType = action.pieceType;
          if (action.responsibilities) extraUpdates.responsibilities = action.responsibilities;
          if (Object.keys(extraUpdates).length > 0) {
            await store.updatePiece(piece.id, extraUpdates);
          }
          executed++;
          break;
        }
        case "createConnection": {
          await store.addConnection(
            action.sourcePieceId as string,
            action.targetPieceId as string,
            (action.label as string) || "",
          );
          executed++;
          break;
        }
        case "updateConnection": {
          const updates = action.updates as Record<string, unknown>;
          await store.updateConnection(action.connectionId as string, updates);
          executed++;
          break;
        }
        case "generatePlan": {
          useLeaderStore.getState().generatePlan(
            projectId,
            (action.guidance as string) || "",
          );
          switchToTab = "plan";
          executed++;
          break;
        }
        case "approvePlan": {
          await useLeaderStore.getState().approvePlan(action.planId as string);
          switchToTab = "plan";
          executed++;
          break;
        }
        case "rejectPlan": {
          await useLeaderStore.getState().rejectPlan(action.planId as string);
          switchToTab = "plan";
          executed++;
          break;
        }
        case "runAllTasks": {
          useLeaderStore.getState().runAllTasks(action.planId as string);
          switchToTab = "plan";
          executed++;
          break;
        }
        case "mergeBranches": {
          useLeaderStore.getState().mergeBranches(action.planId as string);
          switchToTab = "plan";
          executed++;
          break;
        }
        default:
          errors.push(`Unknown action: ${action.action}`);
      }
    } catch (e) {
      devLog("error", "CTO", `Action "${action.action}" failed`, e);
      errors.push(`${action.action} failed: ${e}`);
    }
  }

  devLog("info", "CTO", `Executed ${executed}/${actions.length} actions`, { errors });
  return { executed, errors, switchToTab };
}
