import type { ReactNode } from "react";
import type { LucideIcon } from "lucide-react";
import { cn } from "@/lib/utils";

type MetricCardProps = {
  label: string;
  value: ReactNode;
  detail?: string;
  icon?: LucideIcon;
  tone?: "neutral" | "good" | "warning" | "danger";
  className?: string;
};

const toneClassName = {
  neutral: "text-muted-foreground",
  good: "text-success-foreground",
  warning: "text-warning-foreground",
  danger: "text-danger-foreground",
};

const iconClassName = {
  neutral: "bg-muted text-muted-foreground",
  good: "bg-success-surface text-success-foreground",
  warning: "bg-warning-surface text-warning-foreground",
  danger: "bg-danger-surface text-danger-foreground",
};

export function MetricCard({
  label,
  value,
  detail,
  icon: Icon,
  tone = "neutral",
  className,
}: MetricCardProps) {
  return (
    <div
      className={cn(
        "flex min-h-[92px] items-center gap-3 rounded-[var(--surface-radius)] border border-border bg-surface px-4 py-3 shadow-surface",
        className,
      )}
    >
      {Icon && (
        <div
          className={cn(
            "flex h-10 w-10 shrink-0 items-center justify-center rounded-[12px]",
            iconClassName[tone],
          )}
        >
          <Icon className="h-4 w-4" />
        </div>
      )}
      <div className="min-w-0 flex-1">
        <div className="truncate text-xs text-muted-foreground">{label}</div>
        <div className="mt-0.5 truncate text-[22px] font-semibold leading-7 text-foreground">
          {value}
        </div>
        {detail && (
          <div className={cn("mt-0.5 truncate text-xs", toneClassName[tone])}>
            {detail}
          </div>
        )}
      </div>
    </div>
  );
}
