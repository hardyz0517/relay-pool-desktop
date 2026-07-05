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
};

type MetricPanelProps = {
  title?: string;
  description?: string;
  metrics: MetricItem[];
  className?: string;
};

const toneClassName: Record<MetricTone, string> = {
  neutral: "text-slate-700",
  good: "text-emerald-700",
  warning: "text-amber-700",
  danger: "text-rose-700",
};

const accentClassName: Record<MetricAccent, { icon: string; value: string }> = {
  slate: {
    icon: "bg-slate-100 text-slate-600",
    value: "text-slate-800",
  },
  emerald: {
    icon: "bg-emerald-100 text-emerald-700",
    value: "text-emerald-700",
  },
  green: {
    icon: "bg-green-100 text-green-700",
    value: "text-green-700",
  },
  blue: {
    icon: "bg-blue-100 text-blue-700",
    value: "text-blue-700",
  },
  amber: {
    icon: "bg-amber-100 text-amber-700",
    value: "text-amber-700",
  },
  indigo: {
    icon: "bg-indigo-100 text-indigo-700",
    value: "text-indigo-700",
  },
  violet: {
    icon: "bg-violet-100 text-violet-700",
    value: "text-violet-700",
  },
  purple: {
    icon: "bg-purple-100 text-purple-700",
    value: "text-purple-700",
  },
  rose: {
    icon: "bg-rose-100 text-rose-700",
    value: "text-rose-700",
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
            <h2 className="truncate text-[13px] font-semibold text-slate-800">
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
      <div className="grid gap-2 rounded-[var(--surface-radius)] bg-white p-3 shadow-[var(--surface-shadow)] sm:grid-cols-2 md:grid-cols-4">
        {metrics.map(({ label, value, detail, icon: Icon, tone = "neutral", accent }) => {
          const metricAccent = accent ?? toneAccent[tone];
          const shouldUseToneValue = tone === "warning" || tone === "danger";

          return (
            <div
              key={label}
              className="flex min-h-[64px] items-center gap-2.5 rounded-[8px] border border-border bg-white px-2.5 py-2 shadow-[0_1px_2px_rgba(15,23,42,0.04)]"
            >
              {Icon && (
                <div
                  className={cn(
                    "flex h-8 w-8 shrink-0 items-center justify-center rounded-[8px]",
                    accentClassName[metricAccent].icon,
                  )}
                >
                  <Icon className="h-3.5 w-3.5" />
                </div>
              )}
              <div className="min-w-0 flex-1">
                <div className="truncate text-xs text-muted-foreground">
                  {label}
                </div>
                <div
                  className={cn(
                    "mt-0.5 truncate text-[18px] font-semibold leading-6",
                    shouldUseToneValue
                      ? toneClassName[tone]
                      : accentClassName[metricAccent].value,
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
