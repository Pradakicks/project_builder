import { create } from "zustand";

export interface AgentRunState {
  running: boolean;
  output: string;
  usage: { input: number; output: number } | null;
  exitCode?: number;
  phaseProposal?: string;
  phaseChanged?: string;
  gitBranch?: string;
  gitCommitSha?: string;
  gitDiffStat?: string;
  iterationCount?: number;
}

interface CompleteRunOpts {
  usage: { input: number; output: number };
  exitCode?: number;
  phaseProposal?: string;
  phaseChanged?: string;
  gitBranch?: string;
  gitCommitSha?: string;
  gitDiffStat?: string;
}

interface AgentStore {
  runs: Record<string, AgentRunState>;
  startRun: (pieceId: string) => void;
  startFeedbackRun: (pieceId: string) => void;
  appendChunk: (pieceId: string, chunk: string) => void;
  completeRun: (pieceId: string, opts: CompleteRunOpts) => void;
  clearPhaseProposal: (pieceId: string) => void;
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
  startFeedbackRun: (pieceId) => {
    const existing = get().runs[pieceId];
    const prevOutput = existing?.output ?? "";
    const iteration = (existing?.iterationCount ?? 1) + 1;
    set({
      runs: {
        ...get().runs,
        [pieceId]: {
          running: true,
          output: prevOutput + "\n\n--- Iteration " + iteration + " ---\n\n",
          usage: null,
          iterationCount: iteration,
        },
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
  completeRun: (pieceId, opts) => {
    const runs = get().runs;
    const run = runs[pieceId];
    if (!run) return;
    set({
      runs: {
        ...runs,
        [pieceId]: {
          ...run,
          running: false,
          usage: opts.usage,
          exitCode: opts.exitCode,
          phaseProposal: opts.phaseProposal,
          phaseChanged: opts.phaseChanged,
          gitBranch: opts.gitBranch,
          gitCommitSha: opts.gitCommitSha,
          gitDiffStat: opts.gitDiffStat,
        },
      },
    });
  },
  clearPhaseProposal: (pieceId) => {
    const runs = get().runs;
    const run = runs[pieceId];
    if (!run) return;
    set({
      runs: {
        ...runs,
        [pieceId]: { ...run, phaseProposal: undefined },
      },
    });
  },
}));
