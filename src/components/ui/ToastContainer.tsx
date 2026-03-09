import { useToastStore } from "../../store/useToastStore";

export function ToastContainer() {
  const { toasts, removeToast } = useToastStore();

  if (toasts.length === 0) return null;

  return (
    <div className="fixed bottom-4 right-4 z-50 flex flex-col gap-2">
      {toasts.map((toast) => (
        <div
          key={toast.id}
          className={`flex items-center gap-2 rounded-lg px-4 py-2.5 text-xs shadow-lg ${
            toast.type === "error"
              ? "bg-red-900/90 text-red-100 border border-red-700"
              : "bg-gray-800/90 text-gray-100 border border-gray-700"
          }`}
        >
          <span className="flex-1">{toast.message}</span>
          <button
            onClick={() => removeToast(toast.id)}
            className="text-gray-400 hover:text-gray-200 shrink-0"
          >
            &times;
          </button>
        </div>
      ))}
    </div>
  );
}
