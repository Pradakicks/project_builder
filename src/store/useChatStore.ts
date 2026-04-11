import { create } from "zustand";
import { devLog } from "../utils/devLog";

export interface ChatMessage {
  id: string;
  role: "user" | "agent";
  content: string;
  timestamp: number;
}

export interface ChatThread {
  messages: ChatMessage[];
  streaming: boolean;
  activeRequestId: string | null;
  activeAgentMessageId: string | null;
}

interface ChatStore {
  threads: Record<string, ChatThread>;
  startRequest: (projectId: string, userContent: string, requestId: string) => void;
  appendChunk: (projectId: string, requestId: string, chunk: string) => void;
  finalizeRequest: (
    projectId: string,
    requestId: string,
    finalContent?: string,
  ) => void;
  failRequest: (projectId: string, requestId: string, message: string) => void;
  getConversation: (projectId: string) => ChatMessage[];
}

function getOrCreateThread(
  threads: Record<string, ChatThread>,
  projectId: string,
): ChatThread {
  return (
    threads[projectId] ?? {
      messages: [],
      streaming: false,
      activeRequestId: null,
      activeAgentMessageId: null,
    }
  );
}

export const useChatStore = create<ChatStore>((set, get) => ({
  threads: {},
  startRequest: (projectId, userContent, requestId) => {
    const userMessage: ChatMessage = {
      id: crypto.randomUUID(),
      role: "user",
      content: userContent,
      timestamp: Date.now(),
    };
    const agentMessage: ChatMessage = {
      id: crypto.randomUUID(),
      role: "agent",
      content: "",
      timestamp: Date.now(),
    };
    devLog(
      "debug",
      "Store:Chat",
      `Starting CTO request for project ${projectId} (${userContent.length} chars)`,
    );
    set((state) => {
      const thread = getOrCreateThread(state.threads, projectId);
      return {
        threads: {
          ...state.threads,
          [projectId]: {
            ...thread,
            messages: [...thread.messages, userMessage, agentMessage],
            streaming: true,
            activeRequestId: requestId,
            activeAgentMessageId: agentMessage.id,
          },
        },
      };
    });
  },
  appendChunk: (projectId, requestId, chunk) => {
    set((state) => {
      const thread = state.threads[projectId];
      if (!thread || thread.activeRequestId !== requestId || !thread.activeAgentMessageId) {
        return state;
      }
      return {
        threads: {
          ...state.threads,
          [projectId]: {
            ...thread,
            messages: thread.messages.map((message) =>
              message.id === thread.activeAgentMessageId
                ? { ...message, content: message.content + chunk }
                : message,
            ),
          },
        },
      };
    });
  },
  finalizeRequest: (projectId, requestId, finalContent) => {
    set((state) => {
      const thread = state.threads[projectId];
      if (!thread || thread.activeRequestId !== requestId) {
        return state;
      }
      const nextMessages = !thread.activeAgentMessageId
        ? thread.messages
        : thread.messages.map((message) =>
            message.id === thread.activeAgentMessageId && finalContent !== undefined
              ? { ...message, content: finalContent }
              : message,
          );
      return {
        threads: {
          ...state.threads,
          [projectId]: {
            ...thread,
            messages: nextMessages,
            streaming: false,
            activeRequestId: null,
            activeAgentMessageId: null,
          },
        },
      };
    });
  },
  failRequest: (projectId, requestId, message) => {
    devLog("warn", "Store:Chat", `CTO request failed for project ${projectId}`);
    get().finalizeRequest(projectId, requestId, message);
  },
  getConversation: (projectId) => {
    const thread = get().threads[projectId];
    return thread?.messages.filter((message) => message.content) ?? [];
  },
}));
