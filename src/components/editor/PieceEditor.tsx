import { useState, useEffect, useCallback, useMemo, useRef } from "react";
import { useProjectStore } from "../../store/useProjectStore";
import { useDialogStore } from "../../store/useDialogStore";
import { useAgentStore } from "../../store/useAgentStore";
import { useToastStore } from "../../store/useToastStore";
import type {
  Piece,
  PieceInterface,
  Constraint,
  Phase,
} from "../../types";
import { AgentPromptEditor } from "./AgentPromptEditor";
import { ReferenceSuggestions } from "./ReferenceSuggestions";
import { PillSelect } from "../ui/PillSelect";
import { SelectWithOther } from "../ui/SelectWithOther";
import { debounce } from "../../utils/debounce";
import { useAtReference } from "../../hooks/useAtReference";

type Tab = "general" | "interfaces" | "constraints" | "notes" | "agent";

const tabs: { id: Tab; label: string }[] = [
  { id: "general", label: "General" },
  { id: "interfaces", label: "Inputs & Outputs" },
  { id: "constraints", label: "Requirements" },
  { id: "notes", label: "Notes" },
  { id: "agent", label: "Agent" },
];

const phaseOptions: { value: Phase; label: string }[] = [
  { value: "design", label: "Design" },
  { value: "review", label: "Review" },
  { value: "approved", label: "Approved" },
  { value: "implementing", label: "Implementing" },
];

const phaseColors: Record<Phase, string> = {
  design: "bg-yellow-500/30 text-yellow-300",
  review: "bg-purple-500/30 text-purple-300",
  approved: "bg-green-500/30 text-green-300",
  implementing: "bg-blue-500/30 text-blue-300",
};

const providerPresets = ["Claude", "OpenAI", "Google", "Local"];

const modelPresets: Record<string, string[]> = {
  Claude: ["claude-opus-4-6", "claude-sonnet-4-6", "claude-haiku-4-5"],
  OpenAI: ["gpt-4o", "gpt-4o-mini", "o1", "o3-mini"],
  Google: ["gemini-2.0-flash", "gemini-2.5-pro"],
  Local: ["ollama", "lm-studio"],
};

export function PieceEditor({ pieceId }: { pieceId: string }) {
  const { pieces, updatePiece, deletePiece, selectPiece } = useProjectStore();
  const showConfirm = useDialogStore((s) => s.showConfirm);
  const piece = pieces.find((p) => p.id === pieceId);
  const [activeTab, setActiveTab] = useState<Tab>("general");

  if (!piece) return null;

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center justify-between border-b border-gray-800 px-4 py-2">
        <h2 className="text-sm font-semibold text-gray-200 truncate">
          {piece.name}
        </h2>
        <div className="flex gap-1">
          <button
            onClick={() =>
              showConfirm(`Delete piece "${piece.name}"?`, async () => {
                await deletePiece(piece.id);
                selectPiece(null);
              })
            }
            className="rounded px-2 py-1 text-xs text-red-400 hover:bg-red-900/30"
          >
            Delete
          </button>
          <button
            onClick={() => selectPiece(null)}
            className="rounded px-2 py-1 text-xs text-gray-400 hover:bg-gray-800"
          >
            Close
          </button>
        </div>
      </div>

      <div className="flex border-b border-gray-800">
        {tabs.map((tab) => (
          <button
            key={tab.id}
            onClick={() => setActiveTab(tab.id)}
            className={`px-3 py-1.5 text-xs font-medium transition-colors ${
              activeTab === tab.id
                ? "border-b-2 border-blue-500 text-blue-400"
                : "text-gray-500 hover:text-gray-300"
            }`}
          >
            {tab.label}
          </button>
        ))}
      </div>

      <div className="flex-1 overflow-y-auto p-4">
        {activeTab === "general" && (
          <GeneralTab piece={piece} onUpdate={updatePiece} />
        )}
        {activeTab === "interfaces" && (
          <InterfacesTab piece={piece} onUpdate={updatePiece} />
        )}
        {activeTab === "constraints" && (
          <ConstraintsTab piece={piece} onUpdate={updatePiece} />
        )}
        {activeTab === "notes" && (
          <NotesTab piece={piece} onUpdate={updatePiece} />
        )}
        {activeTab === "agent" && (
          <AgentTab piece={piece} onUpdate={updatePiece} />
        )}
      </div>
    </div>
  );
}

// ── Field Components ─────────────────────────────────────

function FieldLabel({ children }: { children: React.ReactNode }) {
  return <label className="block text-xs font-medium text-gray-400 mb-1">{children}</label>;
}

function TextInput({
  value,
  onChange,
  placeholder,
}: {
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
}) {
  return (
    <input
      type="text"
      value={value}
      onChange={(e) => onChange(e.target.value)}
      placeholder={placeholder}
      className="w-full rounded border border-gray-700 bg-gray-800 px-2 py-1.5 text-xs text-gray-100 placeholder-gray-600 focus:border-blue-500 focus:outline-none"
    />
  );
}

function TextArea({
  value,
  onChange,
  placeholder,
  rows = 4,
}: {
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  rows?: number;
}) {
  return (
    <textarea
      value={value}
      onChange={(e) => onChange(e.target.value)}
      placeholder={placeholder}
      rows={rows}
      className="w-full rounded border border-gray-700 bg-gray-800 px-2 py-1.5 text-xs text-gray-100 placeholder-gray-600 focus:border-blue-500 focus:outline-none resize-none font-mono"
    />
  );
}

// ── Tabs ─────────────────────────────────────────────────

function GeneralTab({
  piece,
  onUpdate,
}: {
  piece: Piece;
  onUpdate: (id: string, updates: Record<string, unknown>) => Promise<void>;
}) {
  const [name, setName] = useState(piece.name);
  const [pieceType, setPieceType] = useState(piece.pieceType);
  const [color, setColor] = useState(piece.color ?? "#3b82f6");
  const [responsibilities, setResponsibilities] = useState(piece.responsibilities);
  const responsibilitiesRef = useRef<HTMLTextAreaElement>(null);
  const refHook = useAtReference(responsibilitiesRef, responsibilities);

  useEffect(() => {
    setName(piece.name);
    setPieceType(piece.pieceType);
    setColor(piece.color ?? "#3b82f6");
    setResponsibilities(piece.responsibilities);
  }, [piece.id, piece.name, piece.pieceType, piece.color, piece.responsibilities]);

  const save = useMemo(
    () =>
      debounce((field: string, value: unknown) => {
        onUpdate(piece.id, { [field]: value });
      }, 300),
    [piece.id, onUpdate],
  );

  return (
    <div className="flex flex-col gap-3">
      <div>
        <FieldLabel>Name</FieldLabel>
        <TextInput
          value={name}
          onChange={(v) => {
            setName(v);
            save("name", v);
          }}
        />
      </div>
      <div>
        <FieldLabel>Type / Role</FieldLabel>
        <TextInput
          value={pieceType}
          onChange={(v) => {
            setPieceType(v);
            save("pieceType", v);
          }}
          placeholder="e.g. Backend Service, Frontend Component"
        />
      </div>
      <div>
        <FieldLabel>Color</FieldLabel>
        <div className="flex items-center gap-2">
          <input
            type="color"
            value={color}
            onChange={(e) => {
              setColor(e.target.value);
              save("color", e.target.value);
            }}
            className="h-7 w-7 rounded border border-gray-700 bg-transparent cursor-pointer"
          />
          <span className="text-xs text-gray-500">{color}</span>
        </div>
      </div>
      <div>
        <FieldLabel>Phase</FieldLabel>
        <PillSelect<Phase>
          value={piece.phase}
          options={phaseOptions}
          colorMap={phaseColors}
          onChange={(v) => save("phase", v)}
        />
      </div>
      <div>
        <FieldLabel>Responsibilities</FieldLabel>
        <div className="relative">
          <textarea
            ref={responsibilitiesRef}
            value={responsibilities}
            onChange={(e) =>
              refHook.handleChange(e, (v) => {
                setResponsibilities(v);
                save("responsibilities", v);
              })
            }
            placeholder="What does this piece do? Use @PieceName to reference others."
            rows={4}
            className="w-full rounded border border-gray-700 bg-gray-800 px-2 py-1.5 text-xs text-gray-100 placeholder-gray-600 focus:border-blue-500 focus:outline-none resize-none font-mono"
          />
          <ReferenceSuggestions
            show={refHook.showSuggestions}
            suggestions={refHook.suggestions}
            onSelect={(name) =>
              refHook.insertReference(name, (v) => {
                setResponsibilities(v);
                save("responsibilities", v);
              })
            }
          />
        </div>
      </div>
    </div>
  );
}

function InterfacesTab({
  piece,
  onUpdate,
}: {
  piece: Piece;
  onUpdate: (id: string, updates: Record<string, unknown>) => Promise<void>;
}) {
  const [interfaces, setInterfaces] = useState<PieceInterface[]>(piece.interfaces);

  useEffect(() => {
    setInterfaces(piece.interfaces);
  }, [piece.id, piece.interfaces]);

  const debouncedUpdate = useMemo(
    () => debounce((updated: PieceInterface[]) => onUpdate(piece.id, { interfaces: updated }), 300),
    [piece.id, onUpdate],
  );

  const save = (updated: PieceInterface[]) => {
    setInterfaces(updated);
    debouncedUpdate(updated);
  };

  const addInterface = () => {
    save([
      ...interfaces,
      { name: "", direction: "in" as const, description: "" },
    ]);
  };

  const removeInterface = (index: number) => {
    save(interfaces.filter((_, i) => i !== index));
  };

  const updateInterface = (index: number, field: keyof PieceInterface, value: string) => {
    const updated = interfaces.map((iface, i) =>
      i === index ? { ...iface, [field]: value } : iface,
    );
    save(updated);
  };

  return (
    <div className="flex flex-col gap-3">
      {interfaces.length === 0 && (
        <div className="rounded border border-dashed border-gray-700 px-4 py-6 text-center">
          <p className="text-xs text-gray-500">No inputs or outputs defined yet.</p>
          <p className="text-[10px] text-gray-600 mt-1">
            Add ports to describe what data this piece sends and receives.
          </p>
        </div>
      )}
      {interfaces.map((iface, i) => (
        <div key={i} className="rounded border border-gray-700 p-2 flex flex-col gap-1.5">
          <div className="flex items-center justify-between">
            <TextInput
              value={iface.name}
              onChange={(v) => updateInterface(i, "name", v)}
              placeholder="Interface name"
            />
            <button
              onClick={() => removeInterface(i)}
              className="ml-2 text-xs text-red-400 hover:text-red-300 shrink-0"
            >
              Remove
            </button>
          </div>
          <div className="flex gap-2 items-center">
            <button
              onClick={() =>
                updateInterface(i, "direction", iface.direction === "in" ? "out" : "in")
              }
              className={`rounded-full px-2.5 py-0.5 text-[10px] font-medium border ${
                iface.direction === "in"
                  ? "bg-green-500/20 text-green-400 border-green-500/30"
                  : "bg-orange-500/20 text-orange-400 border-orange-500/30"
              }`}
            >
              {iface.direction === "in" ? "← In" : "Out →"}
            </button>
            <div className="flex-1">
              <TextInput
                value={iface.description}
                onChange={(v) => updateInterface(i, "description", v)}
                placeholder="Description"
              />
            </div>
          </div>
        </div>
      ))}
      <button
        onClick={addInterface}
        className="rounded border border-dashed border-gray-700 py-1.5 text-xs text-gray-500 hover:border-gray-600 hover:text-gray-400"
      >
        + Add Input or Output
      </button>
    </div>
  );
}

function ConstraintsTab({
  piece,
  onUpdate,
}: {
  piece: Piece;
  onUpdate: (id: string, updates: Record<string, unknown>) => Promise<void>;
}) {
  const [constraints, setConstraints] = useState<Constraint[]>(piece.constraints);

  useEffect(() => {
    setConstraints(piece.constraints);
  }, [piece.id, piece.constraints]);

  const debouncedUpdate = useMemo(
    () => debounce((updated: Constraint[]) => onUpdate(piece.id, { constraints: updated }), 300),
    [piece.id, onUpdate],
  );

  const save = (updated: Constraint[]) => {
    setConstraints(updated);
    debouncedUpdate(updated);
  };

  const addConstraint = () => {
    save([...constraints, { category: "", description: "" }]);
  };

  const removeConstraint = (index: number) => {
    save(constraints.filter((_, i) => i !== index));
  };

  const updateConstraint = (index: number, field: keyof Constraint, value: string) => {
    const updated = constraints.map((c, i) =>
      i === index ? { ...c, [field]: value } : c,
    );
    save(updated);
  };

  return (
    <div className="flex flex-col gap-3">
      {constraints.length === 0 && (
        <div className="rounded border border-dashed border-gray-700 px-4 py-6 text-center">
          <p className="text-xs text-gray-500">No requirements defined yet.</p>
          <p className="text-[10px] text-gray-600 mt-1">
            Add constraints like performance targets, security rules, or technology limits.
          </p>
        </div>
      )}
      {constraints.map((c, i) => (
        <div key={i} className="rounded border border-gray-700 p-2 flex flex-col gap-1.5">
          <div className="flex items-center justify-between">
            <TextInput
              value={c.category}
              onChange={(v) => updateConstraint(i, "category", v)}
              placeholder="Category (e.g. Performance, Security)"
            />
            <button
              onClick={() => removeConstraint(i)}
              className="ml-2 text-xs text-red-400 hover:text-red-300 shrink-0"
            >
              Remove
            </button>
          </div>
          <TextArea
            value={c.description}
            onChange={(v) => updateConstraint(i, "description", v)}
            placeholder="Constraint description"
            rows={2}
          />
        </div>
      ))}
      <button
        onClick={addConstraint}
        className="rounded border border-dashed border-gray-700 py-1.5 text-xs text-gray-500 hover:border-gray-600 hover:text-gray-400"
      >
        + Add Requirement
      </button>
    </div>
  );
}

function NotesTab({
  piece,
  onUpdate,
}: {
  piece: Piece;
  onUpdate: (id: string, updates: Record<string, unknown>) => Promise<void>;
}) {
  const [notes, setNotes] = useState(piece.notes);
  const notesRef = useRef<HTMLTextAreaElement>(null);
  const refHook = useAtReference(notesRef, notes);

  useEffect(() => {
    setNotes(piece.notes);
  }, [piece.id, piece.notes]);

  const debouncedSave = useMemo(
    () => debounce((v: string) => onUpdate(piece.id, { notes: v }), 300),
    [piece.id, onUpdate],
  );

  return (
    <div>
      <FieldLabel>Notes</FieldLabel>
      <div className="relative">
        <textarea
          ref={notesRef}
          value={notes}
          onChange={(e) =>
            refHook.handleChange(e, (v) => {
              setNotes(v);
              debouncedSave(v);
            })
          }
          placeholder="Freeform notes... Use @PieceName to reference other pieces."
          rows={16}
          className="w-full rounded border border-gray-700 bg-gray-800 px-2 py-1.5 text-xs text-gray-100 placeholder-gray-600 focus:border-blue-500 focus:outline-none resize-none font-mono"
        />
        <ReferenceSuggestions
          show={refHook.showSuggestions}
          suggestions={refHook.suggestions}
          onSelect={(name) =>
            refHook.insertReference(name, (v) => {
              setNotes(v);
              debouncedSave(v);
            })
          }
        />
      </div>
    </div>
  );
}

const engineOptions = [
  { value: "built-in", label: "Built-in LLM" },
  { value: "claude-code", label: "Claude Code" },
  { value: "codex", label: "Codex" },
];

function AgentTab({
  piece,
  onUpdate,
}: {
  piece: Piece;
  onUpdate: (id: string, updates: Record<string, unknown>) => Promise<void>;
}) {
  const engine = piece.agentConfig.executionEngine ?? "built-in";
  const isExternal = engine !== "built-in";
  const provider = piece.agentConfig.provider ?? "";
  const model = piece.agentConfig.model ?? "";
  const currentModelPresets = modelPresets[provider] ?? [];
  const workingDir = useProjectStore((s) => s.project?.settings.workingDirectory);

  const run = useAgentStore((s) => s.runs[piece.id]);
  const [feedbackText, setFeedbackText] = useState("");

  useEffect(() => {
    let cancelled = false;

    const loadHistory = async () => {
      const existing = useAgentStore.getState().runs[piece.id];
      if (existing?.running) return;

      const { getAgentHistory } = await import("../../api/tauriApiAsync");
      try {
        const history = await getAgentHistory(piece.id);
        const latest = history[0];
        if (!latest || cancelled) return;

        const metadata = latest.metadata ?? {};
        const validation = metadata.validation ?? undefined;
        useAgentStore.getState().restoreRun(piece.id, {
          running: false,
          output: latest.outputText,
          usage: metadata.usage ?? { input: 0, output: 0 },
          success: metadata.success ?? true,
          exitCode: metadata.exitCode ?? undefined,
          phaseProposal: metadata.phaseProposal ?? undefined,
          phaseChanged: metadata.phaseChanged ?? undefined,
          gitBranch: metadata.gitBranch ?? undefined,
          gitCommitSha: metadata.gitCommitSha ?? undefined,
          gitDiffStat: metadata.gitDiffStat ?? undefined,
          iterationCount: 1,
          validation,
          validationOutput: validation?.output ?? "",
        });
      } catch {
        // Non-fatal: recovery is best-effort.
      }
    };

    loadHistory();
    return () => {
      cancelled = true;
    };
  }, [piece.id]);

  const handleRun = async () => {
    const { runPieceAgent, onAgentOutputChunk } = await import("../../api/tauriApiAsync");
    const agentStore = useAgentStore.getState();
    agentStore.startRun(piece.id);

    const unlisten = await onAgentOutputChunk((payload) => {
      if (payload.pieceId !== piece.id) return;
      const store = useAgentStore.getState();
      if (payload.done) {
        store.completeRun(piece.id, {
          usage: payload.usage ?? { input: 0, output: 0 },
          success: payload.success ?? (payload.exitCode ?? 0) === 0,
          exitCode: payload.exitCode,
          phaseProposal: payload.phaseProposal,
          phaseChanged: payload.phaseChanged,
          gitBranch: payload.gitBranch,
          gitCommitSha: payload.gitCommitSha,
          gitDiffStat: payload.gitDiffStat,
          validation: payload.validation,
        });
        // If phase was auto-changed (autonomous mode), refresh the piece
        if (payload.phaseChanged) {
          useProjectStore.getState().loadProject(piece.projectId);
        }
        unlisten();
      } else {
        if (payload.streamKind === "validation") {
          store.appendValidationChunk(piece.id, payload.chunk);
        } else {
          store.appendChunk(piece.id, payload.chunk);
        }
      }
    });

    try {
      await runPieceAgent(piece.id);
    } catch (e) {
      useToastStore.getState().addToast(`Agent error: ${e}`);
      useAgentStore.getState().completeRun(piece.id, { usage: { input: 0, output: 0 } });
      unlisten();
    }
  };

  const handleFeedbackRun = async () => {
    const fb = feedbackText.trim();
    if (!fb) return;
    setFeedbackText("");
    const { runPieceAgent, onAgentOutputChunk } = await import("../../api/tauriApiAsync");
    useAgentStore.getState().startFeedbackRun(piece.id);

    const unlisten = await onAgentOutputChunk((payload) => {
      if (payload.pieceId !== piece.id) return;
      const store = useAgentStore.getState();
      if (payload.done) {
        store.completeRun(piece.id, {
          usage: payload.usage ?? { input: 0, output: 0 },
          success: payload.success ?? (payload.exitCode ?? 0) === 0,
          exitCode: payload.exitCode,
          phaseProposal: payload.phaseProposal,
          phaseChanged: payload.phaseChanged,
          gitBranch: payload.gitBranch,
          gitCommitSha: payload.gitCommitSha,
          gitDiffStat: payload.gitDiffStat,
          validation: payload.validation,
        });
        if (payload.phaseChanged) {
          useProjectStore.getState().loadProject(piece.projectId);
        }
        unlisten();
      } else {
        if (payload.streamKind === "validation") {
          store.appendValidationChunk(piece.id, payload.chunk);
        } else {
          store.appendChunk(piece.id, payload.chunk);
        }
      }
    });

    try {
      await runPieceAgent(piece.id, fb);
    } catch (e) {
      useToastStore.getState().addToast(`Agent error: ${e}`);
      useAgentStore.getState().completeRun(piece.id, { usage: { input: 0, output: 0 } });
      unlisten();
    }
  };

  const canRun = isExternal
    ? !!workingDir
    : !!provider && !!model;

  const outputRef = useCallback((el: HTMLPreElement | null) => {
    if (el) el.scrollTop = el.scrollHeight;
  }, [run?.output]);

  const handleAdvancePhase = () => {
    if (run?.phaseProposal) {
      onUpdate(piece.id, { phase: run.phaseProposal });
      useAgentStore.getState().clearPhaseProposal(piece.id);
    }
  };

  const handleDismissProposal = () => {
    useAgentStore.getState().clearPhaseProposal(piece.id);
  };

  return (
    <div className="flex flex-col gap-3">
      <div>
        <FieldLabel>Agent Instructions</FieldLabel>
        <p className="text-[10px] text-gray-600 mb-1">
          Use @PieceName to reference other pieces
        </p>
        <AgentPromptEditor
          value={piece.agentPrompt}
          onChange={(v) => onUpdate(piece.id, { agentPrompt: v })}
        />
      </div>

      {/* Execution Engine selector */}
      <div>
        <FieldLabel>Execution Engine</FieldLabel>
        <select
          value={engine}
          onChange={(e) =>
            onUpdate(piece.id, {
              agentConfig: { ...piece.agentConfig, executionEngine: e.target.value === "built-in" ? null : e.target.value },
            })
          }
          className="w-full rounded border border-gray-700 bg-gray-800 px-2 py-1.5 text-xs text-gray-200 focus:border-blue-500 focus:outline-none"
        >
          {engineOptions.map((opt) => (
            <option key={opt.value} value={opt.value}>{opt.label}</option>
          ))}
        </select>
      </div>

      {/* Built-in LLM fields */}
      {!isExternal && (
        <>
          <div>
            <FieldLabel>AI Provider</FieldLabel>
            <SelectWithOther
              value={provider}
              presets={providerPresets}
              onChange={(v) =>
                onUpdate(piece.id, {
                  agentConfig: { ...piece.agentConfig, provider: v || null, model: null },
                })
              }
              placeholder="Custom provider..."
            />
          </div>
          <div>
            <FieldLabel>Model</FieldLabel>
            {currentModelPresets.length > 0 ? (
              <SelectWithOther
                value={model}
                presets={currentModelPresets}
                onChange={(v) =>
                  onUpdate(piece.id, {
                    agentConfig: { ...piece.agentConfig, model: v || null },
                  })
                }
                placeholder="Custom model..."
              />
            ) : (
              <input
                type="text"
                value={model}
                onChange={(e) =>
                  onUpdate(piece.id, {
                    agentConfig: { ...piece.agentConfig, model: e.target.value || null },
                  })
                }
                placeholder="Model name..."
                className="w-full rounded border border-gray-700 bg-gray-800 px-2 py-1.5 text-xs text-gray-100 placeholder-gray-600 focus:border-blue-500 focus:outline-none"
              />
            )}
          </div>
          <div>
            <FieldLabel>Response length limit</FieldLabel>
            <input
              type="number"
              value={piece.agentConfig.tokenBudget ?? ""}
              onChange={(e) =>
                onUpdate(piece.id, {
                  agentConfig: {
                    ...piece.agentConfig,
                    tokenBudget: e.target.value ? parseInt(e.target.value) : null,
                  },
                })
              }
              placeholder="100000"
              className="w-full rounded border border-gray-700 bg-gray-800 px-2 py-1.5 text-xs text-gray-100 placeholder-gray-600 focus:border-blue-500 focus:outline-none"
            />
          </div>
        </>
      )}

      {/* External engine fields */}
      {isExternal && (
        <>
          <div>
            <FieldLabel>Timeout (seconds)</FieldLabel>
            <input
              type="number"
              value={piece.agentConfig.timeout ?? 300}
              onChange={(e) =>
                onUpdate(piece.id, {
                  agentConfig: {
                    ...piece.agentConfig,
                    timeout: e.target.value ? parseInt(e.target.value) : null,
                  },
                })
              }
              placeholder="300"
              className="w-full rounded border border-gray-700 bg-gray-800 px-2 py-1.5 text-xs text-gray-100 placeholder-gray-600 focus:border-blue-500 focus:outline-none"
            />
          </div>
          <div className="rounded border border-gray-800 bg-gray-900 px-3 py-2 text-xs">
            {workingDir ? (
              <p className="text-gray-400">
                Runs in: <span className="font-mono text-gray-300">{workingDir}</span>
              </p>
            ) : (
              <p className="text-amber-400">
                No working directory set. Configure one in Settings to use external tools.
              </p>
            )}
          </div>
        </>
      )}

      <div className="border-t border-gray-800 pt-3">
        <button
          onClick={handleRun}
          disabled={!canRun || run?.running}
          className="w-full rounded bg-green-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-green-500 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
        >
          {run?.running ? "Running..." : "Run Agent"}
        </button>
        {!canRun && !isExternal && (
          <p className="text-[10px] text-gray-600 mt-1">
            Select a provider and model to run the agent
          </p>
        )}
      </div>

      {run?.output && (
        <div>
          <FieldLabel>Output</FieldLabel>
          <pre
            ref={outputRef}
            className="max-h-64 overflow-y-auto rounded border border-gray-700 bg-gray-800 p-2 text-xs text-gray-200 font-mono whitespace-pre-wrap"
          >
            {run.output}
          </pre>
        </div>
      )}

      {run && !run.running && (
        <div className="flex items-center gap-2 text-[10px] text-gray-500">
          {run.success === false && run.validation?.passed === false && (
            <span className="rounded bg-red-900/50 px-1.5 py-0.5 font-medium text-red-400">
              Validation failed
            </span>
          )}
          {run.exitCode !== undefined && (
            <span
              className={`rounded px-1.5 py-0.5 font-medium ${
                run.exitCode === 0
                  ? "bg-green-900/50 text-green-400"
                  : "bg-red-900/50 text-red-400"
              }`}
            >
              {run.exitCode === 0 ? "Success" : `Failed (exit ${run.exitCode})`}
            </span>
          )}
          {run.usage && (run.usage.input > 0 || run.usage.output > 0) && (
            <span>Tokens: {run.usage.input} in / {run.usage.output} out</span>
          )}
        </div>
      )}

      {run?.validation && !run.running && (
        <div className="rounded border border-gray-800 bg-gray-900 px-3 py-2 text-[10px] text-gray-400 space-y-1">
          <div className="flex items-center gap-2">
            <span className="text-gray-500">validation</span>
            <span className={run.validation.passed ? "text-green-400" : "text-red-400"}>
              {run.validation.passed ? "Passed" : `Failed (exit ${run.validation.exitCode})`}
            </span>
          </div>
          <div className="text-gray-500">
            <span className="mr-2">command</span>
            <span className="font-mono text-gray-300">{run.validation.command}</span>
          </div>
          {!!run.validation.output && (
            <pre className="max-h-40 overflow-y-auto whitespace-pre-wrap rounded bg-gray-950 p-2 text-gray-500">
              {run.validation.output}
            </pre>
          )}
        </div>
      )}

      {/* Git info (external engines only) */}
      {run?.gitBranch && !run.running && (
        <div className="rounded border border-gray-800 bg-gray-900 px-3 py-2 text-[10px] text-gray-400 space-y-0.5">
          <div className="flex items-center gap-2">
            <span className="text-gray-500">branch</span>
            <span className="font-mono text-gray-300">{run.gitBranch}</span>
          </div>
          {run.gitCommitSha && (
            <div className="flex items-center gap-2">
              <span className="text-gray-500">commit</span>
              <span className="font-mono text-green-400">{run.gitCommitSha}</span>
            </div>
          )}
          {run.gitDiffStat && (
            <pre className="text-gray-500 whitespace-pre-wrap">{run.gitDiffStat}</pre>
          )}
        </div>
      )}

      {/* Phase transition banner (gated auto-advance) */}
      {run?.phaseProposal && !run.running && (
        <div className="flex items-center gap-2 rounded border border-blue-800 bg-blue-900/30 px-3 py-2 text-xs">
          <span className="flex-1 text-blue-300">
            Agent finished. Advance to <strong className="capitalize">{run.phaseProposal}</strong>?
          </span>
          <button
            onClick={handleAdvancePhase}
            className="rounded bg-blue-600 px-2.5 py-0.5 text-xs font-medium text-white hover:bg-blue-500"
          >
            Advance
          </button>
          <button
            onClick={handleDismissProposal}
            className="text-xs text-gray-400 hover:text-gray-200"
          >
            Stay
          </button>
        </div>
      )}

      {/* Phase auto-advanced confirmation (fully autonomous) */}
      {run?.phaseChanged && !run.running && (
        <p className="text-[10px] text-green-400">
          Phase automatically advanced to <span className="capitalize">{run.phaseChanged}</span>
        </p>
      )}

      {/* Iterative feedback input */}
      {run?.output && !run.running && (
        <div className="flex gap-1.5">
          <textarea
            value={feedbackText}
            onChange={(e) => setFeedbackText(e.target.value)}
            placeholder="Give feedback for another iteration..."
            rows={2}
            className="flex-1 rounded border border-gray-700 bg-gray-800 px-2 py-1.5 text-xs text-gray-200 placeholder-gray-600 focus:border-blue-500 focus:outline-none resize-none"
          />
          <button
            onClick={handleFeedbackRun}
            disabled={!feedbackText.trim()}
            className="shrink-0 self-end rounded bg-blue-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-blue-500 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
          >
            Re-run
          </button>
        </div>
      )}
    </div>
  );
}
