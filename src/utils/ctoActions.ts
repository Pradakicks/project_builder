import { useProjectStore } from "../store/useProjectStore";
import { useLeaderStore } from "../store/useLeaderStore";
import { useAgentStore } from "../store/useAgentStore";
import { useAppStore } from "../store/useAppStore";
import { useGoalRunStore } from "../store/useGoalRunStore";
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
  "runPiece",
  "createConnection",
  "updateConnection",
  "generatePlan",
  "approvePlan",
  "rejectPlan",
  "runAllTasks",
  "mergeBranches",
  "configureRuntime",
  "runProject",
  "stopProject",
  "retryGoalStep",
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

interface ActionBlockCandidate {
  start: number;
  end: number;
  raw: string;
}

function extractBalancedJsonObject(source: string, openBraceIndex: number): string | null {
  if (source[openBraceIndex] !== "{") {
    return null;
  }

  let depth = 0;
  let inString = false;
  let escaped = false;

  for (let index = openBraceIndex; index < source.length; index += 1) {
    const char = source[index];

    if (inString) {
      if (escaped) {
        escaped = false;
      } else if (char === "\\") {
        escaped = true;
      } else if (char === '"') {
        inString = false;
      }
      continue;
    }

    if (char === '"') {
      inString = true;
      continue;
    }

    if (char === "{") {
      depth += 1;
      continue;
    }

    if (char === "}") {
      depth -= 1;
      if (depth === 0) {
        return source.slice(openBraceIndex, index + 1);
      }
    }
  }

  return null;
}

function collectActionBlocks(markdown: string): ActionBlockCandidate[] {
  const candidates: ActionBlockCandidate[] = [];
  const fencedRanges: Array<{ start: number; end: number }> = [];

  const fencedRegex = /```action\s*\n([\s\S]*?)\n```/g;
  let match: RegExpExecArray | null;
  while ((match = fencedRegex.exec(markdown)) !== null) {
    fencedRanges.push({ start: match.index, end: match.index + match[0].length });
    candidates.push({
      start: match.index,
      end: match.index + match[0].length,
      raw: match[1].trim(),
    });
  }

  const inlineRegex = /\baction\s*\{/g;
  while ((match = inlineRegex.exec(markdown)) !== null) {
    const braceIndex = match.index + match[0].lastIndexOf("{");
    const insideFence = fencedRanges.some(
      (range) => braceIndex >= range.start && braceIndex < range.end,
    );
    if (insideFence) {
      continue;
    }

    const raw = extractBalancedJsonObject(markdown, braceIndex);
    if (!raw) {
      continue;
    }

    candidates.push({
      start: match.index,
      end: braceIndex + raw.length,
      raw,
    });
  }

  return candidates.sort((a, b) => a.start - b.start);
}

async function loadLeaderApi() {
  return import("../api/leaderApi");
}

async function executePieceRun(
  pieceId: string,
  feedback?: string,
): Promise<void> {
  const leaderApi = await loadLeaderApi();
  const agentStore = useAgentStore.getState();
  if (feedback?.trim()) {
    agentStore.startFeedbackRun(pieceId);
  } else {
    agentStore.startRun(pieceId);
  }

  const unlisten = await leaderApi.onAgentOutputChunk((payload) => {
    if (payload.pieceId !== pieceId) return;
    const store = useAgentStore.getState();
    if (payload.done) {
      store.completeRun(pieceId, {
        usage: payload.usage ?? { input: 0, output: 0 },
        success: payload.success ?? (payload.exitCode ?? 0) === 0,
        exitCode: payload.exitCode,
        phaseProposal: payload.phaseProposal,
        phaseChanged: payload.phaseChanged,
        gitBranch: payload.gitBranch,
        gitCommitSha: payload.gitCommitSha,
        gitDiffStat: payload.gitDiffStat,
        validation: payload.validation,
      });
      unlisten();
      return;
    }

    if (payload.streamKind === "validation") {
      store.appendValidationChunk(pieceId, payload.chunk);
    } else {
      store.appendChunk(pieceId, payload.chunk);
    }
  });

  try {
    await leaderApi.runPieceAgent(pieceId, feedback);
  } catch (error) {
    unlisten();
    useAgentStore
      .getState()
      .completeRun(pieceId, { usage: { input: 0, output: 0 } });
    throw error;
  }
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
          [
            "action",
            "name",
            "ref",
            "parentRef",
            "parentPieceId",
            "pieceType",
            "responsibilities",
            "agentPrompt",
            "notes",
            "phase",
            "outputMode",
            "executionEngine",
          ],
          actionIndex,
        );
        const name = readString(raw.name, "name", actionIndex);
        const normalized: CtoAction = {
          action: "createPiece",
          name,
        };
        const ref = readOptionalString(raw.ref);
        if (ref) normalized.ref = ref;
        const parentRef =
          readOptionalString(raw.parentRef) ?? readOptionalString(raw.parentPieceId);
        if (parentRef) normalized.parentRef = parentRef;
        const pieceType = readOptionalString(raw.pieceType);
        if (pieceType) normalized.pieceType = pieceType;
        const responsibilities = readOptionalString(raw.responsibilities);
        if (responsibilities) normalized.responsibilities = responsibilities;
        const agentPrompt = readOptionalString(raw.agentPrompt);
        if (agentPrompt) normalized.agentPrompt = agentPrompt;
        const notes = readOptionalString(raw.notes);
        if (notes) normalized.notes = notes;
        const phase = readOptionalString(raw.phase);
        if (phase) normalized.phase = phase;
        const outputMode = readOptionalString(raw.outputMode);
        if (outputMode) normalized.outputMode = outputMode;
        const executionEngine = readOptionalString(raw.executionEngine);
        if (executionEngine) normalized.executionEngine = executionEngine;
        return { action: normalized };
      }
      case "runPiece": {
        ensureAllowedKeys(raw, ["action", "pieceRef", "pieceId", "feedback"], actionIndex);
        const pieceRef =
          readOptionalString(raw.pieceRef) ?? readOptionalString(raw.pieceId);
        if (!pieceRef) {
          throw new Error("pieceRef or pieceId is required");
        }
        const feedback = readOptionalString(raw.feedback);
        return { action: { action: "runPiece", pieceRef, feedback } };
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
      case "configureRuntime": {
        ensureAllowedKeys(raw, ["action", "spec"], actionIndex);
        const spec = readOptionalPlainObject(raw.spec, "spec", actionIndex);
        if (!spec) {
          throw new Error("spec is required");
        }
        return { action: { action: "configureRuntime", spec } };
      }
      case "runProject":
        ensureAllowedKeys(raw, ["action"], actionIndex);
        return { action: { action: "runProject" } };
      case "stopProject":
        ensureAllowedKeys(raw, ["action"], actionIndex);
        return { action: { action: "stopProject" } };
      case "retryGoalStep": {
        ensureAllowedKeys(raw, ["action", "goalRunId"], actionIndex);
        const goalRunId = readOptionalString(raw.goalRunId);
        return { action: { action: "retryGoalStep", goalRunId } };
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
  const candidates = collectActionBlocks(markdown).slice().sort((a, b) => b.start - a.start);
  let cleaned = markdown;
  for (const candidate of candidates) {
    cleaned = `${cleaned.slice(0, candidate.start)}${cleaned.slice(candidate.end)}`;
  }
  return cleaned.replace(/\n{3,}/g, "\n\n").trim();
}

/** Extract, validate, and normalize CTO action blocks from assistant markdown. */
export function reviewActions(markdown: string): CtoActionReview {
  const actions: CtoAction[] = [];
  const validationErrors: string[] = [];

  for (const [actionIndex, candidate] of collectActionBlocks(markdown).entries()) {
    try {
      const parsed = JSON.parse(candidate.raw) as unknown;
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
          ? `Action ${actionIndex + 1}: invalid JSON (${error.message})`
          : `Action ${actionIndex + 1}: invalid JSON`,
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
    case "runPiece":
      return typeof action.pieceRef === "string"
        ? `Run piece "${action.pieceRef}"`
        : "Run piece";
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
    case "configureRuntime":
      return "Configure project runtime";
    case "runProject":
      return "Run project runtime";
    case "stopProject":
      return "Stop project runtime";
    case "retryGoalStep":
      return "Retry goal run";
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
  const rollbackSteps: CtoActionExecutionResult["rollback"]["steps"] = [];
  let switchToTab: string | undefined;
  let reloadCurrentProject = false;
  const createdPieceRefs = new Map<string, string>();
  const isActiveProject =
    useAppStore.getState().activeProjectId === projectId &&
    useProjectStore.getState().project?.id === projectId;

  devLog("info", "CTO", `Executing ${actions.length} actions`, actions.map((a) => a.action));
  for (const [index, action] of actions.entries()) {
    const description = describeAction(action);
    let rollbackStep: CtoActionExecutionResult["steps"][number]["rollback"] = null;
    try {
      switch (action.action) {
        case "updatePiece": {
          const previousPiece = await api.getPiece(action.pieceId as string);
          const updates = action.updates as Record<string, unknown>;
          await api.updatePiece(action.pieceId as string, updates);
          rollbackStep = {
            index,
            action: action.action,
            description,
            supported: true,
            kind: { kind: "restorePiece", piece: previousPiece },
          };
          executed += 1;
          reloadCurrentProject = true;
          steps.push({
            index,
            action: action.action,
            description,
            status: "executed",
            rollback: rollbackStep,
          });
          rollbackSteps.push(rollbackStep);
          break;
        }
        case "createPiece": {
          const randomX = 200 + Math.random() * 400;
          const randomY = 150 + Math.random() * 300;
          const parentId = await resolvePieceReference(
            projectId,
            action.parentRef as string | undefined,
            createdPieceRefs,
          ).catch((error) => {
            if (action.parentRef) throw error;
            return null;
          });
          const initialUpdates: Record<string, unknown> = {};
          if (action.pieceType) initialUpdates.pieceType = action.pieceType;
          if (action.responsibilities) initialUpdates.responsibilities = action.responsibilities;
          if (action.agentPrompt) initialUpdates.agentPrompt = action.agentPrompt;
          if (action.notes) initialUpdates.notes = action.notes;
          if (action.phase) initialUpdates.phase = action.phase;
          if (action.outputMode) initialUpdates.outputMode = action.outputMode;
          if (action.executionEngine) {
            initialUpdates.agentConfig = {
              executionEngine: action.executionEngine,
            };
          }
          const piece = await api.createPiece(
            projectId,
            parentId,
            (action.name as string) || "New Component",
            randomX,
            randomY,
            Object.keys(initialUpdates).length > 0 ? initialUpdates : null,
          );
          if (typeof action.ref === "string" && action.ref.trim()) {
            createdPieceRefs.set(action.ref.trim(), piece.id);
          }
          rollbackStep = {
            index,
            action: action.action,
            description,
            supported: true,
            kind: { kind: "deletePiece", pieceId: piece.id },
          };
          executed += 1;
          reloadCurrentProject = true;
          steps.push({
            index,
            action: action.action,
            description,
            status: "executed",
            rollback: rollbackStep,
          });
          rollbackSteps.push(rollbackStep);
          break;
        }
        case "runPiece": {
          rollbackStep = {
            index,
            action: action.action,
            description,
            supported: false,
            reason: "Piece execution changes workspace state and is not rollback-safe",
          };
          const pieceId = await resolvePieceReference(
            projectId,
            action.pieceRef as string | undefined,
            createdPieceRefs,
          );
          // Ensure the piece is in implementing phase before running. Pieces default
          // to Design phase, which sends "Do NOT write code" instructions to the agent.
          // When the CTO calls runPiece, the intent is always to produce code.
          await api.updatePiece(pieceId, { phase: "implementing" });
          await executePieceRun(pieceId, action.feedback as string | undefined);
          executed += 1;
          reloadCurrentProject = true;
          steps.push({
            index,
            action: action.action,
            description,
            status: "executed",
            rollback: rollbackStep,
          });
          rollbackSteps.push(rollbackStep);
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

          const connection = await api.createConnection(
            projectId,
            sourcePieceId,
            targetPieceId,
            (action.label as string) || "",
          );
          rollbackStep = {
            index,
            action: action.action,
            description,
            supported: true,
            kind: { kind: "deleteConnection", connectionId: connection.id },
          };
          executed += 1;
          reloadCurrentProject = true;
          steps.push({
            index,
            action: action.action,
            description,
            status: "executed",
            rollback: rollbackStep,
          });
          rollbackSteps.push(rollbackStep);
          break;
        }
        case "updateConnection": {
          const previousConnection = await api.getConnection(action.connectionId as string);
          const updates = action.updates as Record<string, unknown>;
          await api.updateConnection(action.connectionId as string, updates);
          rollbackStep = {
            index,
            action: action.action,
            description,
            supported: true,
            kind: { kind: "restoreConnection", connection: previousConnection },
          };
          executed += 1;
          reloadCurrentProject = true;
          steps.push({
            index,
            action: action.action,
            description,
            status: "executed",
            rollback: rollbackStep,
          });
          rollbackSteps.push(rollbackStep);
          break;
        }
        case "generatePlan": {
          rollbackStep = {
            index,
            action: action.action,
            description,
            supported: false,
            reason: "Generated plans are not rollback-safe yet",
          };
          if (isActiveProject) {
            await useLeaderStore
              .getState()
              .generatePlan(projectId, (action.guidance as string) || "");
          } else {
            const leaderApi = await loadLeaderApi();
            await leaderApi.generateWorkPlan(
              projectId,
              (action.guidance as string) || "",
            );
          }
          switchToTab = "plan";
          executed += 1;
          steps.push({
            index,
            action: action.action,
            description,
            status: "executed",
            rollback: rollbackStep,
          });
          rollbackSteps.push(rollbackStep);
          break;
        }
        case "approvePlan": {
          const leaderApi = (await loadLeaderApi()) as any;
          const previousPlan = await leaderApi.getWorkPlan(action.planId as string);
          if (isActiveProject) {
            await useLeaderStore.getState().approvePlan(action.planId as string);
          } else {
            await leaderApi.updatePlanStatus(
              action.planId as string,
              "approved",
            );
          }
          rollbackStep = {
            index,
            action: action.action,
            description,
            supported: true,
            kind: {
              kind: "restorePlanStatus",
              planId: previousPlan.id,
              status: previousPlan.status,
            },
          };
          switchToTab = "plan";
          executed += 1;
          steps.push({
            index,
            action: action.action,
            description,
            status: "executed",
            rollback: rollbackStep,
          });
          rollbackSteps.push(rollbackStep);
          break;
        }
        case "rejectPlan": {
          const leaderApi = (await loadLeaderApi()) as any;
          const previousPlan = await leaderApi.getWorkPlan(action.planId as string);
          if (isActiveProject) {
            await useLeaderStore.getState().rejectPlan(action.planId as string);
          } else {
            await leaderApi.updatePlanStatus(
              action.planId as string,
              "rejected",
            );
          }
          rollbackStep = {
            index,
            action: action.action,
            description,
            supported: true,
            kind: {
              kind: "restorePlanStatus",
              planId: previousPlan.id,
              status: previousPlan.status,
            },
          };
          switchToTab = "plan";
          executed += 1;
          steps.push({
            index,
            action: action.action,
            description,
            status: "executed",
            rollback: rollbackStep,
          });
          rollbackSteps.push(rollbackStep);
          break;
        }
        case "runAllTasks": {
          rollbackStep = {
            index,
            action: action.action,
            description,
            supported: false,
            reason: "Task execution changes workspace state and is not rollback-safe",
          };
          if (isActiveProject) {
            await useLeaderStore.getState().runAllTasks(action.planId as string);
          } else {
            const leaderApi = await loadLeaderApi();
            await leaderApi.runAllPlanTasks(action.planId as string);
          }
          switchToTab = "plan";
          executed += 1;
          steps.push({
            index,
            action: action.action,
            description,
            status: "executed",
            rollback: rollbackStep,
          });
          rollbackSteps.push(rollbackStep);
          break;
        }
        case "mergeBranches": {
          rollbackStep = {
            index,
            action: action.action,
            description,
            supported: false,
            reason: "Git merges are not rollback-safe from the audit log",
          };
          if (isActiveProject) {
            await useLeaderStore.getState().mergeBranches(action.planId as string);
          } else {
            const leaderApi = await loadLeaderApi();
            const summary = await leaderApi.mergePlanBranches(action.planId as string);
            if (!summary.conflict) {
              await leaderApi.runIntegrationReview(action.planId as string);
            }
          }
          switchToTab = "plan";
          executed += 1;
          steps.push({
            index,
            action: action.action,
            description,
            status: "executed",
            rollback: rollbackStep,
          });
          rollbackSteps.push(rollbackStep);
          break;
        }
        case "configureRuntime": {
          rollbackStep = {
            index,
            action: action.action,
            description,
            supported: false,
            reason: "Runtime configuration rollback is not implemented",
          };
          await api.configureRuntime(projectId, action.spec as any);
          executed += 1;
          steps.push({
            index,
            action: action.action,
            description,
            status: "executed",
            rollback: rollbackStep,
          });
          rollbackSteps.push(rollbackStep);
          break;
        }
        case "runProject": {
          rollbackStep = {
            index,
            action: action.action,
            description,
            supported: false,
            reason: "Runtime process control is not rollback-safe",
          };
          await useGoalRunStore.getState().startRuntime(projectId);
          executed += 1;
          switchToTab = "plan";
          steps.push({
            index,
            action: action.action,
            description,
            status: "executed",
            rollback: rollbackStep,
          });
          rollbackSteps.push(rollbackStep);
          break;
        }
        case "stopProject": {
          rollbackStep = {
            index,
            action: action.action,
            description,
            supported: false,
            reason: "Runtime stop is not rollback-safe",
          };
          await useGoalRunStore.getState().stopRuntime(projectId);
          executed += 1;
          switchToTab = "plan";
          steps.push({
            index,
            action: action.action,
            description,
            status: "executed",
            rollback: rollbackStep,
          });
          rollbackSteps.push(rollbackStep);
          break;
        }
        case "retryGoalStep": {
          rollbackStep = {
            index,
            action: action.action,
            description,
            supported: false,
            reason: "Goal-run retries are not rollback-safe",
          };
          const currentGoalRun =
            useGoalRunStore.getState().currentGoalRun;
          const goalRunId =
            (action.goalRunId as string | undefined) ?? currentGoalRun?.id;
          if (!goalRunId) {
            throw new Error("No goal run is available to retry");
          }
          await useGoalRunStore.getState().retryGoalRun(goalRunId);
          executed += 1;
          switchToTab = "plan";
          steps.push({
            index,
            action: action.action,
            description,
            status: "executed",
            rollback: rollbackStep,
          });
          rollbackSteps.push(rollbackStep);
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
            rollback: {
              index,
              action: action.action,
              description,
              supported: false,
              reason: message,
            },
          });
          rollbackSteps.push({
            index,
            action: action.action,
            description,
            supported: false,
            reason: message,
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
        rollback: rollbackStep ?? {
          index,
          action: action.action,
          description,
          supported: false,
          reason: message,
        },
      });
      rollbackSteps.push(
        rollbackStep ?? {
          index,
          action: action.action,
          description,
          supported: false,
          reason: message,
        },
      );
    }
  }

  const rollbackSupported = errors.length === 0 && rollbackSteps.every((step) => step.supported);
  const rollbackReason = rollbackSupported
    ? null
    : errors.length > 0
      ? "One or more CTO actions failed during execution."
      : "This decision includes non-reversible action(s).";

  devLog("info", "CTO", `Executed ${executed}/${actions.length} actions`, { errors });
  return {
    executed,
    errors,
    steps,
    switchToTab,
    reloadCurrentProject,
    rollback: {
      supported: rollbackSupported,
      reason: rollbackReason,
      steps: rollbackSteps,
    },
  };
}
