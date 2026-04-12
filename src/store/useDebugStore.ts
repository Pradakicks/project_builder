import { create } from "zustand";
import type {
  CapturedScenario,
  DebugEvent,
  DebugReport,
  DebugSessionSummary,
} from "../types";

const isDev = import.meta.env.DEV;
const MAX_EVENTS = 250;
const EVENTS_KEY = "project-builder.debug.events";
const SCENARIO_KEY = "project-builder.debug.last-scenario";

type ReplayHandler = (scenario: CapturedScenario) => Promise<void>;

interface DebugStore {
  events: DebugEvent[];
  session: DebugSessionSummary | null;
  lastScenario: CapturedScenario | null;
  diagnosticsOpen: boolean;
  replayHandler: ReplayHandler | null;
  recordEvent: (event: Omit<DebugEvent, "id" | "timestamp">) => void;
  setSession: (session: DebugSessionSummary | null) => void;
  setDiagnosticsOpen: (open: boolean) => void;
  captureScenario: (scenario: CapturedScenario) => void;
  clearScenario: () => void;
  registerReplayHandler: (handler: ReplayHandler | null) => void;
  buildReport: (activeProjectId: string | null, activeView: string) => DebugReport;
}

function readJson<T>(key: string): T | null {
  if (!isDev || typeof window === "undefined") return null;
  try {
    const raw = window.localStorage.getItem(key);
    return raw ? (JSON.parse(raw) as T) : null;
  } catch {
    return null;
  }
}

function writeJson(key: string, value: unknown) {
  if (!isDev || typeof window === "undefined") return;
  try {
    window.localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // Ignore localStorage failures in dev diagnostics.
  }
}

const initialEvents = readJson<DebugEvent[]>(EVENTS_KEY) ?? [];
const initialScenario = readJson<CapturedScenario>(SCENARIO_KEY);

export const useDebugStore = create<DebugStore>((set, get) => ({
  events: initialEvents,
  session: null,
  lastScenario: initialScenario,
  diagnosticsOpen: false,
  replayHandler: null,
  recordEvent: (event) => {
    if (!isDev) return;
    const nextEvent: DebugEvent = {
      ...event,
      id: crypto.randomUUID(),
      timestamp: new Date().toISOString(),
    };
    set((state) => {
      const events = [...state.events, nextEvent].slice(-MAX_EVENTS);
      writeJson(EVENTS_KEY, events);
      return { events };
    });
  },
  setSession: (session) => set({ session }),
  setDiagnosticsOpen: (diagnosticsOpen) => set({ diagnosticsOpen }),
  captureScenario: (scenario) => {
    if (!isDev) return;
    writeJson(SCENARIO_KEY, scenario);
    set({ lastScenario: scenario });
    get().recordEvent({
      kind: "scenario",
      level: scenario.status === "failed" ? "error" : "warn",
      category: "Diagnostics",
      message: `Captured ${scenario.kind} scenario (${scenario.status})`,
      data: {
        scenarioId: scenario.id,
        projectId: scenario.projectId,
        error: scenario.error,
      },
    });
  },
  clearScenario: () => {
    if (isDev && typeof window !== "undefined") {
      window.localStorage.removeItem(SCENARIO_KEY);
    }
    set({ lastScenario: null });
  },
  registerReplayHandler: (handler) => set({ replayHandler: handler }),
  buildReport: (activeProjectId, activeView) => ({
    generatedAt: new Date().toISOString(),
    session: get().session,
    activeProjectId,
    activeView,
    lastScenario: get().lastScenario,
    recentEvents: get().events.slice(-50),
  }),
}));
