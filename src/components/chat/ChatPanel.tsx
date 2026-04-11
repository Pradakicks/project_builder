import { useState, useRef, useEffect } from "react";
import { useChatStore } from "../../store/useChatStore";
import { useAppStore } from "../../store/useAppStore";
import { useProjectStore } from "../../store/useProjectStore";
import { useToastStore } from "../../store/useToastStore";
import { Markdown } from "../ui/Markdown";
import { devLog } from "../../utils/devLog";
import {
  parseActions,
  stripActionBlocks,
  describeAction,
  executeActions,
} from "../../utils/ctoActions";
import type { CtoDecision } from "../../types";

type Tab = "chat" | "decisions";

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
  const thread = useChatStore((s) =>
    projectId ? s.threads[projectId] : undefined,
  );
  const [input, setInput] = useState("");
  const [tab, setTab] = useState<Tab>("chat");
  const [decisions, setDecisions] = useState<CtoDecision[]>([]);
  const [expandedDecision, setExpandedDecision] = useState<string | null>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const messages = thread?.messages ?? [];
  const streaming = thread?.streaming ?? false;

  useEffect(() => {
    listRef.current?.scrollTo(0, listRef.current.scrollHeight);
  }, [messages.length, streaming]);

  useEffect(() => {
    setInput("");
    setDecisions([]);
    setExpandedDecision(null);
  }, [projectId]);

  // Load decisions when switching to decisions tab
  useEffect(() => {
    let cancelled = false;
    if (tab === "decisions" && project) {
      import("../../api/tauriApi").then(({ listCtoDecisions }) => {
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

  const send = async () => {
    const text = input.trim();
    if (!text || streaming || !project) return;
    devLog("info", "Chat", `Sending message (${text.length} chars)`);
    const conversation = messages
      .filter((m) => m.content)
      .map((m) => ({
        role: m.role === "user" ? "user" : "assistant",
        content: m.content,
      }));
    const requestId = crypto.randomUUID();
    useChatStore.getState().startRequest(project.id, text, requestId);
    setInput("");

    const originProjectId = project.id;
    const originProjectName = project.name;
    let streamBuffer = "";

    try {
      const { chatWithCto, onCtoChatChunk, logCtoDecision } = await import(
        "../../api/tauriApi"
      );

      const unlisten = await onCtoChatChunk((payload) => {
        if (
          payload.projectId !== originProjectId ||
          payload.requestId !== requestId
        ) {
          return;
        }

        if (payload.done) {
          // Parse and auto-execute actions
          const actions = parseActions(streamBuffer);
          const cleaned = actions.length > 0
            ? stripActionBlocks(streamBuffer)
            : streamBuffer;
          useChatStore
            .getState()
            .finalizeRequest(originProjectId, requestId, cleaned);

          if (actions.length > 0) {
            executeActions(actions, originProjectId).then(async (result) => {
              const addToast = useToastStore.getState().addToast;
              const isOriginProjectActive =
                useAppStore.getState().activeProjectId === originProjectId &&
                useProjectStore.getState().project?.id === originProjectId;

              if (result.executed > 0) {
                addToast(
                  isOriginProjectActive
                    ? `CTO applied ${result.executed} change${result.executed > 1 ? "s" : ""}`
                    : `CTO applied ${result.executed} change${result.executed > 1 ? "s" : ""} to "${originProjectName}"`,
                  "info",
                );
              }
              for (const err of result.errors) {
                addToast(
                  isOriginProjectActive ? err : `${originProjectName}: ${err}`,
                );
              }
              if (isOriginProjectActive && result.reloadCurrentProject) {
                await useProjectStore.getState().loadProject(originProjectId);
              }

              if (isOriginProjectActive && result.switchToTab) {
                onSwitchTab?.(result.switchToTab);
              }

              const summary =
                cleaned.slice(0, 500) ||
                actions.map((a) => describeAction(a)).join("; ");
              logCtoDecision(
                originProjectId,
                summary,
                JSON.stringify(actions),
              ).catch((e: unknown) => devLog("error", "Chat", "Failed to log CTO decision", e));
            });
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
          <div ref={listRef} className="flex-1 overflow-y-auto p-3 space-y-2">
            {messages.length === 0 && (
              <p className="text-[11px] text-gray-600 text-center mt-8">
                The CTO makes autonomous decisions about your project's
                architecture. Ask a question or describe what you need.
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
            const actions = (() => {
              try {
                return JSON.parse(d.actionsJson) as Array<{ action: string; [k: string]: unknown }>;
              } catch {
                return [];
              }
            })();
            const isExpanded = expandedDecision === d.id;
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
                    <span className="rounded bg-blue-900/50 px-1.5 py-0.5 text-[9px] text-blue-400 font-medium">
                      {actions.length} action{actions.length !== 1 ? "s" : ""}
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
                    {d.summary && (
                      <div className="mt-1.5 text-[10px] text-gray-500 leading-relaxed">
                        <Markdown content={d.summary} />
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
