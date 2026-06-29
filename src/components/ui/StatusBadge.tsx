import { cn } from "@/lib/utils";

export type StatusTone = "healthy" | "warning" | "error" | "disabled" | "info";

type StatusBadgeProps = {
  children: string;
  tone?: StatusTone;
  className?: string;
};

const toneClassName: Record<StatusTone, string> = {
  healthy: "border-emerald-200 bg-emerald-50 text-emerald-700",
  warning: "border-amber-200 bg-amber-50 text-amber-700",
  error: "border-rose-200 bg-rose-50 text-rose-700",
  disabled: "border-slate-200 bg-slate-50 text-slate-500",
  info: "border-blue-200 bg-blue-50 text-blue-700",
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
