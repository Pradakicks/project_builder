import { useProjectStore } from "../store/useProjectStore";
import { useLeaderStore } from "../store/useLeaderStore";
import { useAppStore } from "../store/useAppStore";
import { devLog } from "./devLog";
import * as api from "../api/tauriApi";
import type {
  CtoAction,
  CtoActionExecutionResult,
  CtoActionReview,
  CtoActionName,
} from "../types";

const supportedActions = new Set<CtoActionName>([
  "updatePiece",
  "createPiece",
  "createConnection",
  "updateConnection",
  "generatePlan",
  "approvePlan",
  "rejectPlan",
  "runAllTasks",
  "mergeBranches",
]);

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function readString(
  value: unknown,
  field: string,
  actionIndex: number,
): string {
  if (typeof value !== "string") {
    throw new Error(`Action ${actionIndex + 1}: "${field}" must be a string`);
  }
  const trimmed = value.trim();
  if (!trimmed) {
    throw new Error(`Action ${actionIndex + 1}: "${field}" cannot be empty`);
  }
  return trimmed;
}

function readOptionalString(value: unknown): string | undefined {
  if (value === undefined || value === null) return undefined;
  if (typeof value !== "string") {
    throw new Error("Optional string fields must be strings");
  }
  const trimmed = value.trim();
  return trimmed || undefined;
}

function readOptionalPlainObject(
  value: unknown,
  field: string,
  actionIndex: number,
): Record<string, unknown> | undefined {
  if (value === undefined || value === null) return undefined;
  if (!isPlainObject(value)) {
    throw new Error(`Action ${actionIndex + 1}: "${field}" must be an object`);
  }
  return { ...value };
}

function ensureAllowedKeys(
  raw: Record<string, unknown>,
  allowedKeys: string[],
  actionIndex: number,
): void {
  const extras = Object.keys(raw).filter((key) => !allowedKeys.includes(key));
  if (extras.length > 0) {
    throw new Error(
      `Action ${actionIndex + 1}: unsupported field(s): ${extras.join(", ")}`,
    );
  }
}

function buildActionError(actionIndex: number, actionName: string, reason: string): string {
  return `Action ${actionIndex + 1} (${actionName}): ${reason}`;
}

async function resolvePieceReference(
  projectId: string,
  reference: string | undefined,
  createdPieceRefs: Map<string, string>,
): Promise<string> {
  const trimmed = reference?.trim();
  if (!trimmed) {
    throw new Error("piece reference is required");
  }

  const createdPieceId = createdPieceRefs.get(trimmed);
  if (createdPieceId) {
    return createdPieceId;
  }

  const pieces = await api.listPieces(projectId);
  if (pieces.some((piece) => piece.id === trimmed)) {
    return trimmed;
  }

  const exactNameMatches = pieces.filter((piece) => piece.name === trimmed);
  if (exactNameMatches.length === 1) {
    return exactNameMatches[0].id;
  }

  if (exactNameMatches.length > 1) {
    throw new Error(`Ambiguous piece reference: ${trimmed}`);
  }

  throw new Error(`Piece reference not found: ${trimmed}`);
}

function normalizeAction(
  raw: unknown,
  actionIndex: number,
): { action?: CtoAction; error?: string } {
  if (!isPlainObject(raw)) {
    return {
      error: `Action ${actionIndex + 1}: expected a JSON object`,
    };
  }

  let actionName: string;
  try {
    actionName = readString(raw.action, "action", actionIndex);
  } catch (error) {
    return {
      error:
        error instanceof Error
          ? error.message
          : `Action ${actionIndex + 1}: invalid action field`,
    };
  }
  if (!supportedActions.has(actionName as CtoActionName)) {
    return {
      error: `Action ${actionIndex + 1}: unsupported action "${actionName}"`,
    };
  }

  try {
    switch (actionName as CtoActionName) {
      case "updatePiece": {
        ensureAllowedKeys(raw, ["action", "pieceId", "updates"], actionIndex);
        const pieceId = readString(raw.pieceId, "pieceId", actionIndex);
        const updates = readOptionalPlainObject(raw.updates, "updates", actionIndex);
        if (!updates || Object.keys(updates).length === 0) {
          throw new Error("updates must contain at least one field");
        }
        return { action: { action: "updatePiece", pieceId, updates } };
      }
      case "createPiece": {
        ensureAllowedKeys(
          raw,
          ["action", "name", "ref", "pieceType", "responsibilities"],
          actionIndex,
        );
        const name = readString(raw.name, "name", actionIndex);
        const normalized: CtoAction = {
          action: "createPiece",
          name,
        };
        const ref = readOptionalString(raw.ref);
        if (ref) normalized.ref = ref;
        const pieceType = readOptionalString(raw.pieceType);
        if (pieceType) normalized.pieceType = pieceType;
        const responsibilities = readOptionalString(raw.responsibilities);
        if (responsibilities) normalized.responsibilities = responsibilities;
        return { action: normalized };
      }
      case "createConnection": {
        ensureAllowedKeys(
          raw,
          [
            "action",
            "sourceRef",
            "sourcePieceId",
            "targetRef",
            "targetPieceId",
            "label",
          ],
          actionIndex,
        );
        const sourceRef =
          readOptionalString(raw.sourceRef) ?? readOptionalString(raw.sourcePieceId);
        const targetRef =
          readOptionalString(raw.targetRef) ?? readOptionalString(raw.targetPieceId);
        if (!sourceRef) {
          throw new Error("sourceRef or sourcePieceId is required");
        }
        if (!targetRef) {
          throw new Error("targetRef or targetPieceId is required");
        }
        const normalized: CtoAction = {
          action: "createConnection",
          sourceRef,
          targetRef,
        };
        const label = readOptionalString(raw.label);
        if (label) normalized.label = label;
        return { action: normalized };
      }
      case "updateConnection": {
        ensureAllowedKeys(raw, ["action", "connectionId", "updates"], actionIndex);
        const connectionId = readString(raw.connectionId, "connectionId", actionIndex);
        const updates = readOptionalPlainObject(raw.updates, "updates", actionIndex);
        if (!updates || Object.keys(updates).length === 0) {
          throw new Error("updates must contain at least one field");
        }
        return { action: { action: "updateConnection", connectionId, updates } };
      }
      case "generatePlan": {
        ensureAllowedKeys(raw, ["action", "guidance"], actionIndex);
        const guidance = readString(raw.guidance, "guidance", actionIndex);
        return { action: { action: "generatePlan", guidance } };
      }
      case "approvePlan": {
        ensureAllowedKeys(raw, ["action", "planId"], actionIndex);
        const planId = readString(raw.planId, "planId", actionIndex);
        return { action: { action: "approvePlan", planId } };
      }
      case "rejectPlan": {
        ensureAllowedKeys(raw, ["action", "planId"], actionIndex);
        const planId = readString(raw.planId, "planId", actionIndex);
        return { action: { action: "rejectPlan", planId } };
      }
      case "runAllTasks": {
        ensureAllowedKeys(raw, ["action", "planId"], actionIndex);
        const planId = readString(raw.planId, "planId", actionIndex);
        return { action: { action: "runAllTasks", planId } };
      }
      case "mergeBranches": {
        ensureAllowedKeys(raw, ["action", "planId"], actionIndex);
        const planId = readString(raw.planId, "planId", actionIndex);
        return { action: { action: "mergeBranches", planId } };
      }
      default:
        return {
          error: buildActionError(
            actionIndex,
            actionName,
            "unsupported action",
          ),
        };
    }
  } catch (error) {
    return {
      error:
        error instanceof Error
          ? buildActionError(actionIndex, actionName, error.message)
          : buildActionError(actionIndex, actionName, String(error)),
    };
  }
}

/** Remove action blocks from display text */
export function stripActionBlocks(markdown: string): string {
  return markdown.replace(/```action\s*\n[\s\S]*?\n```/g, "").trim();
}

/** Extract, validate, and normalize CTO action blocks from assistant markdown. */
export function reviewActions(markdown: string): CtoActionReview {
  const actions: CtoAction[] = [];
  const validationErrors: string[] = [];
  const regex = /```action\s*\n([\s\S]*?)\n```/g;
  let match: RegExpExecArray | null;
  let actionIndex = 0;

  while ((match = regex.exec(markdown)) !== null) {
    actionIndex += 1;
    try {
      const parsed = JSON.parse(match[1]) as unknown;
      const normalized = normalizeAction(parsed, actionIndex);
      if (normalized.error) {
        validationErrors.push(normalized.error);
        continue;
      }
      if (normalized.action) {
        actions.push(normalized.action);
      }
    } catch (error) {
      validationErrors.push(
        error instanceof Error
          ? `Action ${actionIndex}: invalid JSON (${error.message})`
          : `Action ${actionIndex}: invalid JSON`,
      );
      devLog("warn", "CTO", `Failed to parse action block JSON`, error);
    }
  }

  return {
    actions,
    cleanedContent: stripActionBlocks(markdown),
    validationErrors,
  };
}

/** Backwards-compatible helper that returns only validated actions. */
export function parseActions(markdown: string): CtoAction[] {
  return reviewActions(markdown).actions;
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
      return `Unknown action: ${(action as { action: string }).action}`;
  }
}

/** Execute parsed CTO actions against the project store */
export async function executeActions(
  actions: CtoAction[],
  projectId: string,
): Promise<CtoActionExecutionResult> {
  let executed = 0;
  const errors: string[] = [];
  const steps: CtoActionExecutionResult["steps"] = [];
  let switchToTab: string | undefined;
  let reloadCurrentProject = false;
  const createdPieceRefs = new Map<string, string>();
  const isActiveProject =
    useAppStore.getState().activeProjectId === projectId &&
    useProjectStore.getState().project?.id === projectId;

  devLog("info", "CTO", `Executing ${actions.length} actions`, actions.map((a) => a.action));
  for (const [index, action] of actions.entries()) {
    const description = describeAction(action);
    try {
      switch (action.action) {
        case "updatePiece": {
          const updates = action.updates as Record<string, unknown>;
          await api.updatePiece(action.pieceId as string, updates);
          executed += 1;
          reloadCurrentProject = true;
          steps.push({
            index,
            action: action.action,
            description,
            status: "executed",
          });
          break;
        }
        case "createPiece": {
          const randomX = 200 + Math.random() * 400;
          const randomY = 150 + Math.random() * 300;
          const piece = await api.createPiece(
            projectId,
            null,
            (action.name as string) || "New Component",
            randomX,
            randomY,
          );
          if (typeof action.ref === "string" && action.ref.trim()) {
            createdPieceRefs.set(action.ref.trim(), piece.id);
          }
          const extraUpdates: Record<string, unknown> = {};
          if (action.pieceType) extraUpdates.pieceType = action.pieceType;
          if (action.responsibilities) extraUpdates.responsibilities = action.responsibilities;
          if (Object.keys(extraUpdates).length > 0) {
            await api.updatePiece(piece.id, extraUpdates);
          }
          executed += 1;
          reloadCurrentProject = true;
          steps.push({
            index,
            action: action.action,
            description,
            status: "executed",
          });
          break;
        }
        case "createConnection": {
          const sourcePieceId = await resolvePieceReference(
            projectId,
            (action.sourceRef as string | undefined) ??
              (action.sourcePieceId as string | undefined),
            createdPieceRefs,
          );
          const targetPieceId = await resolvePieceReference(
            projectId,
            (action.targetRef as string | undefined) ??
              (action.targetPieceId as string | undefined),
            createdPieceRefs,
          );

          await api.createConnection(
            projectId,
            sourcePieceId,
            targetPieceId,
            (action.label as string) || "",
          );
          executed += 1;
          reloadCurrentProject = true;
          steps.push({
            index,
            action: action.action,
            description,
            status: "executed",
          });
          break;
        }
        case "updateConnection": {
          const updates = action.updates as Record<string, unknown>;
          await api.updateConnection(action.connectionId as string, updates);
          executed += 1;
          reloadCurrentProject = true;
          steps.push({
            index,
            action: action.action,
            description,
            status: "executed",
          });
          break;
        }
        case "generatePlan": {
          if (isActiveProject) {
            await useLeaderStore
              .getState()
              .generatePlan(projectId, (action.guidance as string) || "");
          } else {
            await api.generateWorkPlan(projectId, (action.guidance as string) || "");
          }
          switchToTab = "plan";
          executed += 1;
          steps.push({
            index,
            action: action.action,
            description,
            status: "executed",
          });
          break;
        }
        case "approvePlan": {
          if (isActiveProject) {
            await useLeaderStore.getState().approvePlan(action.planId as string);
          } else {
            await api.updatePlanStatus(action.planId as string, "approved");
          }
          switchToTab = "plan";
          executed += 1;
          steps.push({
            index,
            action: action.action,
            description,
            status: "executed",
          });
          break;
        }
        case "rejectPlan": {
          if (isActiveProject) {
            await useLeaderStore.getState().rejectPlan(action.planId as string);
          } else {
            await api.updatePlanStatus(action.planId as string, "rejected");
          }
          switchToTab = "plan";
          executed += 1;
          steps.push({
            index,
            action: action.action,
            description,
            status: "executed",
          });
          break;
        }
        case "runAllTasks": {
          if (isActiveProject) {
            await useLeaderStore.getState().runAllTasks(action.planId as string);
          } else {
            await api.runAllPlanTasks(action.planId as string);
          }
          switchToTab = "plan";
          executed += 1;
          steps.push({
            index,
            action: action.action,
            description,
            status: "executed",
          });
          break;
        }
        case "mergeBranches": {
          if (isActiveProject) {
            await useLeaderStore.getState().mergeBranches(action.planId as string);
          } else {
            const summary = await api.mergePlanBranches(action.planId as string);
            if (!summary.conflict) {
              await api.runIntegrationReview(action.planId as string);
            }
          }
          switchToTab = "plan";
          executed += 1;
          steps.push({
            index,
            action: action.action,
            description,
            status: "executed",
          });
          break;
        }
        default: {
          const message = `Unknown action: ${(action as { action: string }).action}`;
          errors.push(message);
          steps.push({
            index,
            action: action.action,
            description,
            status: "failed",
            error: message,
          });
        }
      }
    } catch (error) {
      devLog("error", "CTO", `Action "${action.action}" failed`, error);
      const message = `${action.action} failed: ${error}`;
      errors.push(message);
      steps.push({
        index,
        action: action.action,
        description,
        status: "failed",
        error: message,
      });
    }
  }

  devLog("info", "CTO", `Executed ${executed}/${actions.length} actions`, { errors });
  return { executed, errors, steps, switchToTab, reloadCurrentProject };
}
