import { cn } from "@/lib/utils";

type SwitchControlProps = {
  checked: boolean;
  disabled?: boolean;
  ariaLabel: string;
  onCheckedChange: () => void;
  onLabel?: string;
  offLabel?: string;
  showLabel?: boolean;
  className?: string;
};

export function SwitchControl({
  checked,
  disabled,
  ariaLabel,
  onCheckedChange,
  onLabel = "开启",
  offLabel = "关闭",
  showLabel = true,
  className,
}: SwitchControlProps) {
  return (
    <button
      aria-checked={checked}
      aria-label={ariaLabel}
      className={cn(
        "inline-flex items-center rounded-full transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/30 disabled:cursor-default disabled:opacity-60",
        showLabel
          ? "h-8 min-w-[96px] justify-between gap-2 border border-border bg-surface px-2 text-xs font-medium text-foreground shadow-surface"
          : "h-5 w-10 justify-center bg-transparent p-0",
        className,
      )}
      disabled={disabled}
      role="switch"
      type="button"
      onClick={onCheckedChange}
    >
      {showLabel && <span className="min-w-7 text-left">{checked ? onLabel : offLabel}</span>}
      <span
          className={cn(
            "relative h-5 w-10 shrink-0 rounded-full transition-colors",
            checked ? "bg-primary-solid" : "bg-muted",
          )}
      >
        <span
          className={cn(
            "absolute left-0.5 top-0.5 h-4 w-4 rounded-full bg-control-thumb shadow-sm ring-1 ring-border transition-transform",
            checked && "translate-x-5",
          )}
        />
      </span>
    </button>
  );
}
