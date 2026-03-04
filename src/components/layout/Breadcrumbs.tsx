import { useProjectStore } from "../../store/useProjectStore";

export function Breadcrumbs() {
  const { breadcrumbs, navigateTo } = useProjectStore();

  if (breadcrumbs.length <= 1) return null;

  return (
    <div className="flex items-center gap-1 border-b border-gray-800 bg-gray-900/50 px-4 py-1 text-xs">
      {breadcrumbs.map((crumb, i) => (
        <span key={crumb.id} className="flex items-center gap-1">
          {i > 0 && <span className="text-gray-600">/</span>}
          {i < breadcrumbs.length - 1 ? (
            <button
              onClick={() => navigateTo(i)}
              className="text-blue-400 hover:text-blue-300"
            >
              {crumb.name}
            </button>
          ) : (
            <span className="text-gray-400">{crumb.name}</span>
          )}
        </span>
      ))}
    </div>
  );
}
