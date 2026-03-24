import { useRef } from "react";
import { useAtReference } from "../../hooks/useAtReference";
import { ReferenceSuggestions } from "./ReferenceSuggestions";

export function AgentPromptEditor({
  value,
  onChange,
}: {
  value: string;
  onChange: (v: string) => void;
}) {
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const { showSuggestions, suggestions, handleChange, insertReference } =
    useAtReference(textareaRef, value);

  return (
    <div className="relative">
      <textarea
        ref={textareaRef}
        value={value}
        onChange={(e) => handleChange(e, onChange)}
        rows={10}
        placeholder="Write the agent prompt for this piece...&#10;Use @PieceName to reference other pieces."
        className="w-full rounded border border-gray-700 bg-gray-800 px-2 py-1.5 text-xs text-gray-100 placeholder-gray-600 focus:border-blue-500 focus:outline-none resize-none font-mono"
      />
      <ReferenceSuggestions
        show={showSuggestions}
        suggestions={suggestions}
        onSelect={(name) => insertReference(name, onChange)}
      />
    </div>
  );
}
