import type { ReactNode } from "react";
import type { LucideIcon } from "lucide-react";
import { cn } from "@/lib/utils";

export type MetricTone = "neutral" | "good" | "warning" | "danger";
export type MetricAccent =
  | "slate"
  | "emerald"
  | "green"
  | "blue"
  | "amber"
  | "indigo"
  | "violet"
  | "purple"
  | "rose";

export type MetricItem = {
  label: string;
  value: ReactNode;
  detail?: ReactNode;
  icon?: LucideIcon;
  tone?: MetricTone;
  accent?: MetricAccent;
  valueClassName?: string;
};

type MetricPanelProps = {
  title?: string;
  description?: string;
  metrics: MetricItem[];
  className?: string;
};

const toneClassName: Record<MetricTone, string> = {
  neutral: "text-foreground",
  good: "text-success-foreground",
  warning: "text-warning-foreground",
  danger: "text-danger-foreground",
};

const accentClassName: Record<MetricAccent, { icon: string; value: string }> = {
  slate: {
    icon: "bg-metric-slate-surface text-metric-slate-foreground",
    value: "text-metric-slate-foreground",
  },
  emerald: {
    icon: "bg-metric-emerald-surface text-metric-emerald-foreground",
    value: "text-metric-emerald-foreground",
  },
  green: {
    icon: "bg-metric-green-surface text-metric-green-foreground",
    value: "text-metric-green-foreground",
  },
  blue: {
    icon: "bg-metric-blue-surface text-metric-blue-foreground",
    value: "text-metric-blue-foreground",
  },
  amber: {
    icon: "bg-metric-amber-surface text-metric-amber-foreground",
    value: "text-metric-amber-foreground",
  },
  indigo: {
    icon: "bg-metric-indigo-surface text-metric-indigo-foreground",
    value: "text-metric-indigo-foreground",
  },
  violet: {
    icon: "bg-metric-violet-surface text-metric-violet-foreground",
    value: "text-metric-violet-foreground",
  },
  purple: {
    icon: "bg-metric-purple-surface text-metric-purple-foreground",
    value: "text-metric-purple-foreground",
  },
  rose: {
    icon: "bg-metric-rose-surface text-metric-rose-foreground",
    value: "text-metric-rose-foreground",
  },
};

const toneAccent: Record<MetricTone, MetricAccent> = {
  neutral: "slate",
  good: "emerald",
  warning: "amber",
  danger: "rose",
};

export function MetricPanel({
  title,
  description,
  metrics,
  className,
}: MetricPanelProps) {
  return (
    <section className={cn("grid gap-3", className)}>
      {(title || description) && (
        <header>
          {title && (
            <h2 className="truncate text-[13px] font-semibold text-foreground">
              {title}
            </h2>
          )}
          {description && (
            <p className="mt-0.5 truncate text-xs text-muted-foreground">
              {description}
            </p>
          )}
        </header>
      )}
      <div className="grid gap-3 sm:grid-cols-2 md:grid-cols-4">
        {metrics.map(({ label, value, detail, icon: Icon, tone = "neutral", accent, valueClassName }) => {
          const metricAccent = accent ?? toneAccent[tone];
          const shouldUseToneValue = tone === "warning" || tone === "danger";

          return (
            <div
              key={label}
              className="flex min-h-[96px] items-center gap-3 rounded-[12px] border border-border bg-surface px-4 py-3 shadow-surface"
            >
              {Icon && (
                <div
                  className={cn(
                    "flex h-9 w-9 shrink-0 items-center justify-center rounded-[8px]",
                    accentClassName[metricAccent].icon,
                  )}
                >
                  <Icon className="h-4 w-4" />
                </div>
              )}
              <div className="min-w-0 flex-1">
                <div className="truncate text-xs text-muted-foreground">
                  {label}
                </div>
                <div
                  className={cn(
                    "mt-0.5 truncate text-[22px] font-semibold leading-7",
                    valueClassName ??
                      (shouldUseToneValue
                        ? toneClassName[tone]
                        : accentClassName[metricAccent].value),
                  )}
                >
                  {value}
                </div>
                {detail && (
                  <div className="mt-0.5 truncate text-xs text-muted-foreground">
                    {detail}
                  </div>
                )}
              </div>
            </div>
          );
        })}
      </div>
    </section>
  );
}
