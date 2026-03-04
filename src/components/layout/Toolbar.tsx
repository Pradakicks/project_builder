import { useProjectStore } from "../../store/useProjectStore";

export function Toolbar() {
  const { project, addPiece } = useProjectStore();

  const handleAddPiece = async () => {
    if (!project) return;
    // Place new piece at a random position near center
    const x = 200 + Math.random() * 400;
    const y = 100 + Math.random() * 300;
    await addPiece("New Piece", x, y);
  };

  return (
    <div className="flex items-center gap-3 border-b border-gray-800 bg-gray-900 px-4 py-2">
      <h1 className="text-sm font-semibold text-gray-300">
        {project?.name ?? "Project Builder"}
      </h1>
      <div className="flex-1" />
      <button
        onClick={handleAddPiece}
        className="rounded bg-blue-600 px-3 py-1 text-xs font-medium text-white hover:bg-blue-500 transition-colors"
      >
        + Add Piece
      </button>
    </div>
  );
}
