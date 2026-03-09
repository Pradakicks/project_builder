import { create } from "zustand";

export interface Toast {
  id: string;
  message: string;
  type: "error" | "info";
}

interface ToastStore {
  toasts: Toast[];
  addToast: (message: string, type?: "error" | "info") => void;
  removeToast: (id: string) => void;
}

export const useToastStore = create<ToastStore>((set, get) => ({
  toasts: [],
  addToast: (message, type = "error") => {
    const id = crypto.randomUUID();
    set({ toasts: [...get().toasts, { id, message, type }] });
    setTimeout(() => get().removeToast(id), 4000);
  },
  removeToast: (id) => {
    set({ toasts: get().toasts.filter((t) => t.id !== id) });
  },
}));
