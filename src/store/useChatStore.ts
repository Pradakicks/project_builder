import { create } from "zustand";
import { devLog } from "../utils/devLog";

export interface ChatMessage {
  id: string;
  role: "user" | "agent";
  content: string;
  timestamp: number;
}

interface ChatStore {
  messages: ChatMessage[];
  addMessage: (role: "user" | "agent", content: string) => void;
}

export const useChatStore = create<ChatStore>((set, get) => ({
  messages: [],
  addMessage: (role, content) => {
    const msg: ChatMessage = {
      id: crypto.randomUUID(),
      role,
      content,
      timestamp: Date.now(),
    };
    devLog("debug", "Store:Chat", `${role} message added (${content.length} chars)`);
    set({ messages: [...get().messages, msg] });
  },
}));
