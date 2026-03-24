import { useState, useEffect, useMemo, useRef } from "react";
import { useProjectStore } from "../../store/useProjectStore";
import { useDialogStore } from "../../store/useDialogStore";
import { debounce } from "../../utils/debounce";
import { useAtReference } from "../../hooks/useAtReference";
import { ReferenceSuggestions } from "./ReferenceSuggestions";

export function ConnectionEditor({ connectionId }: { connectionId: string }) {
  const { connections, updateConnection, deleteConnection, selectConnection } =
    useProjectStore();
  const showConfirm = useDialogStore((s) => s.showConfirm);
  const connection = connections.find((c) => c.id === connectionId);

  const [label, setLabel] = useState("");
  const [dataType, setDataType] = useState("");
  const [protocol, setProtocol] = useState("");
  const [notes, setNotes] = useState("");
  const notesRef = useRef<HTMLTextAreaElement>(null);
  const refHook = useAtReference(notesRef, notes);

  useEffect(() => {
    if (connection) {
      setLabel(connection.label);
      setDataType(connection.dataType ?? "");
      setProtocol(connection.protocol ?? "");
      setNotes(connection.notes);
    }
  }, [connection]);

  // Escape key closes the editor
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") selectConnection(null);
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [selectConnection]);

  const save = useMemo(
    () =>
      debounce((field: string, value: unknown) => {
        if (connection) updateConnection(connection.id, { [field]: value });
      }, 300),
    [connection?.id, updateConnection],
  );

  if (!connection) return null;

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center justify-between border-b border-gray-800 px-4 py-2">
        <h2 className="text-sm font-semibold text-gray-200">Connection</h2>
        <div className="flex gap-1">
          <button
            onClick={() =>
              showConfirm("Delete this connection?", async () => {
                await deleteConnection(connection.id);
                selectConnection(null);
              })
            }
            className="rounded px-2 py-1 text-xs text-red-400 hover:bg-red-900/30"
          >
            Delete
          </button>
          <button
            onClick={() => selectConnection(null)}
            className="rounded px-2 py-1 text-xs text-gray-400 hover:bg-gray-800"
          >
            Close
          </button>
        </div>
      </div>

      <div className="flex-1 overflow-y-auto p-4 flex flex-col gap-3">
        <div>
          <label className="block text-xs font-medium text-gray-400 mb-1">
            Connection Name
          </label>
          <input
            type="text"
            value={label}
            onChange={(e) => {
              setLabel(e.target.value);
              save("label", e.target.value);
            }}
            placeholder="Connection label"
            className="w-full rounded border border-gray-700 bg-gray-800 px-2 py-1.5 text-xs text-gray-100 placeholder-gray-600 focus:border-blue-500 focus:outline-none"
          />
        </div>

        <div>
          <label className="block text-xs font-medium text-gray-400 mb-1">
            Direction
          </label>
          <select
            value={connection.direction}
            onChange={(e) => updateConnection(connection.id, { direction: e.target.value as "unidirectional" | "bidirectional" })}
            className="w-full rounded border border-gray-700 bg-gray-800 px-2 py-1.5 text-xs text-gray-100 focus:border-blue-500 focus:outline-none"
          >
            <option value="unidirectional">One-way</option>
            <option value="bidirectional">Two-way</option>
          </select>
        </div>

        <div>
          <label className="block text-xs font-medium text-gray-400 mb-1">
            Data Type
          </label>
          <input
            type="text"
            value={dataType}
            onChange={(e) => {
              setDataType(e.target.value);
              save("dataType", e.target.value);
            }}
            placeholder="e.g. JSON, Protobuf, REST"
            className="w-full rounded border border-gray-700 bg-gray-800 px-2 py-1.5 text-xs text-gray-100 placeholder-gray-600 focus:border-blue-500 focus:outline-none"
          />
        </div>

        <div>
          <label className="block text-xs font-medium text-gray-400 mb-1">
            Protocol
          </label>
          <input
            type="text"
            value={protocol}
            onChange={(e) => {
              setProtocol(e.target.value);
              save("protocol", e.target.value);
            }}
            placeholder="e.g. HTTP, gRPC, WebSocket"
            className="w-full rounded border border-gray-700 bg-gray-800 px-2 py-1.5 text-xs text-gray-100 placeholder-gray-600 focus:border-blue-500 focus:outline-none"
          />
        </div>

        <div>
          <label className="block text-xs font-medium text-gray-400 mb-1">
            Notes
          </label>
          <div className="relative">
            <textarea
              ref={notesRef}
              value={notes}
              onChange={(e) =>
                refHook.handleChange(e, (v) => {
                  setNotes(v);
                  save("notes", v);
                })
              }
              placeholder="Connection notes... Use @PieceName to reference pieces."
              rows={6}
              className="w-full rounded border border-gray-700 bg-gray-800 px-2 py-1.5 text-xs text-gray-100 placeholder-gray-600 focus:border-blue-500 focus:outline-none resize-none font-mono"
            />
            <ReferenceSuggestions
              show={refHook.showSuggestions}
              suggestions={refHook.suggestions}
              onSelect={(name) =>
                refHook.insertReference(name, (v) => {
                  setNotes(v);
                  save("notes", v);
                })
              }
            />
          </div>
        </div>
      </div>
    </div>
  );
}
