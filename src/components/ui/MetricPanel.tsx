import type { ReactNode } from "react";
import type { LucideIcon } from "lucide-react";
import { cn } from "@/lib/utils";

export type MetricTone = "neutral" | "good" | "warning" | "danger";

export type MetricItem = {
  label: string;
  value: ReactNode;
  detail?: ReactNode;
  icon?: LucideIcon;
  tone?: MetricTone;
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

const iconClassName: Record<MetricTone, string> = {
  neutral: "bg-slate-100",
  good: "bg-emerald-50",
  warning: "bg-amber-50",
  danger: "bg-rose-50",
};

export function MetricPanel({
  title,
  description,
  metrics,
  className,
}: MetricPanelProps) {
  return (
    <section
      className={cn(
        "overflow-hidden rounded-[var(--surface-radius)] border border-border bg-white shadow-[var(--surface-shadow)]",
        className,
      )}
    >
      {(title || description) && (
        <header className="border-b border-border px-4 py-3">
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
      <div className="grid gap-3 p-4 sm:grid-cols-2 xl:grid-cols-4">
        {metrics.map(({ label, value, detail, icon: Icon, tone = "neutral" }) => (
          <div
            key={label}
            className="flex min-h-[78px] items-center gap-3 rounded-[8px] border border-border bg-slate-50/60 px-3 py-2.5"
          >
            {Icon && (
              <div
                className={cn(
                  "flex h-9 w-9 shrink-0 items-center justify-center rounded-[10px]",
                  iconClassName[tone],
                  toneClassName[tone],
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
                  "mt-0.5 truncate text-[21px] font-semibold leading-7",
                  toneClassName[tone],
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
        ))}
      </div>
    </section>
  );
}
