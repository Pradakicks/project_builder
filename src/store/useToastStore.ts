import { create } from "zustand";

export interface Toast {
  id: string;
  message: string;
  type: "error" | "info" | "warning";
}

/// Persisted record of a toast — outlives the active-display lifetime so the
/// debug report can include them. Active toasts auto-dismiss after 4s; history
/// keeps the last N regardless of whether they were manually dismissed.
export interface ToastHistoryEntry {
  id: string;
  message: string;
  type: "error" | "info" | "warning";
  createdAt: string; // ISO
}

const HISTORY_CAP = 50;

interface ToastStore {
  toasts: Toast[];
  history: ToastHistoryEntry[];
  addToast: (message: string, type?: "error" | "info" | "warning") => void;
  removeToast: (id: string) => void;
  getHistory: () => ToastHistoryEntry[];
}

export const useToastStore = create<ToastStore>((set, get) => ({
  toasts: [],
  history: [],
  addToast: (message, type = "error") => {
    const id = crypto.randomUUID();
    const createdAt = new Date().toISOString();
    const entry: ToastHistoryEntry = { id, message, type, createdAt };
    set((state) => {
      const nextHistory = [...state.history, entry];
      const trimmed =
        nextHistory.length > HISTORY_CAP
          ? nextHistory.slice(nextHistory.length - HISTORY_CAP)
          : nextHistory;
      return {
        toasts: [...state.toasts, { id, message, type }],
        history: trimmed,
      };
    });
    setTimeout(() => get().removeToast(id), 4000);
  },
  removeToast: (id) => {
    set({ toasts: get().toasts.filter((t) => t.id !== id) });
  },
  getHistory: () => get().history,
}));
