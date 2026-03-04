import { useState, useRef, useCallback } from "react";
import { useProjectStore } from "../../store/useProjectStore";
import { getReferenceSuggestions } from "../../utils/references";

export function AgentPromptEditor({
  value,
  onChange,
}: {
  value: string;
  onChange: (v: string) => void;
}) {
  const { pieces } = useProjectStore();
  const [showSuggestions, setShowSuggestions] = useState(false);
  const [suggestions, setSuggestions] = useState<{ id: string; name: string }[]>([]);
  const [cursorPosition, setCursorPosition] = useState(0);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const handleChange = useCallback(
    (e: React.ChangeEvent<HTMLTextAreaElement>) => {
      const text = e.target.value;
      const cursor = e.target.selectionStart;
      onChange(text);
      setCursorPosition(cursor);

      // Check if we're typing an @reference
      const textBefore = text.slice(0, cursor);
      const atMatch = textBefore.match(/@([\w\s\-]*)$/);

      if (atMatch) {
        const partial = atMatch[1];
        const matches = getReferenceSuggestions(partial, pieces);
        setSuggestions(matches.map((p) => ({ id: p.id, name: p.name })));
        setShowSuggestions(matches.length > 0);
      } else {
        setShowSuggestions(false);
      }
    },
    [onChange, pieces],
  );

  const insertReference = useCallback(
    (name: string) => {
      const textBefore = value.slice(0, cursorPosition);
      const textAfter = value.slice(cursorPosition);
      const atMatch = textBefore.match(/@([\w\s\-]*)$/);

      if (atMatch) {
        const newBefore = textBefore.slice(0, atMatch.index) + `@${name}`;
        const newText = newBefore + " " + textAfter;
        onChange(newText);
        setShowSuggestions(false);

        // Focus and set cursor after the inserted reference
        requestAnimationFrame(() => {
          if (textareaRef.current) {
            const newPos = newBefore.length + 1;
            textareaRef.current.focus();
            textareaRef.current.setSelectionRange(newPos, newPos);
          }
        });
      }
    },
    [value, cursorPosition, onChange],
  );

  return (
    <div className="relative">
      <textarea
        ref={textareaRef}
        value={value}
        onChange={handleChange}
        rows={10}
        placeholder="Write the agent prompt for this piece...&#10;Use @PieceName to reference other pieces."
        className="w-full rounded border border-gray-700 bg-gray-800 px-2 py-1.5 text-xs text-gray-100 placeholder-gray-600 focus:border-blue-500 focus:outline-none resize-none font-mono"
      />
      {showSuggestions && (
        <div className="absolute left-0 right-0 z-10 mt-1 rounded border border-gray-700 bg-gray-800 shadow-lg max-h-32 overflow-y-auto">
          {suggestions.map((s) => (
            <button
              key={s.id}
              onClick={() => insertReference(s.name)}
              className="block w-full px-3 py-1.5 text-left text-xs text-gray-200 hover:bg-gray-700"
            >
              @{s.name}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
