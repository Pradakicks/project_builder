import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { devLog } from "../utils/devLog";

export async function loggedInvoke<T>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T> {
  devLog("debug", "IPC", `→ ${cmd}`, args);
  const start = performance.now();
  try {
    const result = await invoke<T>(cmd, args);
    devLog(
      "debug",
      "IPC",
      `← ${cmd} (${(performance.now() - start).toFixed(0)}ms)`,
    );
    return result;
  } catch (e) {
    devLog(
      "error",
      "IPC",
      `✗ ${cmd} (${(performance.now() - start).toFixed(0)}ms)`,
      e,
    );
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
