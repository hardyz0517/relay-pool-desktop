import { cn } from "@/lib/utils";

type SegmentedControlOption<T extends string> = {
  value: T;
  label: string;
};

type SegmentedControlProps<T extends string> = {
  value: T;
  options: SegmentedControlOption<T>[];
  onChange?: (value: T) => void;
  className?: string;
};

export function SegmentedControl<T extends string>({
  value,
  options,
  onChange,
  className,
}: SegmentedControlProps<T>) {
  return (
    <div
      className={cn(
        "inline-flex h-8 overflow-hidden rounded-[var(--surface-radius)] border border-border bg-slate-50 p-0.5",
        className,
      )}
    >
      {options.map((option) => (
        <button
          key={option.value}
          type="button"
          onClick={() => onChange?.(option.value)}
          className={cn(
            "cursor-pointer rounded-[calc(var(--surface-radius)-3px)] px-2.5 text-xs font-medium transition-colors",
            option.value === value
              ? "bg-white text-blue-700 shadow-[0_0_0_1px_rgba(148,163,184,0.45)]"
              : "text-slate-600 hover:text-slate-800",
          )}
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}
