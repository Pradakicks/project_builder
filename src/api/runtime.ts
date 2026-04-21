import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { devLog } from "../utils/devLog";
import { useDebugStore } from "../store/useDebugStore";

type E2eHarness = {
  invoke: <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
  listen?: <T>(
    eventName: string,
    callback: (payload: T) => void,
  ) => Promise<UnlistenFn>;
};

function getE2eHarness(): E2eHarness | null {
  if (!import.meta.env.VITE_E2E_HARNESS) return null;
  if (typeof window === "undefined") return null;
  return window.__PROJECT_BUILDER_E2E__ ?? null;
}

export async function loggedInvoke<T>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T> {
  devLog("debug", "IPC", `→ ${cmd}`, args);
  useDebugStore.getState().recordEvent({
    kind: "ipc-invoke",
    level: "debug",
    category: "IPC",
    message: cmd,
    data: args,
  });
  const start = performance.now();
  try {
    const harness = getE2eHarness();
    const result = harness
      ? await harness.invoke<T>(cmd, args)
      : await invoke<T>(cmd, args);
    const durationMs = Number((performance.now() - start).toFixed(0));
    devLog(
      "debug",
      "IPC",
      `← ${cmd} (${durationMs}ms)`,
    );
    useDebugStore.getState().recordEvent({
      kind: "ipc-result",
      level: "debug",
      category: "IPC",
      message: `${cmd} ok`,
      data: { durationMs },
    });
    return result;
  } catch (e) {
    const durationMs = Number((performance.now() - start).toFixed(0));
    devLog(
      "error",
      "IPC",
      `✗ ${cmd} (${durationMs}ms)`,
      e,
    );
    useDebugStore.getState().recordEvent({
      kind: "ipc-error",
      level: "error",
      category: "IPC",
      message: `${cmd} failed`,
      data: { args, durationMs, error: String(e) },
    });
    throw e;
  }
}

export async function listenToEvent<T>(
  eventName: string,
  callback: (payload: T) => void,
): Promise<UnlistenFn> {
  const harness = getE2eHarness();
  if (harness?.listen) {
    return harness.listen(eventName, callback);
  }
  return listen<T>(eventName, (event) => {
    callback(event.payload);
  });
}
