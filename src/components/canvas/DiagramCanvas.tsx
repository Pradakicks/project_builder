import { useCallback, useMemo } from "react";
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
    updatePiece,
    drillInto,
  } = useProjectStore();

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
      // Persist position changes
      for (const change of changes) {
        if (change.type === "position" && change.position && !change.dragging) {
          updatePiece(change.id, {
            positionX: change.position.x,
            positionY: change.position.y,
          }).catch(() => {});
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
