import { create } from "zustand";

export interface AgentRunState {
  running: boolean;
  output: string;
  usage: { input: number; output: number } | null;
}

interface AgentStore {
  runs: Record<string, AgentRunState>;
  startRun: (pieceId: string) => void;
  appendChunk: (pieceId: string, chunk: string) => void;
  completeRun: (pieceId: string, usage: { input: number; output: number }) => void;
}

export const useAgentStore = create<AgentStore>((set, get) => ({
  runs: {},
  startRun: (pieceId) => {
    set({
      runs: {
        ...get().runs,
        [pieceId]: { running: true, output: "", usage: null },
      },
    });
  },
  appendChunk: (pieceId, chunk) => {
    const runs = get().runs;
    const run = runs[pieceId];
    if (!run) return;
    set({
      runs: {
        ...runs,
        [pieceId]: { ...run, output: run.output + chunk },
      },
    });
  },
  completeRun: (pieceId, usage) => {
    const runs = get().runs;
    const run = runs[pieceId];
    if (!run) return;
    set({
      runs: {
        ...runs,
        [pieceId]: { ...run, running: false, usage },
      },
    });
  },
}));
