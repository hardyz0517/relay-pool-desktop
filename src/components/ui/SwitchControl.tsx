import { cn } from "@/lib/utils";

type SwitchControlProps = {
  checked: boolean;
  disabled?: boolean;
  ariaLabel: string;
  onCheckedChange: () => void;
  onLabel?: string;
  offLabel?: string;
  className?: string;
};

export function SwitchControl({
  checked,
  disabled,
  ariaLabel,
  onCheckedChange,
  onLabel = "开启",
  offLabel = "关闭",
  className,
}: SwitchControlProps) {
  return (
    <button
      aria-checked={checked}
      aria-label={ariaLabel}
      className={cn(
        "inline-flex h-8 min-w-[96px] items-center justify-between gap-2 rounded-full border border-border bg-white px-2 text-xs font-medium text-slate-700 shadow-[var(--surface-shadow)] transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[hsl(var(--accent)/0.28)] disabled:cursor-default disabled:opacity-60",
        className,
      )}
      disabled={disabled}
      role="switch"
      type="button"
      onClick={onCheckedChange}
    >
      <span className="min-w-7 text-left">{checked ? onLabel : offLabel}</span>
      <span
        className={cn(
          "relative h-5 w-10 shrink-0 rounded-full transition-colors",
          checked ? "bg-teal-500" : "bg-slate-200",
        )}
      >
        <span
          className={cn(
            "absolute left-0.5 top-0.5 h-4 w-4 rounded-full bg-white shadow-sm ring-1 ring-black/5 transition-transform",
            checked && "translate-x-5",
          )}
        />
      </span>
    </button>
  );
}
