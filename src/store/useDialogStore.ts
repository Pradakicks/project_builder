import { create } from "zustand";

interface DialogStore {
  open: boolean;
  message: string;
  onConfirm: (() => void) | null;
  showConfirm: (message: string, onConfirm: () => void) => void;
  close: () => void;
}

export const useDialogStore = create<DialogStore>((set) => ({
  open: false,
  message: "",
  onConfirm: null,
  showConfirm: (message, onConfirm) => set({ open: true, message, onConfirm }),
  close: () => set({ open: false, message: "", onConfirm: null }),
}));
