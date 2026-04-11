import { create } from "zustand";
import { devLog } from "../utils/devLog";
import type { ValidationResult, TokenUsage } from "../types";

export interface AgentRunState {
  running: boolean;
  output: string;
  usage: TokenUsage | null;
  success?: boolean;
  exitCode?: number;
  phaseProposal?: string;
  phaseChanged?: string;
  gitBranch?: string;
  gitCommitSha?: string;
  gitDiffStat?: string;
  iterationCount?: number;
  validation?: ValidationResult;
  validationOutput?: string;
}

interface CompleteRunOpts {
  usage: TokenUsage;
  success?: boolean;
  exitCode?: number;
  phaseProposal?: string;
  phaseChanged?: string;
  gitBranch?: string;
  gitCommitSha?: string;
  gitDiffStat?: string;
  validation?: ValidationResult;
  validationOutput?: string;
}

interface AgentStore {
  runs: Record<string, AgentRunState>;
  startRun: (pieceId: string) => void;
  startFeedbackRun: (pieceId: string) => void;
  appendChunk: (pieceId: string, chunk: string) => void;
  appendValidationChunk: (pieceId: string, chunk: string) => void;
  completeRun: (pieceId: string, opts: CompleteRunOpts) => void;
  restoreRun: (pieceId: string, run: AgentRunState) => void;
  clearPhaseProposal: (pieceId: string) => void;
}

export const useAgentStore = create<AgentStore>((set, get) => ({
  runs: {},
  startRun: (pieceId) => {
    devLog("info", "Store:Agent", `Starting agent run for piece ${pieceId}`);
    set({
      runs: {
        ...get().runs,
        [pieceId]: { running: true, output: "", usage: null, validationOutput: "" },
      },
    });
  },
  startFeedbackRun: (pieceId) => {
    const existing = get().runs[pieceId];
    const prevOutput = existing?.output ?? "";
    const iteration = (existing?.iterationCount ?? 1) + 1;
    devLog("info", "Store:Agent", `Starting feedback run #${iteration} for piece ${pieceId}`);
    set({
      runs: {
        ...get().runs,
        [pieceId]: {
          running: true,
          output: prevOutput + "\n\n--- Iteration " + iteration + " ---\n\n",
          usage: null,
          iterationCount: iteration,
          validation: undefined,
          validationOutput: "",
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
  appendValidationChunk: (pieceId, chunk) => {
    const runs = get().runs;
    const run = runs[pieceId];
    if (!run) return;
    set({
      runs: {
        ...runs,
        [pieceId]: {
          ...run,
          validationOutput: (run.validationOutput ?? "") + chunk,
        },
      },
    });
  },
  completeRun: (pieceId, opts) => {
    devLog("info", "Store:Agent", `Agent run complete for piece ${pieceId}`, {
      success: opts.success,
      exitCode: opts.exitCode,
      tokens: opts.usage,
      gitBranch: opts.gitBranch,
    });
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
          success: opts.success,
          exitCode: opts.exitCode,
          phaseProposal: opts.phaseProposal,
          phaseChanged: opts.phaseChanged,
          gitBranch: opts.gitBranch,
          gitCommitSha: opts.gitCommitSha,
          gitDiffStat: opts.gitDiffStat,
          validation: opts.validation,
          validationOutput: opts.validationOutput ?? run.validationOutput,
        },
      },
    });
  },
  restoreRun: (pieceId, run) => {
    const existing = get().runs[pieceId];
    if (existing?.running) return;
    set({
      runs: {
        ...get().runs,
        [pieceId]: run,
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
