import { useState, useEffect } from "react";
import { useProjectStore } from "../../store/useProjectStore";


export function ConnectionEditor({ connectionId }: { connectionId: string }) {
  const { connections, updateConnection, deleteConnection, selectConnection } =
    useProjectStore();
  const connection = connections.find((c) => c.id === connectionId);

  const [label, setLabel] = useState("");
  const [dataType, setDataType] = useState("");
  const [protocol, setProtocol] = useState("");
  const [notes, setNotes] = useState("");

  useEffect(() => {
    if (connection) {
      setLabel(connection.label);
      setDataType(connection.dataType ?? "");
      setProtocol(connection.protocol ?? "");
      setNotes(connection.notes);
    }
  }, [connection]);

  if (!connection) return null;

  const save = (field: string, value: unknown) => {
    updateConnection(connection.id, { [field]: value });
  };

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center justify-between border-b border-gray-800 px-4 py-2">
        <h2 className="text-sm font-semibold text-gray-200">Connection</h2>
        <div className="flex gap-1">
          <button
            onClick={async () => {
              await deleteConnection(connection.id);
              selectConnection(null);
            }}
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
            Label
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
            onChange={(e) => save("direction", e.target.value)}
            className="w-full rounded border border-gray-700 bg-gray-800 px-2 py-1.5 text-xs text-gray-100 focus:border-blue-500 focus:outline-none"
          >
            <option value="unidirectional">Unidirectional</option>
            <option value="bidirectional">Bidirectional</option>
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
          <textarea
            value={notes}
            onChange={(e) => {
              setNotes(e.target.value);
              save("notes", e.target.value);
            }}
            placeholder="Connection notes..."
            rows={6}
            className="w-full rounded border border-gray-700 bg-gray-800 px-2 py-1.5 text-xs text-gray-100 placeholder-gray-600 focus:border-blue-500 focus:outline-none resize-none font-mono"
          />
        </div>
      </div>
    </div>
  );
}
