import { useCallback, useMemo, useRef } from "react";
import {
  ReactFlow,
  Background,
  Controls,
  MiniMap,
  useNodesState,
  useEdgesState,
  addEdge,
  type Node,
  type Edge,
  type OnConnect,
  type OnNodesChange,
  type OnEdgesChange,
  type NodeMouseHandler,
  type EdgeMouseHandler,
  BackgroundVariant,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { PieceNode } from "./PieceNode";
import { ConnectionEdge } from "./ConnectionEdge";
import { useProjectStore } from "../../store/useProjectStore";

const nodeTypes = { piece: PieceNode };
const edgeTypes = { connection: ConnectionEdge };

export function DiagramCanvas() {
  const {
    pieces,
    connections,
    selectPiece,
    selectConnection,
    addConnection,
    addPiece,
    updatePiece,
    drillInto,
  } = useProjectStore();

  // Debounce position persistence (500ms)
  const positionTimers = useRef<Record<string, ReturnType<typeof setTimeout>>>({});

  const initialNodes: Node[] = useMemo(
    () =>
      pieces.map((p) => ({
        id: p.id,
        type: "piece",
        position: { x: p.positionX, y: p.positionY },
        data: {
          label: p.name,
          pieceType: p.pieceType,
          phase: p.phase,
          color: p.color,
          interfaces: p.interfaces,
        },
      })),
    [pieces],
  );

  const initialEdges: Edge[] = useMemo(
    () =>
      connections.map((c) => ({
        id: c.id,
        type: "connection",
        source: c.sourcePieceId,
        target: c.targetPieceId,
        data: {
          label: c.label,
          direction: c.direction,
        },
      })),
    [connections],
  );

  const [nodes, setNodes, onNodesChange] = useNodesState(initialNodes);
  const [edges, setEdges, onEdgesChange] = useEdgesState(initialEdges);

  // Sync nodes when pieces change
  useMemo(() => {
    setNodes(initialNodes);
  }, [initialNodes, setNodes]);

  useMemo(() => {
    setEdges(initialEdges);
  }, [initialEdges, setEdges]);

  const onConnect: OnConnect = useCallback(
    async (params) => {
      if (params.source && params.target) {
        try {
          await addConnection(params.source, params.target, "");
        } catch (e) {
          // Fallback: add edge locally
          setEdges((eds) => addEdge(params, eds));
        }
      }
    },
    [addConnection, setEdges],
  );

  const handleNodesChange: OnNodesChange = useCallback(
    (changes) => {
      onNodesChange(changes);
      // Persist position changes with debounce
      for (const change of changes) {
        if (change.type === "position" && change.position && !change.dragging) {
          const id = change.id;
          const pos = change.position;
          clearTimeout(positionTimers.current[id]);
          positionTimers.current[id] = setTimeout(() => {
            updatePiece(id, { positionX: pos.x, positionY: pos.y }).catch(() => {});
            delete positionTimers.current[id];
          }, 500);
        }
      }
    },
    [onNodesChange, updatePiece],
  );

  const handleEdgesChange: OnEdgesChange = useCallback(
    (changes) => {
      onEdgesChange(changes);
    },
    [onEdgesChange],
  );

  const onNodeClick: NodeMouseHandler = useCallback(
    (_event, node) => {
      selectPiece(node.id);
    },
    [selectPiece],
  );

  const onNodeDoubleClick: NodeMouseHandler = useCallback(
    (_event, node) => {
      drillInto(node.id);
    },
    [drillInto],
  );

  const onEdgeClick: EdgeMouseHandler = useCallback(
    (_event, edge) => {
      selectConnection(edge.id);
    },
    [selectConnection],
  );

  const onPaneClick = useCallback(() => {
    selectPiece(null);
    selectConnection(null);
  }, [selectPiece, selectConnection]);

  // Empty state
  if (pieces.length === 0) {
    return (
      <div className="flex h-full w-full items-center justify-center bg-gray-950">
        <div className="flex flex-col items-center gap-3 text-center">
          <p className="text-sm text-gray-400">No pieces yet</p>
          <button
            onClick={() => addPiece("New Piece", 400, 250)}
            className="rounded bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-500 transition-colors"
          >
            Add your first piece
          </button>
          <p className="text-xs text-gray-600 max-w-xs">
            Pieces represent components of your project. Connect them to define how they interact.
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="h-full w-full">
      <ReactFlow
        nodes={nodes}
        edges={edges}
        onNodesChange={handleNodesChange}
        onEdgesChange={handleEdgesChange}
        onConnect={onConnect}
        onNodeClick={onNodeClick}
        onNodeDoubleClick={onNodeDoubleClick}
        onEdgeClick={onEdgeClick}
        onPaneClick={onPaneClick}
        nodeTypes={nodeTypes}
        edgeTypes={edgeTypes}
        fitView
        className="bg-gray-950"
      >
        <Background variant={BackgroundVariant.Dots} gap={20} size={1} color="#374151" />
        <Controls className="!bg-gray-800 !border-gray-700 [&>button]:!bg-gray-800 [&>button]:!border-gray-700 [&>button]:!text-gray-300" />
        <MiniMap
          className="!bg-gray-900 !border-gray-700"
          nodeColor={(n) => (n.data?.color as string) ?? "#3b82f6"}
          maskColor="rgba(0,0,0,0.5)"
        />
      </ReactFlow>
    </div>
  );
}
