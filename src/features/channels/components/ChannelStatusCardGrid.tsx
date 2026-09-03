import { memo, useEffect, useMemo, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { Gauge, Timer } from "lucide-react";
import { useShellPageVirtualizerTarget } from "@/app/navigation/useShellPageVirtualizerTarget";
import { Sub2ApiPlatformIcon } from "@/components/group/Sub2ApiPlatformIcon";
import { EmptyState, StatusBadge } from "@/components/ui";
import { groupVisualClassNames } from "@/lib/groupVisualStyles";
import { cn } from "@/lib/utils";
import {
  availabilityHue,
  areChannelStatusRowViewsEqual,
  type ChannelStatusRowView,
  type StatusTone,
} from "../channelStatusViewModel";
import { StatusTrend } from "./StatusTrend";

type ChannelStatusCardGridProps = {
  rows: ChannelStatusRowView[];
  loading: boolean;
};

const badgeTone: Record<StatusTone, "healthy" | "warning" | "error" | "disabled" | "info"> = {
  available: "healthy",
  degraded: "warning",
  unavailable: "error",
  skipped: "info",
  missing: "disabled",
  running: "info",
  disabled: "disabled",
};

export function ChannelStatusCardGrid({
  rows,
  loading,
}: ChannelStatusCardGridProps) {
  const gridContainerRef = useRef<HTMLDivElement>(null);
  const [columnCount, setColumnCount] = useState(1);
  const hasRows = rows.length > 0;
  useEffect(() => {
    const updateColumns = () => {
      const width = gridContainerRef.current?.clientWidth ?? window.innerWidth;
      setColumnCount(width >= 1536 ? 4 : width >= 1280 ? 3 : width >= 768 ? 2 : 1);
    };
    updateColumns();
    window.addEventListener("resize", updateColumns);
    const resizeObserver = typeof ResizeObserver !== "undefined" && gridContainerRef.current
      ? new ResizeObserver(updateColumns)
      : null;
    if (resizeObserver && gridContainerRef.current) {
      resizeObserver.observe(gridContainerRef.current);
    }
    return () => {
      window.removeEventListener("resize", updateColumns);
      resizeObserver?.disconnect();
    };
  }, [hasRows]);

  const gridRows = useMemo(() => {
    const groups: ChannelStatusRowView[][] = [];
    for (let index = 0; index < rows.length; index += columnCount) {
      groups.push(rows.slice(index, index + columnCount));
    }
    return groups;
  }, [columnCount, rows]);
  const shouldVirtualize = gridRows.length > 20;
  const {
    targetRef: virtualListRef,
    scrollElement,
    scrollMargin,
    resolved: scrollTargetResolved,
  } = useShellPageVirtualizerTarget<HTMLDivElement>();
  const virtualizationEnabled = shouldVirtualize && scrollElement !== null;
  const renderVirtualRows = virtualizationEnabled || (shouldVirtualize && !scrollTargetResolved);
  const virtualizer = useVirtualizer({
    count: virtualizationEnabled ? gridRows.length : 0,
    getScrollElement: () => scrollElement,
    estimateSize: () => 250,
    getItemKey: (index) => gridRows[index]?.[0]?.rowKey ?? index,
    gap: 12,
    overscan: 4,
    scrollMargin,
  });
  const virtualItems = virtualizer.getVirtualItems();
  const renderedVirtualItems = virtualItems.length > 0
    ? virtualItems
    : renderVirtualRows
      ? [{ index: 0, start: scrollMargin, end: scrollMargin + 250, key: gridRows[0]?.[0]?.rowKey ?? 0 }]
      : [];
  const estimatedTotalSize = gridRows.length * 250 + Math.max(0, gridRows.length - 1) * 12;
  const virtualTotalSize = virtualizationEnabled
    ? virtualizer.getTotalSize()
    : estimatedTotalSize;
  const topPadding = Math.max(
    0,
    (renderedVirtualItems[0]?.start ?? scrollMargin) - scrollMargin,
  );
  const bottomPadding = Math.max(
    0,
    virtualTotalSize -
      ((renderedVirtualItems[renderedVirtualItems.length - 1]?.end ?? scrollMargin) -
        scrollMargin),
  );

  const renderGridRow = (gridRow: ChannelStatusRowView[], index: number) => (
    <div
      key={gridRow[0]?.rowKey ?? index}
      data-index={index}
      ref={virtualizationEnabled ? virtualizer.measureElement : undefined}
      className="grid gap-3"
      style={{ gridTemplateColumns: `repeat(${columnCount}, minmax(0, 1fr))` }}
    >
      {gridRow.map((row) => <MemoizedChannelStatusCard key={row.rowKey} row={row} />)}
    </div>
  );

  if (rows.length === 0) {
    return (
      <EmptyState
        title={loading ? "正在读取状态监控" : "暂无状态监控卡片"}
        description="创建或启用监控后，卡片视图会显示每个密钥的当前状态、可用性、延迟和趋势。"
      />
    );
  }

  return (
    <div ref={gridContainerRef} data-channel-status-card-grid>
      <div
        ref={virtualListRef}
        className="flex flex-col gap-3"
        style={renderVirtualRows ? { paddingTop: topPadding, paddingBottom: bottomPadding } : undefined}
      >
        {renderVirtualRows
          ? renderedVirtualItems.map((item) => renderGridRow(gridRows[item.index], item.index))
          : gridRows.map(renderGridRow)}
      </div>
    </div>
  );
}

type ChannelStatusCardProps = {
  row: ChannelStatusRowView;
};

const MemoizedChannelStatusCard = memo(function ChannelStatusCard({ row }: ChannelStatusCardProps) {
  const availabilityHueValue = availabilityHue(row.availabilityPercent);
  const platformClassNames = groupVisualClassNames[row.visualPlatform];

  return (
    <article className="flex h-full flex-col rounded-[var(--surface-radius)] border border-border bg-surface p-3.5 shadow-[var(--surface-shadow)]">
      <div className="flex items-start justify-between gap-3">
        <div className="flex min-w-0 items-start gap-2.5">
          <span
            className={cn(
              "flex h-9 w-9 shrink-0 items-center justify-center rounded-[8px]",
              platformClassNames.rateBadge,
            )}
            title={`${row.visualPlatformLabel}${row.groupName ? ` · ${row.groupName}` : ""}`}
          >
            <Sub2ApiPlatformIcon
              platform={row.visualPlatform}
              className={cn("h-4 w-4", platformClassNames.icon)}
            />
          </span>
          <div className="min-w-0">
            <div className="truncate text-[15px] font-semibold leading-5 text-foreground" title={row.targetName}>
              {row.targetName}
            </div>
            <div className="mt-1 flex min-w-0 items-center gap-1.5">
              <span
                className="min-w-0 truncate text-xs text-muted-foreground"
                title={`${row.stationName} · ${row.modelLabel}`}
              >
                {row.stationName} · {row.modelLabel}
              </span>
            </div>
          </div>
        </div>
        <StatusBadge
          tone={badgeTone[row.currentTone]}
          className={cn(
            "shrink-0 border-0 px-2.5",
            row.currentTone === "available" && "bg-channel-health-surface text-channel-health-label",
          )}
        >
          {row.currentLabel}
        </StatusBadge>
      </div>

      <div className="mt-3 grid grid-cols-2 gap-2">
        <MetricTile
          icon={<Timer className="h-3.5 w-3.5" />}
          label="模型延迟"
          value={row.latencyLabel}
          title={`总耗时：${row.latencyLabel}\n首包：${row.ttfbLabel}\n首字：${row.firstContentLabel}`}
        />
        <MetricTile icon={<Gauge className="h-3.5 w-3.5" />} label="端点 Ping" value={row.endpointPingLabel} />
      </div>

      <div className="mt-3 border-t border-border pt-3">
        <div className="flex items-end justify-between gap-3">
          <div className="min-w-0 pb-0.5 text-xs font-medium text-muted-foreground">
            <div>可用性</div>
          </div>
          <div
            className={cn(
              "shrink-0 text-3xl font-semibold leading-8 tracking-normal",
              availabilityHueValue === null ? "text-muted-foreground" : "text-channel-availability",
            )}
            style={availabilityHueValue === null
              ? undefined
              : ({ "--channel-availability-hue": availabilityHueValue } as CSSProperties)}
          >
            {row.availabilityLabel}
          </div>
        </div>
      </div>

      <div className="mt-2.5 border-t border-border pt-2.5">
        <div className="mb-1.5 flex items-center justify-between gap-2 text-[11px] text-muted-foreground/70">
          <span>近 60 次记录</span>
          <span className="truncate" title={row.lastCheckedLabel}>最后检查 {row.lastCheckedLabel}</span>
        </div>
        <StatusTrend cells={row.recentTrend} compact variant="bars" slotCount={60} />
        <div className="mt-1 flex justify-between text-[10px] leading-3 text-muted-foreground/70">
          <span>过去</span>
          <span>现在</span>
        </div>
      </div>

    </article>
  );
}, (previous, next) => areChannelStatusRowViewsEqual(previous.row, next.row));

function MetricTile({ icon, label, value, title }: { icon: ReactNode; label: string; value: string; title?: string }) {
  return (
    <div className="min-w-0 rounded-[8px] border border-border bg-surface-subtle px-3 py-2.5" title={title}>
      <div className="flex items-center gap-1.5 text-[11px] font-medium text-muted-foreground/70">
        {icon}
        <span className="truncate">{label}</span>
      </div>
      <div className="mt-2 truncate text-[18px] font-semibold leading-6 text-foreground" title={value}>
        {value}
      </div>
    </div>
  );
}
