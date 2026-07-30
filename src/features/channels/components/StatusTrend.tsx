import { cn } from "@/lib/utils";
import type { TrendCellTone, TrendCellView } from "../channelStatusViewModel";

const trendToneClassName: Record<TrendCellTone, string> = {
  available: "bg-success-foreground/70",
  degraded: "bg-warning-foreground/70",
  unavailable: "bg-danger-foreground/75",
  skipped: "bg-info-foreground/35",
  missing: "bg-muted-foreground/25",
  dirty: "bg-warning-foreground/45",
  corrupt: "bg-danger-solid",
};

type StatusTrendProps = {
  cells: TrendCellView[];
  compact?: boolean;
};

export function StatusTrend({ cells, compact = false }: StatusTrendProps) {
  return (
    <div
      className={cn(
        "grid min-w-[180px] items-stretch gap-[2px]",
        compact ? "h-4" : "h-6",
      )}
      style={{ gridTemplateColumns: `repeat(${Math.max(cells.length, 1)}, minmax(3px, 1fr))` }}
      aria-label="监控趋势"
    >
      {cells.length === 0 ? (
        <span className="rounded-[3px] bg-muted" title="暂无后端趋势数据" />
      ) : (
        cells.map((cell) => (
          <span
            key={cell.id}
            className={cn("rounded-[3px]", trendToneClassName[cell.tone])}
            title={cell.title}
            aria-label={cell.label}
          />
        ))
      )}
    </div>
  );
}
