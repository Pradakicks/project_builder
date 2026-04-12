import { useDialogStore } from "../../store/useDialogStore";

export function ConfirmDialog() {
  const {
    open,
    title,
    message,
    details,
    confirmLabel,
    cancelLabel,
    onConfirm,
    close,
  } = useDialogStore();

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
      <div className="mx-4 w-full max-w-sm rounded-lg border border-gray-700 bg-gray-900 p-5 shadow-xl">
        <h2 className="text-sm font-semibold text-gray-100">{title}</h2>
        <p className="mt-3 text-sm text-gray-200 whitespace-pre-line">{message}</p>
        {details ? (
          <pre className="mt-3 max-h-48 overflow-y-auto rounded border border-gray-800 bg-gray-950 px-3 py-2 text-[11px] leading-relaxed text-gray-400 whitespace-pre-wrap">
            {details}
          </pre>
        ) : null}
        <div className="flex justify-end gap-2">
          <button
            onClick={close}
            className="rounded px-3 py-1.5 text-xs text-gray-400 hover:bg-gray-800 border border-gray-700"
          >
            {cancelLabel}
          </button>
          <button
            onClick={() => {
              onConfirm?.();
              close();
            }}
            className="rounded bg-red-600 px-3 py-1.5 text-xs text-white hover:bg-red-500"
          >
            {confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
