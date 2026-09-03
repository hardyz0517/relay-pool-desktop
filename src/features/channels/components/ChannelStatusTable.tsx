import { useVirtualizer } from "@tanstack/react-virtual";
import { Eye, Play, Square } from "lucide-react";
import { forwardRef, memo, type CSSProperties, type ReactNode } from "react";
import { useShellPageVirtualizerTarget } from "@/app/navigation/useShellPageVirtualizerTarget";
import { Sub2ApiPlatformIcon } from "@/components/group/Sub2ApiPlatformIcon";
import { EmptyState, IconButton, StatusBadge } from "@/components/ui";
import { groupVisualClassNames } from "@/lib/groupVisualStyles";
import { cn } from "@/lib/utils";
import {
  availabilityHue,
  areChannelStatusRowViewsEqual,
  statusLabel,
  type ChannelStatusRowView,
  type StatusTone,
} from "../channelStatusViewModel";
import { StatusTrend } from "./StatusTrend";

type ChannelStatusTableProps = {
  rows: ChannelStatusRowView[];
  loading: boolean;
  actionPending: boolean;
  onRunNow: (row: ChannelStatusRowView) => void;
  onCancel: (executionId: string) => void;
  onOpenExecution: (executionId: string) => void;
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

export function ChannelStatusTable({
  rows,
  loading,
  actionPending,
  onRunNow,
  onCancel,
  onOpenExecution,
}: ChannelStatusTableProps) {
  const shouldVirtualize = rows.length > 80;
  const {
    targetRef: virtualListRef,
    scrollElement,
    scrollMargin,
    resolved: scrollTargetResolved,
  } = useShellPageVirtualizerTarget<HTMLTableSectionElement>();
  const virtualizationEnabled = shouldVirtualize && scrollElement !== null;
  const renderVirtualRows = virtualizationEnabled || (shouldVirtualize && !scrollTargetResolved);
  const virtualizer = useVirtualizer({
    count: virtualizationEnabled ? rows.length : 0,
    getScrollElement: () => scrollElement,
    estimateSize: () => 72,
    getItemKey: (index) => rows[index]?.rowKey ?? index,
    overscan: 8,
    scrollMargin,
  });
  const virtualItems = virtualizer.getVirtualItems();
  const renderedVirtualItems = virtualItems.length > 0
    ? virtualItems
    : renderVirtualRows
      ? [{ index: 0, start: scrollMargin, end: scrollMargin + 72, key: rows[0]?.rowKey ?? 0 }]
      : [];
  const virtualTotalSize = virtualizationEnabled
    ? virtualizer.getTotalSize()
    : rows.length * 72;

  if (rows.length === 0) {
    return (
      <EmptyState
        title={loading ? "正在读取状态监控" : "暂无状态监控行"}
        description="创建或启用 monitor 后，后端 V2 read model 会在这里显示每个密钥的独立事实行。"
      />
    );
  }

  return (
    <div className="overflow-hidden rounded-[var(--surface-radius)] border border-border bg-surface shadow-[var(--surface-shadow)]">
      <div className="overflow-x-auto" data-channel-status-horizontal-scroll>
        <table className="min-w-[1080px] w-full table-fixed border-collapse bg-surface text-left text-sm">
          <colgroup>
            <col className="w-[18%]" />
            <col className="w-[9%]" />
            <col className="w-[9%]" />
            <col className="w-[9%]" />
            <col className="w-[9%]" />
            <col className="w-[40%]" />
            <col className="w-[6%]" />
          </colgroup>
          <thead className="border-b border-border bg-surface text-xs font-medium text-muted-foreground">
            <tr>
              <HeaderCell>密钥 / 站点</HeaderCell>
              <HeaderCell>模型</HeaderCell>
              <HeaderCell>当前状态</HeaderCell>
              <HeaderCell>可用性</HeaderCell>
              <HeaderCell>最近探测</HeaderCell>
              <HeaderCell>趋势</HeaderCell>
              <HeaderCell className="text-right">操作</HeaderCell>
            </tr>
          </thead>
          <tbody ref={virtualListRef}>
            {renderVirtualRows ? (
              <>
                <VirtualSpacer
                  height={Math.max(
                    0,
                    (renderedVirtualItems[0]?.start ?? scrollMargin) - scrollMargin,
                  )}
                />
                {renderedVirtualItems.map((virtualRow) => (
                  <MemoizedChannelStatusTableRow
                    key={rows[virtualRow.index].rowKey}
                    data-index={virtualRow.index}
                    ref={virtualizationEnabled ? virtualizer.measureElement : undefined}
                    row={rows[virtualRow.index]}
                    actionPending={actionPending}
                    onRunNow={onRunNow}
                    onCancel={onCancel}
                    onOpenExecution={onOpenExecution}
                  />
                ))}
                <VirtualSpacer
                  height={Math.max(
                    0,
                    virtualTotalSize -
                      ((renderedVirtualItems[renderedVirtualItems.length - 1]?.end ?? scrollMargin) -
                        scrollMargin),
                  )}
                />
              </>
            ) : rows.map((row) => (
              <MemoizedChannelStatusTableRow
                key={row.rowKey}
                row={row}
                actionPending={actionPending}
                onRunNow={onRunNow}
                onCancel={onCancel}
                onOpenExecution={onOpenExecution}
              />
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

type ChannelStatusTableRowProps = {
  row: ChannelStatusRowView;
  actionPending: boolean;
  "data-index"?: number;
  onRunNow: (row: ChannelStatusRowView) => void;
  onCancel: (executionId: string) => void;
  onOpenExecution: (executionId: string) => void;
};

const ChannelStatusTableRow = forwardRef<HTMLTableRowElement, ChannelStatusTableRowProps>(function ChannelStatusTableRow({
  row,
  actionPending,
  "data-index": dataIndex,
  onRunNow,
  onCancel,
  onOpenExecution,
}, ref) {
  const platformClassNames = groupVisualClassNames[row.visualPlatform];
  return (
    <tr
      ref={ref}
      data-index={dataIndex}
      key={row.rowKey}
      className="border-t border-border hover:bg-hover/70"
    >
      <BodyCell>
        <div className="flex min-w-0 items-center gap-2.5">
          <span
            className={cn(
              "flex h-8 w-8 shrink-0 items-center justify-center rounded-[8px]",
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
            <div className="truncate font-medium text-foreground" title={row.targetName}>
              {row.targetName}
            </div>
            <div className="truncate text-xs text-muted-foreground" title={row.stationName}>
              {row.stationName} · {row.monitorName}
            </div>
          </div>
        </div>
      </BodyCell>
      <BodyCell>
        <div className="truncate text-foreground" title={row.modelLabel}>{row.modelLabel}</div>
      </BodyCell>
      <BodyCell>
        <div className="flex items-center gap-2">
          <StatusBadge tone={badgeTone[row.currentTone]}>{row.currentLabel}</StatusBadge>
          {row.corrupt && (
            <span className="rounded-full bg-danger-surface px-1.5 py-0.5 text-[10px] text-danger-foreground">
              数据异常
            </span>
          )}
        </div>
      </BodyCell>
      <BodyCell>
        <div
          className={cn(
            "font-semibold",
            row.availabilityPercent === null ? "text-muted-foreground" : "text-channel-availability",
          )}
          style={availabilityColorStyle(row.availabilityPercent)}
        >
          {row.availabilityLabel}
        </div>
      </BodyCell>
      <BodyCell>
        <div
          className="flex items-center gap-2"
          title={`最近探测：${statusLabel(row.latestProbeTone)}\n总耗时：${row.latencyLabel}\n首包：${row.ttfbLabel}\n首字：${row.firstContentLabel}`}
        >
          <span
            role="img"
            aria-label={`最近探测：${statusLabel(row.latestProbeTone)}`}
            className={cn("h-2 w-2 shrink-0 rounded-full", probeDotClassName[row.latestProbeTone])}
          />
          <div>
            <div className="whitespace-nowrap font-medium tabular-nums text-foreground">{row.latencyLabel}</div>
            <div className="whitespace-nowrap text-[11px] text-muted-foreground">{row.lastCheckedLabel}</div>
          </div>
        </div>
      </BodyCell>
      <BodyCell className="pr-5">
        <StatusTrend cells={row.trend} />
      </BodyCell>
      <BodyCell className="text-right">
        <div className="flex justify-end gap-1">
          {row.runningExecutionId ? (
            <IconButton
              label="取消执行"
              disabled={actionPending}
              onClick={() => onCancel(row.runningExecutionId!)}
            >
              <Square className="h-4 w-4" />
            </IconButton>
          ) : (
            <IconButton
              label="立即运行"
              disabled={actionPending}
              onClick={() => onRunNow(row)}
            >
              <Play className="h-4 w-4" />
            </IconButton>
          )}
          <IconButton
            label="查看执行"
            disabled={!row.latestExecutionId && !row.runningExecutionId}
            onClick={() => onOpenExecution(row.runningExecutionId ?? row.latestExecutionId!)}
          >
            <Eye className="h-4 w-4" />
          </IconButton>
        </div>
      </BodyCell>
    </tr>
  );
});

const MemoizedChannelStatusTableRow = memo(
  ChannelStatusTableRow,
  (previous, next) => previous.actionPending === next.actionPending
    && previous.onRunNow === next.onRunNow
    && previous.onCancel === next.onCancel
    && previous.onOpenExecution === next.onOpenExecution
    && areChannelStatusRowViewsEqual(previous.row, next.row),
);

function VirtualSpacer({ height }: { height: number }) {
  if (height <= 0) return null;
  return (
    <tr aria-hidden="true" role="presentation">
      <td colSpan={7} style={{ height, padding: 0, border: 0 }} />
    </tr>
  );
}

function HeaderCell({ children, className }: { children: string; className?: string }) {
  return <th className={cn("h-8 whitespace-nowrap px-3", className)}>{children}</th>;
}

const probeDotClassName: Record<StatusTone, string> = {
  available: "bg-channel-health-bar",
  degraded: "bg-channel-health-degraded-bar",
  unavailable: "bg-channel-health-danger-bar",
  skipped: "bg-channel-health-empty-bar",
  missing: "bg-channel-health-empty-bar",
  running: "bg-channel-health-empty-bar",
  disabled: "bg-channel-health-empty-bar",
};

function BodyCell({
  children,
  className,
}: {
  children: ReactNode;
  className?: string;
}) {
  return <td className={cn("border-b border-border px-3 py-2.5 align-middle", className)}>{children}</td>;
}

function availabilityColorStyle(value: number | null): CSSProperties | undefined {
  const hue = availabilityHue(value);
  return hue === null
    ? undefined
    : ({ "--channel-availability-hue": hue } as CSSProperties);
}
