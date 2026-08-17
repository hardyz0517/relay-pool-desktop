import { useState } from "react";
import { cn } from "@/lib/utils";

export type StatusTrendTone =
  | "available"
  | "degraded"
  | "unavailable"
  | "skipped"
  | "missing"
  | "dirty"
  | "corrupt";

export type StatusTrendCell = {
  id: string;
  tone: StatusTrendTone;
  label: string;
  modelLabel: string;
  timeLabel: string;
  availabilityLabel: string;
  latencyLabel: string;
  metricLabel?: string;
};

type StatusTrendProps = {
  cells: StatusTrendCell[];
  compact?: boolean;
  variant?: "blocks" | "bars";
  slotCount?: number;
  ariaLabel?: string;
};

const toneClassName: Record<StatusTrendTone, string> = {
  available: "bg-channel-health-bar",
  degraded: "bg-channel-health-degraded-bar",
  unavailable: "bg-channel-health-danger-bar",
  skipped: "bg-channel-health-empty-bar",
  missing: "bg-channel-health-empty-bar",
  dirty: "bg-channel-health-degraded-bar",
  corrupt: "bg-channel-health-danger-bar",
};

const barHeight: Record<StatusTrendTone, string> = {
  available: "100%",
  degraded: "65%",
  unavailable: "35%",
  skipped: "50%",
  missing: "15%",
  dirty: "55%",
  corrupt: "35%",
};

type HoveredCell = {
  cell: StatusTrendCell;
  x: number;
  y: number;
  placement: "top" | "bottom";
};

// Fixed-slot timeline semantics are adapted from Wei-Shaw/sub2api MonitorTimeline.vue (LGPL-3.0).
// The caller supplies display-only cells, keeping published and active monitoring facts separate.
export function StatusTrend({
  cells,
  compact = false,
  variant = "blocks",
  slotCount,
  ariaLabel = "状态趋势",
}: StatusTrendProps) {
  const [hoveredCell, setHoveredCell] = useState<HoveredCell | null>(null);
  const bars = variant === "bars";
  const fixedSlotCount = slotCount ? Math.max(1, Math.floor(slotCount)) : null;
  const visibleCells = fixedSlotCount ? cells.slice(-fixedSlotCount) : cells;
  const leadingEmptySlots = fixedSlotCount
    ? Array.from({ length: fixedSlotCount - visibleCells.length }, () => null)
    : [];
  const timelineSlots: Array<StatusTrendCell | null> = [...leadingEmptySlots, ...visibleCells];
  const gridSlotCount = fixedSlotCount ?? Math.max(cells.length, 1);

  return (
    <div
      className={cn(
        bars
          ? "flex w-full min-w-0 items-end gap-[2px]"
          : "grid w-full min-w-[180px] items-stretch gap-[2px]",
        compact ? "h-5" : "h-6",
      )}
      style={bars ? undefined : { gridTemplateColumns: `repeat(${gridSlotCount}, minmax(3px, 1fr))` }}
      aria-label={ariaLabel}
    >
      {timelineSlots.length === 0 ? (
        <span className="min-h-[3px] rounded-[2px] bg-muted" aria-label="暂无趋势数据" />
      ) : (
        timelineSlots.map((cell, index) => (
          <span
            key={cell?.id ?? `empty-${index}`}
            className={cn(
              bars ? "min-h-[3px] min-w-0 flex-1 rounded-[2px]" : "rounded-[3px]",
              cell && "cursor-default transition-[filter,opacity] duration-150 ease-out hover:brightness-110 hover:opacity-90 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/30 motion-reduce:transition-none",
              toneClassName[cell?.tone ?? "missing"],
            )}
            style={bars ? { height: barHeight[cell?.tone ?? "missing"] } : undefined}
            tabIndex={cell ? 0 : undefined}
            onPointerEnter={cell ? (event) => setHoveredCell(tooltipAnchor(cell, event.currentTarget)) : undefined}
            onPointerMove={cell ? (event) => setHoveredCell(tooltipAnchor(cell, event.currentTarget)) : undefined}
            onPointerLeave={cell ? () => setHoveredCell(null) : undefined}
            onFocus={cell ? (event) => setHoveredCell(tooltipAnchor(cell, event.currentTarget)) : undefined}
            onBlur={cell ? () => setHoveredCell(null) : undefined}
            aria-label={cell?.label ?? "无记录"}
          />
        ))
      )}
      {hoveredCell ? <TrendTooltip hoveredCell={hoveredCell} /> : null}
    </div>
  );
}

function tooltipAnchor(cell: StatusTrendCell, element: HTMLElement): HoveredCell {
  const rect = element.getBoundingClientRect();
  const centerX = rect.left + rect.width / 2;
  const tooltipHalfWidth = 105;
  const viewportWidth = typeof window === "undefined" ? 0 : window.innerWidth;
  const x = viewportWidth > 0
    ? Math.min(Math.max(centerX, tooltipHalfWidth + 8), viewportWidth - tooltipHalfWidth - 8)
    : centerX;
  const placement = rect.top < 130 ? "bottom" : "top";
  return { cell, x, y: placement === "top" ? rect.top - 8 : rect.bottom + 8, placement };
}

function TrendTooltip({ hoveredCell }: { hoveredCell: HoveredCell }) {
  const { cell, x, y, placement } = hoveredCell;
  return (
    <div
      role="tooltip"
      className={cn(
        "pointer-events-none fixed z-[120] w-[210px] rounded-[var(--surface-radius)] border border-border bg-popover px-3 py-2 text-xs text-foreground shadow-popover",
        placement === "top" ? "-translate-x-1/2 -translate-y-full" : "-translate-x-1/2",
      )}
      style={{ left: x, top: y }}
    >
      <div className="flex items-center gap-2 border-b border-border pb-1.5 font-semibold text-channel-health-label">
        <span className={cn("h-2 w-2 rounded-full", toneClassName[cell.tone])} />
        <span className="min-w-0 truncate">{cell.modelLabel}</span>
      </div>
      <div className="mt-2 grid gap-1.5 text-muted-foreground">
        <div className="truncate">{cell.timeLabel}</div>
        <div className="flex items-center gap-1.5">
          <span className={cn("h-2 w-2 rounded-full", toneClassName[cell.tone])} />
          <span>{cell.availabilityLabel}</span>
        </div>
        <div className="font-medium text-channel-health-emphasis">{cell.metricLabel ?? `延迟 ${cell.latencyLabel}`}</div>
      </div>
    </div>
  );
}
