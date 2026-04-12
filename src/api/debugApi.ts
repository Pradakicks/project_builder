import type {
  CapturedScenario,
  DebugLogTail,
  DebugSessionSummary,
} from "../types";
import { loggedInvoke } from "./runtime";

export async function getDebugSessionInfo(): Promise<DebugSessionSummary> {
  return loggedInvoke("get_debug_session_info");
}

export async function recordDebugScenario(
  scenario: CapturedScenario,
): Promise<CapturedScenario> {
  const record = await loggedInvoke<{
    path: string | null;
    id: string;
    kind: "cto-chat";
    status: "failed" | "rejected";
    projectId: string;
    projectName: string | null;
    prompt: string;
    conversation: { role: string; content: string }[];
    assistantText: string | null;
    cleanedContent: string | null;
    review: unknown;
    decision: unknown;
    error: string | null;
    capturedAt: string;
  }>("record_debug_scenario", { scenario });

  return {
    id: record.id,
    kind: record.kind,
    status: record.status,
    projectId: record.projectId,
    projectName: record.projectName,
    prompt: record.prompt,
    conversation: record.conversation,
    assistantText: record.assistantText,
    cleanedContent: record.cleanedContent,
    review: record.review as CapturedScenario["review"],
    decision: record.decision as CapturedScenario["decision"],
    error: record.error,
    capturedAt: record.capturedAt,
    path: record.path,
  };
}

export async function getLastDebugScenario(): Promise<CapturedScenario | null> {
  const record = await loggedInvoke<{
    id: string;
    kind: "cto-chat";
    status: "failed" | "rejected";
    projectId: string;
    projectName: string | null;
    prompt: string;
    conversation: { role: string; content: string }[];
    assistantText: string | null;
    cleanedContent: string | null;
    review: unknown;
    decision: unknown;
    error: string | null;
    capturedAt: string;
    path: string | null;
  } | null>("get_last_debug_scenario");

  return record
    ? {
        id: record.id,
        kind: record.kind,
        status: record.status,
        projectId: record.projectId,
        projectName: record.projectName,
        prompt: record.prompt,
        conversation: record.conversation,
        assistantText: record.assistantText,
        cleanedContent: record.cleanedContent,
        review: record.review as CapturedScenario["review"],
        decision: record.decision as CapturedScenario["decision"],
        error: record.error,
        capturedAt: record.capturedAt,
        path: record.path,
      }
    : null;
}

export async function readDebugLogTail(limit = 120): Promise<DebugLogTail> {
  return loggedInvoke("read_debug_log_tail", { limit });
}
