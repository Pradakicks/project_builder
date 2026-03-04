export function PillSelect<T extends string>({
  value,
  options,
  colorMap,
  onChange,
}: {
  value: T;
  options: { value: T; label: string }[];
  colorMap?: Record<T, string>;
  onChange: (v: T) => void;
}) {
  return (
    <div className="flex flex-wrap gap-1.5">
      {options.map((opt) => {
        const active = opt.value === value;
        const colors = active && colorMap?.[opt.value]
          ? colorMap[opt.value]
          : active
            ? "bg-blue-600 text-white"
            : "bg-gray-800 text-gray-400 hover:text-gray-200";
        return (
          <button
            key={opt.value}
            onClick={() => onChange(opt.value)}
            className={`rounded-full px-3 py-1 text-xs font-medium transition-colors border ${
              active ? "border-transparent" : "border-gray-700"
            } ${colors}`}
          >
            {opt.label}
          </button>
        );
      })}
    </div>
  );
}
