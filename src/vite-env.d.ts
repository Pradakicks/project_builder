/// <reference types="vite/client" />

type ProjectBuilderE2eHarness = {
  invoke: <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
  listen?: <T>(
    eventName: string,
    callback: (payload: T) => void,
  ) => Promise<() => void>;
  snapshot?: () => unknown;
};

interface Window {
  __PROJECT_BUILDER_E2E__?: ProjectBuilderE2eHarness;
}
