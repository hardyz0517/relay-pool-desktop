import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

type MetricCardProps = {
  label: string;
  value: ReactNode;
  detail?: string;
  tone?: "neutral" | "good" | "warning" | "danger";
  className?: string;
};

const toneClassName = {
  neutral: "text-slate-500",
  good: "text-emerald-700",
  warning: "text-amber-700",
  danger: "text-rose-700",
};

export function MetricCard({
  label,
  value,
  detail,
  tone = "neutral",
  className,
}: MetricCardProps) {
  return (
    <div className={cn("rounded-lg border border-border bg-white p-3", className)}>
      <div className="text-xs text-muted-foreground">{label}</div>
      <div className="mt-1 text-xl font-semibold leading-7 text-slate-800">
        {value}
      </div>
      {detail && (
        <div className={cn("mt-1 text-xs", toneClassName[tone])}>{detail}</div>
      )}
    </div>
  );
}
