import { useDebugStore } from "../store/useDebugStore";

const isDev = import.meta.env.DEV;

type LogLevel = "debug" | "info" | "warn" | "error" | "trace";

export function devLog(
  level: LogLevel,
  category: string,
  message: string,
  data?: unknown,
) {
  if (!isDev) return;
  useDebugStore.getState().recordEvent({
    kind: "frontend-log",
    level,
    category,
    message,
    data,
  });
  const ts = new Date().toISOString().slice(11, 23);
  const prefix = `[${ts}] [${category}]`;
  const method =
    level === "trace"
      ? "debug"
      : level === "error"
        ? "error"
        : level === "warn"
          ? "warn"
          : "log";
  if (data !== undefined) {
    console[method](prefix, message, data);
  } else {
    console[method](prefix, message);
  }
}
