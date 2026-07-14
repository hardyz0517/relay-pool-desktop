import { cn } from "@/lib/utils";

export type StatusTone = "healthy" | "warning" | "error" | "disabled" | "info";

type StatusBadgeProps = {
  children: string;
  tone?: StatusTone;
  className?: string;
};

const toneClassName: Record<StatusTone, string> = {
  healthy: "border-success-border bg-success-surface text-success-foreground",
  warning: "border-warning-border bg-warning-surface text-warning-foreground",
  error: "border-danger-border bg-danger-surface text-danger-foreground",
  disabled: "border-border bg-muted text-muted-foreground",
  info: "border-info-border bg-info-surface text-info-foreground",
};

export function StatusBadge({
  children,
  tone = "info",
  className,
}: StatusBadgeProps) {
  return (
    <span
      className={cn(
        "inline-flex h-6 items-center rounded-full border px-2 text-xs font-medium",
        toneClassName[tone],
        className,
      )}
    >
      {children}
    </span>
  );
}
