import { useState, useCallback, type RefObject } from "react";
import { useProjectStore } from "../store/useProjectStore";
import { getReferenceSuggestions } from "../utils/references";

/**
 * Provides @reference autocomplete behaviour for any textarea.
 *
 * Usage:
 *   const ref = useRef<HTMLTextAreaElement>(null);
 *   const { showSuggestions, suggestions, handleChange, insertReference } =
 *     useAtReference(ref, value);
 *
 *   <div className="relative">
 *     <textarea ref={ref} onChange={(e) => handleChange(e, onTextChange)} />
 *     <ReferenceSuggestions show={showSuggestions} suggestions={suggestions}
 *       onSelect={(name) => insertReference(name, onTextChange)} />
 *   </div>
 */
export function useAtReference(
  textareaRef: RefObject<HTMLTextAreaElement | null>,
  value: string,
) {
  const pieces = useProjectStore((s) => s.pieces);
  const [showSuggestions, setShowSuggestions] = useState(false);
  const [suggestions, setSuggestions] = useState<{ id: string; name: string }[]>([]);
  const [cursorPosition, setCursorPosition] = useState(0);

  const handleChange = useCallback(
    (
      e: React.ChangeEvent<HTMLTextAreaElement>,
      onTextChange: (v: string) => void,
    ) => {
      const text = e.target.value;
      const cursor = e.target.selectionStart;
      onTextChange(text);
      setCursorPosition(cursor);

      const textBefore = text.slice(0, cursor);
      const atMatch = textBefore.match(/@([\w\s\-]*)$/);
      if (atMatch) {
        const matches = getReferenceSuggestions(atMatch[1], pieces);
        setSuggestions(matches.map((p) => ({ id: p.id, name: p.name })));
        setShowSuggestions(matches.length > 0);
      } else {
        setShowSuggestions(false);
      }
    },
    [pieces],
  );

  const insertReference = useCallback(
    (name: string, onTextChange: (v: string) => void) => {
      const textBefore = value.slice(0, cursorPosition);
      const textAfter = value.slice(cursorPosition);
      const atMatch = textBefore.match(/@([\w\s\-]*)$/);
      if (!atMatch) return;

      const newBefore = textBefore.slice(0, atMatch.index) + `@${name}`;
      const newText = newBefore + " " + textAfter;
      onTextChange(newText);
      setShowSuggestions(false);

      requestAnimationFrame(() => {
        if (textareaRef.current) {
          const newPos = newBefore.length + 1;
          textareaRef.current.focus();
          textareaRef.current.setSelectionRange(newPos, newPos);
        }
      });
    },
    [value, cursorPosition, textareaRef],
  );

  const closeSuggestions = useCallback(() => setShowSuggestions(false), []);

  return { showSuggestions, suggestions, handleChange, insertReference, closeSuggestions };
}
