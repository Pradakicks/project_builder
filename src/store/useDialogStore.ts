import { create } from "zustand";

interface DialogStore {
  open: boolean;
  message: string;
  details: string;
  title: string;
  confirmLabel: string;
  cancelLabel: string;
  onConfirm: (() => void) | null;
  showConfirm: (
    message: string,
    onConfirm: () => void,
    options?: {
      title?: string;
      details?: string;
      confirmLabel?: string;
      cancelLabel?: string;
    },
  ) => void;
  close: () => void;
}

export const useDialogStore = create<DialogStore>((set) => ({
  open: false,
  message: "",
  details: "",
  title: "Confirm action",
  confirmLabel: "Confirm",
  cancelLabel: "Cancel",
  onConfirm: null,
  showConfirm: (message, onConfirm, options) =>
    set({
      open: true,
      message,
      details: options?.details ?? "",
      title: options?.title ?? "Confirm action",
      confirmLabel: options?.confirmLabel ?? "Confirm",
      cancelLabel: options?.cancelLabel ?? "Cancel",
      onConfirm,
    }),
  close: () =>
    set({
      open: false,
      message: "",
      details: "",
      title: "Confirm action",
      confirmLabel: "Confirm",
      cancelLabel: "Cancel",
      onConfirm: null,
    }),
}));
