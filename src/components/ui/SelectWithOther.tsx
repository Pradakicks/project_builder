import { useState, useEffect } from "react";

export function SelectWithOther({
  value,
  presets,
  onChange,
  placeholder,
}: {
  value: string;
  presets: string[];
  onChange: (v: string) => void;
  placeholder?: string;
}) {
  const isPreset = presets.includes(value);
  const [showCustom, setShowCustom] = useState(!isPreset && value !== "");

  useEffect(() => {
    setShowCustom(!presets.includes(value) && value !== "");
  }, [value, presets]);

  return (
    <div className="flex flex-col gap-1.5">
      <div className="flex flex-wrap gap-1">
        {presets.map((preset) => (
          <button
            key={preset}
            onClick={() => {
              setShowCustom(false);
              onChange(preset);
            }}
            className={`rounded px-2 py-1 text-xs transition-colors border ${
              value === preset && !showCustom
                ? "bg-blue-600 text-white border-transparent"
                : "bg-gray-800 text-gray-400 hover:text-gray-200 border-gray-700"
            }`}
          >
            {preset}
          </button>
        ))}
        <button
          onClick={() => {
            setShowCustom(true);
            if (presets.includes(value)) onChange("");
          }}
          className={`rounded px-2 py-1 text-xs transition-colors border ${
            showCustom
              ? "bg-blue-600 text-white border-transparent"
              : "bg-gray-800 text-gray-400 hover:text-gray-200 border-gray-700"
          }`}
        >
          Other
        </button>
      </div>
      {showCustom && (
        <input
          type="text"
          value={presets.includes(value) ? "" : value}
          onChange={(e) => onChange(e.target.value)}
          placeholder={placeholder ?? "Custom value..."}
          className="w-full rounded border border-gray-700 bg-gray-800 px-2 py-1.5 text-xs text-gray-100 placeholder-gray-600 focus:border-blue-500 focus:outline-none"
        />
      )}
    </div>
  );
}
