import { useState, useRef, useEffect } from "react";
import { useChatStore } from "../../store/useChatStore";
import { useAppStore } from "../../store/useAppStore";
import { useProjectStore } from "../../store/useProjectStore";
import { useToastStore } from "../../store/useToastStore";
import { useDialogStore } from "../../store/useDialogStore";
import { useDebugStore } from "../../store/useDebugStore";
import { useGoalRunStore } from "../../store/useGoalRunStore";
import { Markdown } from "../ui/Markdown";
import { devLog } from "../../utils/devLog";
import {
  reviewActions,
  describeAction,
  executeActions,
} from "../../utils/ctoActions";
import type {
  CtoDecision,
  CtoActionReview,
  CtoActionExecutionResult,
  CtoDecisionRecordInput,
  CapturedScenario,
  DebugConversationMessage,
} from "../../types";

type Tab = "chat" | "decisions";

function formatActionReviewDetails(review: CtoActionReview): string {
  const sections = [
    `Actions: ${review.actions.length}`,
    ...review.actions.map((action, index) => `${index + 1}. ${describeAction(action)}`),
  ];

  if (review.validationErrors.length > 0) {
    sections.push("");
    sections.push("Validation errors:");
    sections.push(...review.validationErrors.map((error) => `- ${error}`));
  }

  return sections.join("\n");
}

function buildExecutedDecision(
  assistantText: string,
  review: CtoActionReview,
  result: CtoActionExecutionResult,
): CtoDecisionRecordInput {
  const summary =
    review.cleanedContent.trim() || assistantText.trim() || "CTO response";
  return {
    summary,
    review: {
      assistantText: assistantText.trim(),
      cleanedContent: review.cleanedContent,
      actions: review.actions,
      validationErrors: review.validationErrors,
    },
    execution: {
      executed: result.executed,
      errors: result.errors,
      steps: result.steps,
      reloadCurrentProject: result.reloadCurrentProject,
      switchToTab: result.switchToTab ?? null,
      rollback: result.rollback,
    },
    status: result.errors.length > 0 ? "failed" : "executed",
  };
}

function buildRejectedDecision(
  assistantText: string,
  review: CtoActionReview,
): CtoDecisionRecordInput {
  return {
    summary: buildRejectedDecisionSummary(review),
    review: {
      assistantText: assistantText.trim(),
      cleanedContent: review.cleanedContent,
      actions: review.actions,
      validationErrors: review.validationErrors,
    },
    execution: null,
    status: "rejected",
  };
}

function buildRejectedDecisionSummary(review: CtoActionReview): string {
  const sections = [
    "Rejected CTO action block",
    "Nothing executed because validation failed before dispatch.",
  ];

  if (review.actions.length > 0) {
    sections.push(`Recovered actions: ${review.actions.length}`);
  }

  if (review.validationErrors.length > 0) {
    sections.push("");
    sections.push("Validation errors:");
    sections.push(...review.validationErrors.map((error) => `- ${error}`));
  }

  sections.push("");
  sections.push(
    "Next step: ask the CTO to return only fenced ```action blocks containing a single JSON object per block.",
  );

  return sections.join("\n");
}

export function ChatPanel({
  open,
  onToggle,
  embedded,
  onSwitchTab,
}: {
  open: boolean;
  onToggle: () => void;
  embedded?: boolean;
  onSwitchTab?: (tab: string) => void;
}) {
  const project = useProjectStore((s) => s.project);
  const projectId = project?.id ?? null;
  const showConfirm = useDialogStore((s) => s.showConfirm);
  const currentGoalRun = useGoalRunStore((s) => s.currentGoalRun);
  const runtimeStatus = useGoalRunStore((s) => s.runtimeStatus);
  const orchestrating = useGoalRunStore((s) => s.orchestrating);
  const goalRunError = useGoalRunStore((s) => s.lastError);
  const thread = useChatStore((s) =>
    projectId ? s.threads[projectId] : undefined,
  );
  const [input, setInput] = useState("");
  const [tab, setTab] = useState<Tab>("chat");
  const [decisions, setDecisions] = useState<CtoDecision[]>([]);
  const [expandedDecision, setExpandedDecision] = useState<string | null>(null);
  const [pendingReview, setPendingReview] = useState<{
    projectId: string;
    projectName: string;
    goalRunId: string | null;
    requestId: string;
    prompt: string;
    conversation: DebugConversationMessage[];
    assistantText: string;
    cleanedContent: string;
    review: CtoActionReview;
  } | null>(null);
  const [executingReview, setExecutingReview] = useState(false);
  const [runtimeDetecting, setRuntimeDetecting] = useState(false);
  const [runtimeAgentOutput, setRuntimeAgentOutput] = useState("");
  const [showManualRuntime, setShowManualRuntime] = useState(false);
  const [manualRunCommand, setManualRunCommand] = useState("");
  const [manualAppUrl, setManualAppUrl] = useState("http://127.0.0.1:8080");
  const [manualPort, setManualPort] = useState("");
  const listRef = useRef<HTMLDivElement>(null);
  const messages = thread?.messages ?? [];
  const streaming = thread?.streaming ?? false;

  const refreshDecisions = (decision: CtoDecision) => {
    setDecisions((current) => {
      const next = current.filter((item) => item.id !== decision.id);
      return [decision, ...next];
    });
  };

  const captureScenario = async (scenario: CapturedScenario) => {
    useDebugStore.getState().captureScenario(scenario);
    try {
      const { recordDebugScenario } = await import("../../api/debugApi");
      const persisted = await recordDebugScenario(scenario);
      useDebugStore.getState().captureScenario(persisted);
    } catch (error) {
      devLog("warn", "Diagnostics", "Failed to persist debug scenario artifact", error);
    }
  };

  const buildScenario = ({
    status,
    prompt,
    conversation,
    assistantText,
    cleanedContent,
    review,
    decision,
    error,
    scenarioProjectId,
    scenarioProjectName,
  }: {
    status: "failed" | "rejected";
    prompt: string;
    conversation: DebugConversationMessage[];
    assistantText: string | null;
    cleanedContent: string | null;
    review: CtoActionReview | null;
    decision: CtoDecisionRecordInput | null;
    error: string | null;
    scenarioProjectId: string;
    scenarioProjectName: string | null;
  }): CapturedScenario => ({
    id: crypto.randomUUID(),
    kind: "cto-chat",
    status,
    projectId: scenarioProjectId,
    projectName: scenarioProjectName,
    prompt,
    conversation,
    assistantText,
    cleanedContent,
    review,
    decision,
    error,
    capturedAt: new Date().toISOString(),
  });

  useEffect(() => {
    listRef.current?.scrollTo(0, listRef.current.scrollHeight);
  }, [messages.length, streaming]);

  useEffect(() => {
    setInput("");
    setDecisions([]);
    setExpandedDecision(null);
    setPendingReview(null);
    setExecutingReview(false);
  }, [projectId]);

  // Load decisions when switching to decisions tab
  useEffect(() => {
    let cancelled = false;
    if (tab === "decisions" && project) {
      import("../../api/ctoApi").then(({ listCtoDecisions }) => {
        listCtoDecisions(project.id)
          .then((items) => {
            if (!cancelled) setDecisions(items);
          })
          .catch((e: unknown) => devLog("error", "Chat", "Failed to load CTO decisions", e));
      });
    } else {
      setDecisions([]);
    }
    return () => {
      cancelled = true;
    };
  }, [tab, project?.id]);

  const sendPrompt = async (
    promptText: string,
    conversationOverride?: DebugConversationMessage[],
  ) => {
    const text = promptText.trim();
    if (!text || streaming || !project) return;
    devLog("info", "Chat", `Sending message (${text.length} chars)`);
    const conversation =
      conversationOverride ??
      messages
      .filter((m) => m.content)
      .map((m) => ({
        role: m.role === "user" ? "user" : "assistant",
        content: m.content,
      }));
    useDebugStore.getState().recordEvent({
      kind: "cto-request",
      level: "info",
      category: "CTO",
      message: "Starting CTO request",
      data: {
        projectId: project.id,
        prompt: text,
        conversationCount: conversation.length,
      },
    });
    const requestId = crypto.randomUUID();
    useChatStore.getState().startRequest(project.id, text, requestId);
    setInput("");
    setPendingReview(null);

    let goalRunId: string | null = null;
    try {
      const goalRun = await useGoalRunStore.getState().beginPromptRun(project.id, text);
      goalRunId = goalRun.id;
    } catch (error) {
      devLog("warn", "Chat", "Failed to create goal run for CTO prompt", error);
    }

    const originProjectId = project.id;
    const originProjectName = project.name;
    let streamBuffer = "";

    try {
      const { chatWithCto, onCtoChatChunk } = await import("../../api/ctoApi");

      const unlisten = await onCtoChatChunk(async (payload) => {
        if (
          payload.projectId !== originProjectId ||
          payload.requestId !== requestId
        ) {
          return;
        }

        if (payload.done) {
          const review = reviewActions(streamBuffer);
          useDebugStore.getState().recordEvent({
            kind: "cto-response",
            level: "info",
            category: "CTO",
            message: "Received CTO response",
            data: {
              projectId: originProjectId,
              responseLength: streamBuffer.length,
            },
          });
          useDebugStore.getState().recordEvent({
            kind: "cto-review",
            level: review.validationErrors.length > 0 ? "warn" : "info",
            category: "CTO",
            message: "Reviewed CTO action block",
            data: {
              actionCount: review.actions.length,
              validationErrors: review.validationErrors,
            },
          });
          useChatStore
            .getState()
            .finalizeRequest(originProjectId, requestId, review.cleanedContent);

          if (review.actions.length > 0 || review.validationErrors.length > 0) {
            setPendingReview({
              projectId: originProjectId,
              projectName: originProjectName,
              goalRunId,
              requestId,
              prompt: text,
              conversation,
              assistantText: streamBuffer,
              cleanedContent: review.cleanedContent,
              review,
            });
          } else {
            setPendingReview(null);
          }

          if (review.validationErrors.length > 0) {
            useToastStore.getState().addToast(
              `CTO action block rejected: ${review.validationErrors[0]}. Nothing executed.`,
              "warning",
            );
            const rejectedDecision = buildRejectedDecision(streamBuffer, review);
            await captureScenario(
              buildScenario({
                status: "rejected",
                prompt: text,
                conversation,
                assistantText: streamBuffer,
                cleanedContent: review.cleanedContent,
                review,
                decision: rejectedDecision,
                error: review.validationErrors.join("; "),
                scenarioProjectId: originProjectId,
                scenarioProjectName: originProjectName,
              }),
            );
            if (goalRunId) {
              try {
                await import("../../api/goalRunApi").then(({ updateGoalRun }) =>
                  updateGoalRun(goalRunId!, {
                    status: "blocked",
                    blockerReason: review.validationErrors.join("; "),
                    lastFailureSummary: "CTO action validation failed",
                  }),
                );
                await useGoalRunStore.getState().loadGoalRuns(originProjectId);
              } catch (goalRunError) {
                devLog("warn", "Chat", "Failed to update blocked goal run after CTO rejection", goalRunError);
              }
            }
            try {
              const { logCtoDecision } = await import("../../api/ctoApi");
              const decision = await logCtoDecision(
                originProjectId,
                rejectedDecision,
              );
              refreshDecisions(decision);
              useDebugStore.getState().recordEvent({
                kind: "cto-decision",
                level: "warn",
                category: "CTO",
                message: "Logged rejected CTO decision",
                data: { projectId: originProjectId, decisionId: decision.id },
              });
            } catch (logError) {
              devLog("warn", "Chat", "Failed to log rejected CTO decision", logError);
              await captureScenario(
                buildScenario({
                  status: "failed",
                  prompt: text,
                  conversation,
                  assistantText: streamBuffer,
                  cleanedContent: review.cleanedContent,
                  review,
                  decision: rejectedDecision,
                  error: `Failed to log rejected CTO decision: ${logError}`,
                  scenarioProjectId: originProjectId,
                  scenarioProjectName: originProjectName,
                }),
              );
            }
          }

          if (review.actions.length === 0 && review.validationErrors.length === 0 && goalRunId) {
            try {
              await import("../../api/goalRunApi").then(({ updateGoalRun }) =>
                updateGoalRun(goalRunId!, {
                  status: "blocked",
                  blockerReason: "CTO returned no actionable changes.",
                  lastFailureSummary: "No CTO actions were emitted",
                }),
              );
              await useGoalRunStore.getState().loadGoalRuns(originProjectId);
            } catch (goalRunError) {
              devLog("warn", "Chat", "Failed to update blocked goal run after empty CTO response", goalRunError);
            }
          }

          devLog("info", "Chat", `CTO response complete (${streamBuffer.length} chars)`);
          unlisten();
        } else {
          streamBuffer += payload.chunk;
          useChatStore
            .getState()
            .appendChunk(originProjectId, requestId, payload.chunk);
        }
      });

      await chatWithCto(originProjectId, text, conversation, requestId);
    } catch (e) {
      devLog("error", "Chat", `CTO chat error`, e);
      await captureScenario(
        buildScenario({
          status: "failed",
          prompt: text,
          conversation,
          assistantText: streamBuffer || null,
          cleanedContent: streamBuffer || null,
          review: null,
          decision: null,
          error: `CTO chat error: ${e}`,
          scenarioProjectId: originProjectId,
          scenarioProjectName: originProjectName,
        }),
      );
      if (goalRunId) {
        try {
          await import("../../api/goalRunApi").then(({ updateGoalRun }) =>
            updateGoalRun(goalRunId!, {
              status: "failed",
              lastFailureSummary: `CTO chat error: ${e}`,
            }),
          );
          await useGoalRunStore.getState().loadGoalRuns(originProjectId);
        } catch (goalRunError) {
          devLog("warn", "Chat", "Failed to update failed goal run after CTO error", goalRunError);
        }
      }
      useChatStore
        .getState()
        .failRequest(originProjectId, requestId, "(Failed to connect to LLM)");
      useToastStore.getState().addToast(
        useAppStore.getState().activeProjectId === originProjectId
          ? `CTO chat error: ${e}`
          : `CTO chat error for "${originProjectName}": ${e}`,
      );
    }
  };

  const retryRuntimeWithAgent = async () => {
    if (!project || !currentGoalRun || runtimeDetecting) return;
    setRuntimeDetecting(true);
    setRuntimeAgentOutput("");
    const goalRunId = currentGoalRun.id;
    try {
      const { listen } = await import("@tauri-apps/api/event");
      const unlisten = await listen<{ projectId: string; chunk: string; done: boolean }>(
        "runtime-detection-chunk",
        (event) => {
          if (event.payload.projectId === project.id && !event.payload.done) {
            setRuntimeAgentOutput((prev) => prev + event.payload.chunk);
          }
        },
      );
      const runtimeApi = await import("../../api/runtimeApi");
      const spec = await runtimeApi.detectRuntimeWithAgent(project.id);
      unlisten();
      if (spec) {
        await runtimeApi.configureRuntime(project.id, spec);
        await useGoalRunStore.getState().retryGoalRun(goalRunId);
      } else {
        useToastStore.getState().addToast(
          "Agent could not detect runtime — use the manual form below.",
          "warning",
        );
      }
    } catch (error) {
      useToastStore.getState().addToast(`Runtime agent error: ${error}`, "warning");
    } finally {
      setRuntimeDetecting(false);
    }
  };

  const applyManualRuntime = async () => {
    if (!project || !currentGoalRun || !manualRunCommand.trim()) return;
    const goalRunId = currentGoalRun.id;
    try {
      const runtimeApi = await import("../../api/runtimeApi");
      const port = manualPort ? parseInt(manualPort, 10) : undefined;
      await runtimeApi.configureRuntime(project.id, {
        installCommand: null,
        runCommand: manualRunCommand.trim(),
        verifyCommand: null,
        readinessCheck: { kind: "none" },
        stopBehavior: { kind: "kill" },
        appUrl: manualAppUrl.trim() || null,
        portHint: port ?? null,
      });
      await useGoalRunStore.getState().retryGoalRun(goalRunId);
    } catch (error) {
      useToastStore.getState().addToast(`Failed to configure runtime: ${error}`, "warning");
    }
  };

  const send = async () => {
    await sendPrompt(input);
  };

  useEffect(() => {
    if (!import.meta.env.DEV || !projectId) {
      useDebugStore.getState().registerReplayHandler(null);
      return;
    }

    useDebugStore.getState().registerReplayHandler(async (scenario) => {
      if (scenario.kind !== "cto-chat") {
        throw new Error(`Unsupported scenario kind: ${scenario.kind}`);
      }
      if (scenario.projectId !== projectId) {
        throw new Error("Open the captured project before replaying this scenario.");
      }
      await sendPrompt(scenario.prompt, scenario.conversation);
    });

    return () => {
      useDebugStore.getState().registerReplayHandler(null);
    };
  }, [projectId, project?.id, streaming, messages]);

  const executePendingReview = async () => {
    const currentReview = pendingReview;
    if (!currentReview || currentReview.review.validationErrors.length > 0 || executingReview) {
      return;
    }

    setExecutingReview(true);
    const addToast = useToastStore.getState().addToast;
    let executedDecision: CtoDecisionRecordInput | null = null;
    try {
      const result = await executeActions(
        currentReview.review.actions,
        currentReview.projectId,
      );
      const isOriginProjectActive =
        useAppStore.getState().activeProjectId === currentReview.projectId &&
        useProjectStore.getState().project?.id === currentReview.projectId;

      if (result.executed > 0) {
        addToast(
          isOriginProjectActive
            ? `CTO applied ${result.executed} change${result.executed > 1 ? "s" : ""}`
            : `CTO applied ${result.executed} change${result.executed > 1 ? "s" : ""} to "${currentReview.projectName}"`,
          "info",
        );
      }

      for (const err of result.errors) {
        addToast(
          isOriginProjectActive ? err : `${currentReview.projectName}: ${err}`,
          "warning",
        );
      }

      const { logCtoDecision } = await import("../../api/ctoApi");
      executedDecision = buildExecutedDecision(
        currentReview.assistantText,
        currentReview.review,
        result,
      );
      const savedDecision = await logCtoDecision(currentReview.projectId, executedDecision);
      refreshDecisions(savedDecision);
      useDebugStore.getState().recordEvent({
        kind: "cto-decision",
        level: result.errors.length > 0 ? "warn" : "info",
        category: "CTO",
        message: "Logged executed CTO decision",
        data: { projectId: currentReview.projectId, decisionId: savedDecision.id },
      });

      if (isOriginProjectActive && result.reloadCurrentProject) {
        await useProjectStore.getState().loadProject(currentReview.projectId);
      }

      if (isOriginProjectActive && result.switchToTab) {
        onSwitchTab?.(result.switchToTab);
      }

      if (
        currentReview.goalRunId &&
        useProjectStore.getState().project?.settings.autonomyMode === "autopilot"
      ) {
        void useGoalRunStore
          .getState()
          .continueAutopilot(currentReview.goalRunId)
          .catch((error) =>
            devLog("error", "Chat", "Autopilot continuation failed", error),
          );
      }

      setPendingReview(null);
    } catch (error) {
      devLog("error", "Chat", "Failed to execute CTO review", error);
      await captureScenario(
        buildScenario({
          status: "failed",
          prompt: currentReview.prompt,
          conversation: currentReview.conversation,
          assistantText: currentReview.assistantText,
          cleanedContent: currentReview.cleanedContent,
          review: currentReview.review,
          decision: executedDecision,
          error: `CTO execution failed: ${error}`,
          scenarioProjectId: currentReview.projectId,
          scenarioProjectName: currentReview.projectName,
        }),
      );
      if (currentReview.goalRunId) {
        try {
          await import("../../api/goalRunApi").then(({ updateGoalRun }) =>
            updateGoalRun(currentReview.goalRunId!, {
              status: "failed",
              lastFailureSummary: `CTO execution failed: ${error}`,
            }),
          );
          await useGoalRunStore.getState().loadGoalRuns(currentReview.projectId);
        } catch (goalRunError) {
          devLog("warn", "Chat", "Failed to update goal run after CTO execution failure", goalRunError);
        }
      }
      addToast(
        useAppStore.getState().activeProjectId === currentReview.projectId
          ? `CTO execution failed: ${error}`
          : `CTO execution failed for "${currentReview.projectName}": ${error}`,
      );
    } finally {
      setExecutingReview(false);
    }
  };

  const promptPendingReview = () => {
    if (!pendingReview || pendingReview.review.validationErrors.length > 0) {
      return;
    }

    showConfirm(
      `Execute ${pendingReview.review.actions.length} CTO action${pendingReview.review.actions.length !== 1 ? "s" : ""}?`,
      () => {
        void executePendingReview();
      },
      {
        title: "Review CTO actions",
        details: formatActionReviewDetails(pendingReview.review),
        confirmLabel: "Execute",
        cancelLabel: "Keep Reviewing",
      },
    );
  };

  const rollbackDecision = async (decision: CtoDecision) => {
    if (!decision.execution?.rollback.supported) {
      return;
    }

    showConfirm(
      `Roll back CTO decision from ${new Date(decision.createdAt).toLocaleString()}?`,
      () => {
        void (async () => {
          try {
            const { rollbackCtoDecision } = await import("../../api/ctoApi");
            const updatedDecision = await rollbackCtoDecision(decision.id);
            refreshDecisions(updatedDecision);
            if (useAppStore.getState().activeProjectId === decision.projectId) {
              await useProjectStore.getState().loadProject(decision.projectId);
            }
            useToastStore.getState().addToast("CTO decision rolled back", "info");
        } catch (error) {
          devLog("error", "Chat", "Failed to roll back CTO decision", error);
          try {
            const { listCtoDecisions } = await import("../../api/ctoApi");
            const items = await listCtoDecisions(decision.projectId);
            setDecisions(items);
          } catch (refreshError) {
            devLog("warn", "Chat", "Failed to refresh CTO decisions after rollback error", refreshError);
          }
          useToastStore.getState().addToast(`CTO rollback failed: ${error}`, "warning");
        }
      })();
    },
      {
        title: "Rollback CTO decision",
        details: decision.execution?.rollback.reason ?? "This decision can be rolled back safely.",
        confirmLabel: "Rollback",
        cancelLabel: "Keep Decision",
      },
    );
  };

  if (!open) {
    return (
      <button
        onClick={onToggle}
        className="absolute left-2 top-14 z-10 rounded-lg border border-gray-700 bg-gray-900 p-2 text-gray-400 hover:text-gray-200 shadow-lg"
        title="Open CTO Agent"
      >
        <svg
          width="18"
          height="18"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
        >
          <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" />
        </svg>
      </button>
    );
  }

  const content = (
    <>
      {!embedded && (
        <div className="flex items-center justify-between border-b border-gray-800 px-3 py-2">
          <span className="text-xs font-semibold text-gray-300">
            CTO Agent
          </span>
          <button
            onClick={onToggle}
            className="text-xs text-gray-500 hover:text-gray-300"
          >
            Collapse
          </button>
        </div>
      )}

      {/* Tab switcher */}
      <div className="flex border-b border-gray-800">
        <button
          onClick={() => setTab("chat")}
          className={`flex-1 py-1.5 text-[10px] font-medium transition-colors ${
            tab === "chat"
              ? "text-blue-400 border-b border-blue-400"
              : "text-gray-500 hover:text-gray-300"
          }`}
        >
          Chat
        </button>
        <button
          onClick={() => setTab("decisions")}
          className={`flex-1 py-1.5 text-[10px] font-medium transition-colors ${
            tab === "decisions"
              ? "text-blue-400 border-b border-blue-400"
              : "text-gray-500 hover:text-gray-300"
          }`}
        >
          Decisions
        </button>
      </div>

      {tab === "chat" && (
        <>
          {currentGoalRun && currentGoalRun.projectId === projectId ? (
            <div className="mx-3 mt-3 rounded border border-emerald-900/50 bg-emerald-950/20 p-3 text-xs">
              <div className="flex items-start justify-between gap-3">
                <div>
                  <p className="font-semibold text-emerald-200">
                    Goal run in progress
                  </p>
                  <p className="mt-1 text-[11px] text-gray-300">
                    {currentGoalRun.prompt}
                  </p>
                </div>
                <div className="shrink-0 text-right">
                  <p className="rounded bg-emerald-900/40 px-2 py-0.5 text-[10px] font-medium text-emerald-200">
                    {currentGoalRun.status} / {currentGoalRun.phase}
                  </p>
                  {orchestrating ? (
                    <p className="mt-1 text-[10px] text-emerald-300">
                      Autopilot advancing
                    </p>
                  ) : null}
                </div>
              </div>
              <div className="mt-2 grid gap-2 text-[11px] text-gray-300 md:grid-cols-2">
                <p>
                  Plan:{" "}
                  <span className="font-mono text-gray-100">
                    {currentGoalRun.currentPlanId ?? "none"}
                  </span>
                </p>
                <p>
                  Runtime:{" "}
                  <span className="font-mono text-gray-100">
                    {runtimeStatus?.session?.status ?? currentGoalRun.runtimeStatusSummary ?? "idle"}
                  </span>
                </p>
              </div>
              {currentGoalRun.lastFailureSummary || goalRunError ? (
                <div className="mt-2 rounded border border-amber-900/50 bg-amber-950/20 px-2 py-1 text-[11px] text-amber-100">
                  {currentGoalRun.lastFailureSummary ?? goalRunError}
                </div>
              ) : null}

              {/* Interrupted: run was mid-execution when the app closed */}
              {currentGoalRun.status === "interrupted" && (
                <div className="mt-3 space-y-2">
                  <p className="text-[11px] text-amber-300">
                    This run was interrupted when the app closed.
                  </p>
                  <button
                    onClick={() => void useGoalRunStore.getState().retryGoalRun(currentGoalRun.id)}
                    className="w-full rounded border border-amber-700 bg-amber-900/20 px-2.5 py-1.5 text-[11px] font-medium text-amber-200 hover:bg-amber-900/40"
                  >
                    Resume from {currentGoalRun.phase}
                  </button>
                </div>
              )}

              {/* Blocked at runtime-configuration: show agent retry + manual form */}
              {currentGoalRun.status === "blocked" &&
               currentGoalRun.phase === "runtime-configuration" && (
                <div className="mt-3 space-y-2">
                  <p className="text-[11px] text-gray-400">
                    The autopilot could not determine how to run the generated project.
                  </p>
                  <button
                    onClick={() => void retryRuntimeWithAgent()}
                    disabled={runtimeDetecting}
                    className="w-full rounded border border-blue-700 bg-blue-900/30 px-2.5 py-1.5 text-[11px] font-medium text-blue-200 hover:bg-blue-900/50 disabled:opacity-50"
                  >
                    {runtimeDetecting ? "Agent detecting…" : "Retry with agent"}
                  </button>
                  {runtimeAgentOutput && (
                    <div className="rounded border border-gray-700 bg-gray-900 p-2 text-[10px] font-mono text-gray-400 max-h-24 overflow-y-auto whitespace-pre-wrap">
                      {runtimeAgentOutput}
                    </div>
                  )}
                  <button
                    onClick={() => setShowManualRuntime((v) => !v)}
                    className="w-full text-left text-[10px] text-gray-500 hover:text-gray-300"
                  >
                    {showManualRuntime ? "▾" : "▸"} Configure manually
                  </button>
                  {showManualRuntime && (
                    <div className="space-y-1.5 rounded border border-gray-700 bg-gray-900/60 p-2">
                      <div>
                        <label className="block text-[10px] text-gray-400 mb-0.5">Run command *</label>
                        <input
                          type="text"
                          value={manualRunCommand}
                          onChange={(e) => setManualRunCommand(e.target.value)}
                          placeholder="e.g. python3 app.py"
                          className="w-full rounded border border-gray-700 bg-gray-800 px-2 py-1 text-[11px] text-gray-100 placeholder-gray-600 focus:outline-none focus:border-blue-600"
                        />
                      </div>
                      <div className="grid grid-cols-2 gap-1.5">
                        <div>
                          <label className="block text-[10px] text-gray-400 mb-0.5">App URL</label>
                          <input
                            type="text"
                            value={manualAppUrl}
                            onChange={(e) => setManualAppUrl(e.target.value)}
                            className="w-full rounded border border-gray-700 bg-gray-800 px-2 py-1 text-[11px] text-gray-100 focus:outline-none focus:border-blue-600"
                          />
                        </div>
                        <div>
                          <label className="block text-[10px] text-gray-400 mb-0.5">Port</label>
                          <input
                            type="number"
                            value={manualPort}
                            onChange={(e) => setManualPort(e.target.value)}
                            placeholder="8080"
                            className="w-full rounded border border-gray-700 bg-gray-800 px-2 py-1 text-[11px] text-gray-100 placeholder-gray-600 focus:outline-none focus:border-blue-600"
                          />
                        </div>
                      </div>
                      <button
                        onClick={() => void applyManualRuntime()}
                        disabled={!manualRunCommand.trim()}
                        className="w-full rounded border border-gray-600 bg-gray-800 px-2.5 py-1 text-[11px] text-gray-200 hover:bg-gray-700 disabled:opacity-40"
                      >
                        Save &amp; continue
                      </button>
                    </div>
                  )}
                </div>
              )}

              <div className="mt-2 flex justify-end">
                <button
                  onClick={() => onSwitchTab?.("plan")}
                  className="rounded border border-gray-700 px-2.5 py-1 text-[11px] text-gray-200 hover:bg-gray-800"
                >
                  Open work plan
                </button>
              </div>
            </div>
          ) : null}

          <div ref={listRef} className="flex-1 overflow-y-auto p-3 space-y-2">
            {messages.length === 0 && (
              <p className="text-[11px] text-gray-600 text-center mt-8">
                The CTO suggests actions for review. Ask a question or describe
                what you need.
              </p>
            )}
            {messages.map((msg) => (
              <div
                key={msg.id}
                className={`flex ${msg.role === "user" ? "justify-end" : "justify-start"}`}
              >
                <div
                  className={`max-w-[85%] rounded-lg px-2.5 py-1.5 text-xs ${
                    msg.role === "user"
                      ? "bg-blue-600 text-white"
                      : "bg-gray-800 text-gray-200"
                  }`}
                >
                  {msg.role === "agent" ? (
                    msg.content ? (
                      <Markdown content={msg.content} />
                    ) : streaming ? (
                      <span className="inline-flex gap-1">
                        <span
                          className="w-1.5 h-1.5 bg-blue-400 rounded-full animate-bounce"
                          style={{ animationDelay: "0ms" }}
                        />
                        <span
                          className="w-1.5 h-1.5 bg-blue-400 rounded-full animate-bounce"
                          style={{ animationDelay: "150ms" }}
                        />
                        <span
                          className="w-1.5 h-1.5 bg-blue-400 rounded-full animate-bounce"
                          style={{ animationDelay: "300ms" }}
                        />
                      </span>
                    ) : null
                  ) : (
                    msg.content || ""
                  )}
                </div>
              </div>
            ))}
          </div>

          {pendingReview && pendingReview.projectId === projectId ? (
            <div className="mx-3 rounded border border-blue-900/60 bg-blue-950/30 p-3 text-xs">
              <div className="flex items-start justify-between gap-3">
                <div>
                  <p className="font-semibold text-blue-200">
                    CTO action review
                  </p>
                  <p className="mt-1 text-[11px] text-gray-300">
                    {pendingReview.review.validationErrors.length > 0
                      ? "The returned action block was rejected before execution. Nothing changed in the project."
                      : "The returned action block is valid. Review it here before execution."}
                  </p>
                  {pendingReview.review.validationErrors.length > 0 ? (
                    <p className="mt-1 text-[11px] text-gray-400">
                      Next step: ask the CTO to emit one fenced{" "}
                      <code className="rounded bg-gray-900 px-1 py-0.5 text-[10px] text-gray-200">
                        ```action
                      </code>{" "}
                      block per change, with valid JSON only.
                    </p>
                  ) : null}
                </div>
                <span
                  className={`shrink-0 rounded px-2 py-0.5 text-[10px] font-medium ${
                    pendingReview.review.validationErrors.length > 0
                      ? "bg-red-900/40 text-red-300"
                      : "bg-blue-900/50 text-blue-300"
                  }`}
                >
                  {pendingReview.review.validationErrors.length > 0
                    ? "Rejected"
                    : `${pendingReview.review.actions.length} action${pendingReview.review.actions.length !== 1 ? "s" : ""}`}
                </span>
              </div>

              <div className="mt-2 space-y-1">
                {pendingReview.review.actions.length > 0 ? (
                  pendingReview.review.actions.map((action, index) => (
                    <div
                      key={`${pendingReview.requestId}-${index}`}
                      className="flex items-start gap-2 text-[11px] text-gray-200"
                    >
                      <span className="mt-0.5 text-blue-300">•</span>
                      <span>{describeAction(action)}</span>
                    </div>
                  ))
                ) : pendingReview.review.validationErrors.length > 0 ? (
                  <p className="text-[11px] text-gray-400">
                    No valid actions were extracted, so nothing was queued for execution.
                  </p>
                ) : null}
                {pendingReview.review.validationErrors.length > 0 ? (
                  <div className="rounded border border-red-900/60 bg-red-950/40 px-2 py-1 text-[11px] text-red-200">
                    {pendingReview.review.validationErrors.map((error, index) => (
                      <p key={`${pendingReview.requestId}-validation-${index}`}>{error}</p>
                    ))}
                  </div>
                ) : null}
              </div>

              <div className="mt-3 flex justify-end gap-2">
                <button
                  onClick={() => setPendingReview(null)}
                  className="rounded border border-gray-700 px-2.5 py-1.5 text-[11px] text-gray-300 hover:bg-gray-800"
                >
                  Dismiss
                </button>
                {pendingReview.review.validationErrors.length === 0 ? (
                  <button
                    onClick={promptPendingReview}
                    disabled={executingReview}
                    className="rounded bg-blue-600 px-2.5 py-1.5 text-[11px] text-white hover:bg-blue-500 disabled:opacity-50"
                  >
                    {executingReview ? "Executing..." : "Review & Execute"}
                  </button>
                ) : null}
              </div>
            </div>
          ) : null}

          <div className="border-t border-gray-800 p-2 flex gap-1.5">
            <input
              type="text"
              value={input}
              onChange={(e) => setInput(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && send()}
              placeholder="Message CTO agent..."
              disabled={streaming}
              className="flex-1 rounded border border-gray-700 bg-gray-800 px-2 py-1.5 text-xs text-gray-100 placeholder-gray-600 focus:border-blue-500 focus:outline-none disabled:opacity-50"
            />
            <button
              onClick={send}
              disabled={streaming}
              className="rounded bg-blue-600 px-2.5 py-1.5 text-xs text-white hover:bg-blue-500 disabled:opacity-50"
            >
              Send
            </button>
          </div>
        </>
      )}

      {tab === "decisions" && (
        <div className="flex-1 overflow-y-auto p-3 space-y-2">
          {decisions.length === 0 && (
            <p className="text-[11px] text-gray-600 text-center mt-8">
              No decisions yet. The CTO's actions will appear here.
            </p>
          )}
          {decisions.map((d) => {
            const actions = d.review.actions;
            const isExpanded = expandedDecision === d.id;
            const isRejected = d.status === "rejected";
            const date = new Date(d.createdAt);
            const timeStr = date.toLocaleString(undefined, {
              month: "short",
              day: "numeric",
              hour: "2-digit",
              minute: "2-digit",
            });

            return (
              <div
                key={d.id}
                className="rounded border border-gray-700 bg-gray-800/50 p-2"
              >
                <button
                  onClick={() =>
                    setExpandedDecision(isExpanded ? null : d.id)
                  }
                  className="w-full text-left"
                >
                  <div className="flex items-start justify-between gap-2">
                    <p className="text-[11px] text-gray-200 line-clamp-2 flex-1">
                      {d.summary.slice(0, 200)}
                    </p>
                    <span className="text-[9px] text-gray-500 shrink-0 mt-0.5">
                      {timeStr}
                    </span>
                  </div>
                  <div className="flex items-center gap-1.5 mt-1">
                    <span
                      className={`rounded px-1.5 py-0.5 text-[9px] font-medium ${
                        isRejected
                          ? "bg-red-900/50 text-red-300"
                          : "bg-blue-900/50 text-blue-400"
                      }`}
                    >
                      {isRejected
                        ? "Rejected"
                        : d.status === "rolled-back"
                          ? "Rolled back"
                          : d.status === "failed"
                            ? "Failed"
                            : `${actions.length} action${actions.length !== 1 ? "s" : ""}`}
                    </span>
                    <span className="text-[9px] text-gray-600">
                      {isExpanded ? "▾" : "▸"}
                    </span>
                  </div>
                </button>
                {isExpanded && (
                  <div className="mt-2 space-y-1 border-t border-gray-700 pt-2">
                    {actions.map((a, i) => (
                      <div
                        key={i}
                        className="text-[10px] text-gray-400 flex items-start gap-1"
                      >
                        <span className="text-blue-400 mt-px">•</span>
                        <span>{describeAction(a)}</span>
                      </div>
                    ))}
                    {d.execution && (
                      <div className="rounded border border-gray-700 bg-gray-900/70 px-2 py-1 text-[10px] text-gray-400">
                        <p>
                          Execution: {d.execution.executed} applied
                          {d.execution.errors.length > 0 ? `, ${d.execution.errors.length} warning(s)` : ""}
                        </p>
                        {d.execution.errors.length > 0 && (
                          <div className="mt-0.5 space-y-0.5 text-red-300">
                            {d.execution.errors.map((error, index) => (
                              <p key={`${d.id}-exec-${index}`}>{error}</p>
                            ))}
                          </div>
                        )}
                      </div>
                    )}
                    {d.execution?.rollback && (
                      <div className="rounded border border-gray-700 bg-gray-900/70 px-2 py-1 text-[10px] text-gray-400">
                        <p>
                          Rollback:{" "}
                          {d.execution.rollback.supported
                            ? "available"
                            : d.execution.rollback.reason ?? "not available"}
                        </p>
                        {d.execution.rollback.steps.length > 0 && (
                          <div className="mt-0.5 space-y-0.5">
                            {d.execution.rollback.steps.map((step) => (
                              <p key={`${d.id}-rollback-${step.index}`}>
                                {step.supported ? "•" : "×"} {step.description}
                              </p>
                            ))}
                          </div>
                        )}
                      </div>
                    )}
                    {d.rollback && (
                      <div className="rounded border border-green-900/60 bg-green-950/30 px-2 py-1 text-[10px] text-green-200">
                        <p>Rolled back at {new Date(d.rollback.appliedAt).toLocaleString()}</p>
                        {d.rollback.errors.length > 0 && (
                          <div className="mt-0.5 space-y-0.5 text-red-300">
                            {d.rollback.errors.map((error, index) => (
                              <p key={`${d.id}-rollback-error-${index}`}>{error}</p>
                            ))}
                          </div>
                        )}
                      </div>
                    )}
                    {d.summary && (
                      <div className="mt-1.5 text-[10px] text-gray-500 leading-relaxed">
                        <Markdown content={d.summary} />
                      </div>
                    )}
                    {d.execution?.rollback.supported && d.status === "executed" && (
                      <div className="pt-1">
                        <button
                          onClick={() => void rollbackDecision(d)}
                          className="rounded border border-amber-700 px-2 py-1 text-[9px] text-amber-300 hover:bg-amber-900/30"
                        >
                          Roll back decision
                        </button>
                      </div>
                    )}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}
    </>
  );

  if (embedded) {
    return content;
  }

  return (
    <div className="flex w-72 shrink-0 flex-col border-r border-gray-800 bg-gray-900">
      {content}
    </div>
  );
}
