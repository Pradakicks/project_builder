/**
 * Dropdown for @reference autocomplete suggestions.
 * Rendered inside a `relative`-positioned container just below the textarea.
 */
export function ReferenceSuggestions({
  show,
  suggestions,
  onSelect,
}: {
  show: boolean;
  suggestions: { id: string; name: string }[];
  onSelect: (name: string) => void;
}) {
  if (!show || suggestions.length === 0) return null;

  return (
    <div className="absolute left-0 right-0 z-10 mt-1 rounded border border-gray-700 bg-gray-800 shadow-lg max-h-32 overflow-y-auto">
      {suggestions.map((s) => (
        <button
          key={s.id}
          onMouseDown={(e) => {
            // Prevent textarea blur before the click registers
            e.preventDefault();
            onSelect(s.name);
          }}
          className="block w-full px-3 py-1.5 text-left text-xs text-gray-200 hover:bg-gray-700"
        >
          @{s.name}
        </button>
      ))}
    </div>
  );
}
