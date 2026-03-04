import { useState, useEffect, useCallback } from "react";
import { useProjectStore } from "../../store/useProjectStore";
import type {
  Piece,
  PieceInterface,
  Constraint,
  Phase,
} from "../../types";
import { AgentPromptEditor } from "./AgentPromptEditor";
import { PillSelect } from "../ui/PillSelect";
import { SelectWithOther } from "../ui/SelectWithOther";

type Tab = "general" | "interfaces" | "constraints" | "notes" | "agent";

const tabs: { id: Tab; label: string }[] = [
  { id: "general", label: "General" },
  { id: "interfaces", label: "Interfaces" },
  { id: "constraints", label: "Constraints" },
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
            onClick={async () => {
              await deletePiece(piece.id);
              selectPiece(null);
            }}
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

  useEffect(() => {
    setName(piece.name);
    setPieceType(piece.pieceType);
    setColor(piece.color ?? "#3b82f6");
    setResponsibilities(piece.responsibilities);
  }, [piece.id, piece.name, piece.pieceType, piece.color, piece.responsibilities]);

  const save = useCallback(
    (field: string, value: unknown) => {
      onUpdate(piece.id, { [field]: value });
    },
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
        <TextArea
          value={responsibilities}
          onChange={(v) => {
            setResponsibilities(v);
            save("responsibilities", v);
          }}
          placeholder="What does this piece do?"
          rows={4}
        />
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

  const save = (updated: PieceInterface[]) => {
    setInterfaces(updated);
    onUpdate(piece.id, { interfaces: updated });
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
        + Add Interface
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

  const save = (updated: Constraint[]) => {
    setConstraints(updated);
    onUpdate(piece.id, { constraints: updated });
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
        + Add Constraint
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

  useEffect(() => {
    setNotes(piece.notes);
  }, [piece.id, piece.notes]);

  return (
    <div>
      <FieldLabel>Notes (Markdown)</FieldLabel>
      <TextArea
        value={notes}
        onChange={(v) => {
          setNotes(v);
          onUpdate(piece.id, { notes: v });
        }}
        placeholder="Freeform notes..."
        rows={16}
      />
    </div>
  );
}

function AgentTab({
  piece,
  onUpdate,
}: {
  piece: Piece;
  onUpdate: (id: string, updates: Record<string, unknown>) => Promise<void>;
}) {
  const provider = piece.agentConfig.provider ?? "";
  const model = piece.agentConfig.model ?? "";

  // Determine which model presets to show based on provider
  const currentModelPresets = modelPresets[provider] ?? [];

  return (
    <div className="flex flex-col gap-3">
      <div>
        <FieldLabel>Agent Prompt</FieldLabel>
        <p className="text-[10px] text-gray-600 mb-1">
          Use @PieceName to reference other pieces
        </p>
        <AgentPromptEditor
          value={piece.agentPrompt}
          onChange={(v) => onUpdate(piece.id, { agentPrompt: v })}
        />
      </div>
      <div>
        <FieldLabel>LLM Provider</FieldLabel>
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
        <FieldLabel>Token Budget</FieldLabel>
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
    </div>
  );
}
