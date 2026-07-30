import { Activity, Eye, Play, Square } from "lucide-react";
import type { ReactNode } from "react";
import { EmptyState, IconButton, StatusBadge } from "@/components/ui";
import { cn } from "@/lib/utils";
import {
  availabilityTone,
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
        description="创建或启用 monitor 后，后端 V2 read model 会在这里显示每个 Key 的独立事实行。"
      />
    );
  }

  return (
    <div className="overflow-hidden rounded-[var(--surface-radius)] border border-border bg-surface shadow-[var(--surface-shadow)]">
      <div className="overflow-x-auto">
        <table className="min-w-[1180px] w-full border-separate border-spacing-0 text-left text-sm">
          <thead className="bg-surface-subtle text-xs font-medium text-muted-foreground">
            <tr>
              <HeaderCell className="w-[230px]">Key / Station</HeaderCell>
              <HeaderCell className="w-[220px]">Model / Protocol</HeaderCell>
              <HeaderCell className="w-[170px]">Profile</HeaderCell>
              <HeaderCell className="w-[130px]">Current</HeaderCell>
              <HeaderCell className="w-[130px] text-right">Availability</HeaderCell>
              <HeaderCell className="w-[150px]">Last Probe</HeaderCell>
              <HeaderCell className="w-[230px]">Trend</HeaderCell>
              <HeaderCell className="w-[120px] text-right">Actions</HeaderCell>
            </tr>
          </thead>
          <tbody>
            {rows.map((row) => (
              <tr key={row.rowKey} className="border-t border-border hover:bg-hover/70">
                <BodyCell>
                  <div className="flex min-w-0 items-center gap-2.5">
                    <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-[8px] bg-info-surface text-info-foreground">
                      <Activity className="h-4 w-4" />
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
                  <div className="truncate text-xs text-muted-foreground">{row.protocolLabel}</div>
                </BodyCell>
                <BodyCell>
                  <div className="truncate text-xs text-muted-foreground" title={row.profileLabel}>
                    {row.profileLabel}
                  </div>
                  <div className="truncate text-xs text-muted-foreground">{row.fallbackLabel}</div>
                </BodyCell>
                <BodyCell>
                  <div className="flex items-center gap-2">
                    <StatusBadge tone={badgeTone[row.currentTone]}>{row.currentLabel}</StatusBadge>
                    {(row.dirty || row.corrupt) && (
                      <span
                        className={cn(
                          "rounded-full px-1.5 py-0.5 text-[10px]",
                          row.corrupt ? "bg-danger-surface text-danger-foreground" : "bg-warning-surface text-warning-foreground",
                        )}
                      >
                        {row.corrupt ? "corrupt" : "dirty"}
                      </span>
                    )}
                  </div>
                  <div className="mt-1 truncate text-xs text-muted-foreground" title={row.currentReason ?? undefined}>
                    {row.currentReason ?? row.attemptsLabel}
                  </div>
                </BodyCell>
                <BodyCell className="text-right">
                  <div className={cn("font-semibold", availabilityToneClass(availabilityTone(row.availabilityPercent)))}>
                    {row.availabilityLabel}
                  </div>
                  <div className="text-xs text-muted-foreground">strict {row.strictAvailabilityPercent === null ? "--" : `${row.strictAvailabilityPercent.toFixed(2)}%`}</div>
                </BodyCell>
                <BodyCell>
                  <div className="font-medium text-foreground">{row.latencyLabel}</div>
                  <div className="text-xs text-muted-foreground">{row.lastCheckedLabel}</div>
                </BodyCell>
                <BodyCell>
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
                        disabled={actionPending || !row.enabled}
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
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

function HeaderCell({ children, className }: { children: string; className?: string }) {
  return <th className={cn("border-b border-border px-3 py-2.5", className)}>{children}</th>;
}

function BodyCell({
  children,
  className,
}: {
  children: ReactNode;
  className?: string;
}) {
  return <td className={cn("border-b border-border px-3 py-2.5 align-middle", className)}>{children}</td>;
}

function availabilityToneClass(tone: ReturnType<typeof availabilityTone>) {
  if (tone === "success") return "text-success-foreground";
  if (tone === "warning") return "text-warning-foreground";
  if (tone === "danger") return "text-danger-foreground";
  return "text-muted-foreground";
}
