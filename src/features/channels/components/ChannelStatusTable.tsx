import { Eye, Play, Square } from "lucide-react";
import type { CSSProperties, ReactNode } from "react";
import { Sub2ApiPlatformIcon } from "@/components/group/Sub2ApiPlatformIcon";
import { EmptyState, IconButton, StatusBadge } from "@/components/ui";
import { groupVisualClassNames } from "@/lib/groupVisualStyles";
import { cn } from "@/lib/utils";
import {
  availabilityHue,
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
      <div className="overflow-x-auto">
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
          <tbody>
            {rows.map((row) => {
              const platformClassNames = groupVisualClassNames[row.visualPlatform];
              return (
                <tr key={row.rowKey} className="border-t border-border hover:bg-hover/70">
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
                        <div className="font-medium text-foreground">{row.latencyLabel}</div>
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
            })}
          </tbody>
        </table>
      </div>
    </div>
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
