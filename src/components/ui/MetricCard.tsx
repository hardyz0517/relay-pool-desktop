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
  neutral: "text-slate-500",
  good: "text-emerald-700",
  warning: "text-amber-700",
  danger: "text-rose-700",
};

const iconClassName = {
  neutral: "bg-teal-100 text-teal-700",
  good: "bg-emerald-100 text-emerald-700",
  warning: "bg-amber-100 text-amber-700",
  danger: "bg-rose-100 text-rose-700",
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
        "flex min-h-[92px] items-center gap-3 rounded-2xl border border-white/70 bg-white/95 px-4 py-3 shadow-[0_12px_30px_rgba(33,79,88,0.07)]",
        className,
      )}
    >
      {Icon && (
        <div
          className={cn(
            "flex h-10 w-10 shrink-0 items-center justify-center rounded-2xl",
            iconClassName[tone],
          )}
        >
          <Icon className="h-4 w-4" />
        </div>
      )}
      <div className="min-w-0 flex-1">
        <div className="truncate text-xs text-muted-foreground">{label}</div>
        <div className="mt-0.5 truncate text-[22px] font-semibold leading-7 text-slate-800">
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
