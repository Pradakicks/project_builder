import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { devLog } from "../utils/devLog";
import { useDebugStore } from "../store/useDebugStore";

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
    const result = await invoke<T>(cmd, args);
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
  return listen<T>(eventName, (event) => {
    callback(event.payload);
  });
}
