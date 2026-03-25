import { useState, useRef, useEffect } from "react";
import { useChatStore } from "../../store/useChatStore";
import { useProjectStore } from "../../store/useProjectStore";
import { useToastStore } from "../../store/useToastStore";
import { Markdown } from "../ui/Markdown";
import {
  parseActions,
  stripActionBlocks,
  describeAction,
  executeActions,
  type CtoAction,
} from "../../utils/ctoActions";

export function ChatPanel({
  open,
  onToggle,
  embedded,
}: {
  open: boolean;
  onToggle: () => void;
  embedded?: boolean;
}) {
  const { messages, addMessage } = useChatStore();
  const project = useProjectStore((s) => s.project);
  const [input, setInput] = useState("");
  const [streaming, setStreaming] = useState(false);
  const [pendingActions, setPendingActions] = useState<CtoAction[] | null>(null);
  const [applying, setApplying] = useState(false);
  const listRef = useRef<HTMLDivElement>(null);
  const streamBufferRef = useRef("");

  useEffect(() => {
    listRef.current?.scrollTo(0, listRef.current.scrollHeight);
  }, [messages.length, streaming, pendingActions]);

  const send = async () => {
    const text = input.trim();
    if (!text || streaming || !project) return;
    addMessage("user", text);
    setInput("");
    setStreaming(true);
    setPendingActions(null);
    streamBufferRef.current = "";

    // Add a placeholder agent message
    addMessage("agent", "");

    try {
      const { chatWithCto, onCtoChatChunk } = await import("../../api/tauriApi");

      // Build conversation history (exclude the empty placeholder)
      const conversation = messages
        .filter((m) => m.content)
        .map((m) => ({
          role: m.role === "user" ? "user" : "assistant",
          content: m.content,
        }));

      const unlisten = await onCtoChatChunk((payload) => {
        if (payload.done) {
          setStreaming(false);
          // Parse actions from completed response
          const actions = parseActions(streamBufferRef.current);
          if (actions.length > 0) {
            setPendingActions(actions);
            // Strip action blocks from displayed message
            const cleaned = stripActionBlocks(streamBufferRef.current);
            const store = useChatStore.getState();
            const msgs = [...store.messages];
            const lastIdx = msgs.length - 1;
            if (lastIdx >= 0 && msgs[lastIdx].role === "agent") {
              msgs[lastIdx] = { ...msgs[lastIdx], content: cleaned };
              useChatStore.setState({ messages: msgs });
            }
          }
          unlisten();
        } else {
          streamBufferRef.current += payload.chunk;
          // Update the last agent message
          const store = useChatStore.getState();
          const msgs = [...store.messages];
          const lastIdx = msgs.length - 1;
          if (lastIdx >= 0 && msgs[lastIdx].role === "agent") {
            msgs[lastIdx] = { ...msgs[lastIdx], content: streamBufferRef.current };
            useChatStore.setState({ messages: msgs });
          }
        }
      });

      await chatWithCto(project.id, text, conversation);
    } catch (e) {
      setStreaming(false);
      useToastStore.getState().addToast(`CTO chat error: ${e}`);
      // Update the placeholder with error
      const store = useChatStore.getState();
      const msgs = [...store.messages];
      const lastIdx = msgs.length - 1;
      if (lastIdx >= 0 && msgs[lastIdx].role === "agent" && !msgs[lastIdx].content) {
        msgs[lastIdx] = { ...msgs[lastIdx], content: "(Failed to connect to LLM)" };
        useChatStore.setState({ messages: msgs });
      }
    }
  };

  const handleApplyActions = async () => {
    if (!pendingActions || !project) return;
    setApplying(true);
    const addToast = useToastStore.getState().addToast;
    try {
      const result = await executeActions(pendingActions, project.id);
      if (result.executed > 0) {
        addToast(`Applied ${result.executed} change${result.executed > 1 ? "s" : ""}`, "info");
      }
      for (const err of result.errors) {
        addToast(err);
      }
      // Reload project to reflect changes
      await useProjectStore.getState().loadProject(project.id);
    } catch (e) {
      addToast(`Failed to apply changes: ${e}`);
    }
    setPendingActions(null);
    setApplying(false);
  };

  if (!open) {
    return (
      <button
        onClick={onToggle}
        className="absolute left-2 top-14 z-10 rounded-lg border border-gray-700 bg-gray-900 p-2 text-gray-400 hover:text-gray-200 shadow-lg"
        title="Open CTO Agent"
      >
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
          <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" />
        </svg>
      </button>
    );
  }

  const content = (
    <>
      {!embedded && (
        <div className="flex items-center justify-between border-b border-gray-800 px-3 py-2">
          <span className="text-xs font-semibold text-gray-300">CTO Agent</span>
          <button
            onClick={onToggle}
            className="text-xs text-gray-500 hover:text-gray-300"
          >
            Collapse
          </button>
        </div>
      )}

      <div ref={listRef} className="flex-1 overflow-y-auto p-3 space-y-2">
        {messages.length === 0 && (
          <p className="text-[11px] text-gray-600 text-center mt-8">
            Ask the CTO agent about your project's architecture, design, or implementation strategy.
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
                    <span className="w-1.5 h-1.5 bg-blue-400 rounded-full animate-bounce" style={{animationDelay: '0ms'}} />
                    <span className="w-1.5 h-1.5 bg-blue-400 rounded-full animate-bounce" style={{animationDelay: '150ms'}} />
                    <span className="w-1.5 h-1.5 bg-blue-400 rounded-full animate-bounce" style={{animationDelay: '300ms'}} />
                  </span>
                ) : null
              ) : (
                msg.content || ""
              )}
            </div>
          </div>
        ))}

        {/* CTO Action Card */}
        {pendingActions && pendingActions.length > 0 && (
          <div className="rounded-lg border border-blue-500/30 bg-blue-950/30 p-2.5">
            <p className="text-[11px] font-medium text-blue-300 mb-1.5">
              CTO wants to make {pendingActions.length} change{pendingActions.length > 1 ? "s" : ""}:
            </p>
            <ul className="space-y-0.5 mb-2">
              {pendingActions.map((action, i) => (
                <li key={i} className="text-[10px] text-gray-400 flex items-start gap-1">
                  <span className="text-blue-400 mt-px">•</span>
                  <span>{describeAction(action)}</span>
                </li>
              ))}
            </ul>
            <div className="flex gap-1.5">
              <button
                onClick={handleApplyActions}
                disabled={applying}
                className="rounded bg-blue-600 px-2.5 py-1 text-[10px] font-medium text-white hover:bg-blue-500 disabled:opacity-50"
              >
                {applying ? "Applying..." : "Apply"}
              </button>
              <button
                onClick={() => setPendingActions(null)}
                disabled={applying}
                className="rounded border border-gray-700 px-2.5 py-1 text-[10px] text-gray-400 hover:bg-gray-800 disabled:opacity-50"
              >
                Skip
              </button>
            </div>
          </div>
        )}
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
