import type {
  CtoDecision,
  CtoDecisionRecordInput,
  CtoDecisionReview,
  CtoActionExecutionMode,
  CtoDecisionExecution,
} from "../types";
import { loggedInvoke, listenToEvent } from "./runtime";

export interface LlmMessage {
  role: string;
  content: string;
}

export interface CtoChatChunk {
  projectId: string;
  requestId: string;
  chunk: string;
  done: boolean;
  usage?: { input: number; output: number };
}

export async function chatWithCto(
  projectId: string,
  userMessage: string,
  conversation: LlmMessage[],
  requestId: string,
): Promise<void> {
  return loggedInvoke("chat_with_cto", {
    projectId,
    userMessage,
    conversation,
    requestId,
  });
}

export async function reviewCtoActions(
  assistantText: string,
): Promise<CtoDecisionReview> {
  return loggedInvoke("review_cto_actions", { assistantText });
}

export async function executeCtoActions(
  projectId: string,
  review: CtoDecisionReview,
  executionMode: CtoActionExecutionMode = "manual-review",
): Promise<CtoDecisionExecution> {
  return loggedInvoke("execute_cto_actions", {
    projectId,
    review,
    executionMode,
  });
}

export async function logCtoDecision(
  projectId: string,
  decision: CtoDecisionRecordInput,
): Promise<CtoDecision> {
  return loggedInvoke("log_cto_decision", {
    projectId,
    decision,
  });
}

export async function listCtoDecisions(
  projectId: string,
): Promise<CtoDecision[]> {
  return loggedInvoke("list_cto_decisions", { projectId });
}

export async function rollbackCtoDecision(
  decisionId: string,
): Promise<CtoDecision> {
  return loggedInvoke("rollback_cto_decision", { decisionId });
}

export function onCtoChatChunk(
  callback: (payload: CtoChatChunk) => void,
): Promise<import("@tauri-apps/api/event").UnlistenFn> {
  return listenToEvent<CtoChatChunk>("cto-chat-chunk", callback);
}

export interface CtoActionStepEvent {
  projectId: string;
  step: number;
  total: number;
  action: string;
  status: "started" | "completed" | "failed";
}

export function onCtoActionStep(
  callback: (payload: CtoActionStepEvent) => void,
): Promise<import("@tauri-apps/api/event").UnlistenFn> {
  return listenToEvent<CtoActionStepEvent>("cto-action-step", callback);
}
