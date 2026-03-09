import { useDialogStore } from "../../store/useDialogStore";

export function ConfirmDialog() {
  const { open, message, onConfirm, close } = useDialogStore();

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
      <div className="mx-4 w-full max-w-sm rounded-lg border border-gray-700 bg-gray-900 p-5 shadow-xl">
        <p className="text-sm text-gray-200 mb-5">{message}</p>
        <div className="flex justify-end gap-2">
          <button
            onClick={close}
            className="rounded px-3 py-1.5 text-xs text-gray-400 hover:bg-gray-800 border border-gray-700"
          >
            Cancel
          </button>
          <button
            onClick={() => {
              onConfirm?.();
              close();
            }}
            className="rounded bg-red-600 px-3 py-1.5 text-xs text-white hover:bg-red-500"
          >
            Confirm
          </button>
        </div>
      </div>
    </div>
  );
}
