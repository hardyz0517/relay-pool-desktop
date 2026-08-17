import { AlertTriangle, Clock3, Database, RefreshCw } from "lucide-react";
import { Sub2ApiPlatformIcon } from "@/components/group/Sub2ApiPlatformIcon";
import { Button, StatusBadge, type StatusTone } from "@/components/ui";
import { StatusTrend, type StatusTrendCell } from "@/components/status/StatusTrend";
import { groupVisualMetaFor } from "@/lib/groupVisualMeta";
import { groupVisualClassNames } from "@/lib/groupVisualStyles";
import { cn } from "@/lib/utils";
import type {
  StationPublishedStatusOutcome,
  StationPublishedStatusRow,
  StationPublishedStatusSourceState,
  StationPublishedStatusWorkspace,
} from "@/lib/types/stationPublishedStatus";

type StationPublishedStatusSectionProps = {
  stationName: string;
  workspace: StationPublishedStatusWorkspace | undefined;
  isLoading: boolean;
  isError: boolean;
  isRefreshing: boolean;
  isRefreshError: boolean;
  onRefresh: () => Promise<void>;
  onRetryWorkspace: () => Promise<void>;
};

const badgeTone: Record<StationPublishedStatusOutcome, StatusTone> = {
  available: "healthy",
  degraded: "warning",
  unavailable: "error",
  unknown: "disabled",
};

export function StationPublishedStatusSection({
  stationName,
  workspace,
  isLoading,
  isError,
  isRefreshing,
  isRefreshError,
  onRefresh,
  onRetryWorkspace,
}: StationPublishedStatusSectionProps) {
  const state = workspace?.sourceState;
  // A provider can withdraw the capability without changing the endpoint revision.
  // Retained rows remain diagnostic history, but must not override the current
  // unsupported state in the station detail.
  const showRows = Boolean(
    workspace && workspace.sourceState !== "unsupported" && workspace.rows.length > 0,
  );
  const latestOfficialUpdatedAtMs = latestOfficialUpdateAtMs(workspace?.rows ?? []);

  return (
    <section className="overflow-hidden rounded-[var(--surface-radius)] border border-border bg-surface shadow-[var(--surface-shadow)]">
      <div className="flex flex-wrap items-center justify-between gap-3 border-b border-border px-4 py-3">
        <div className="flex min-w-0 items-center gap-2">
          <Database className="h-4 w-4 shrink-0 text-muted-foreground" />
          <div className="min-w-0">
            <h2 className="truncate text-sm font-semibold text-foreground">官方渠道状态</h2>
            <p
              className="mt-0.5 truncate text-xs text-muted-foreground"
              title={latestOfficialUpdatedAtMs === null
                ? "数据由中转站管理端发布，不是本地主动探针。"
                : `数据由中转站管理端发布，不是本地主动探针。当前列表最新官方检查：${formatTime(latestOfficialUpdatedAtMs)}`}
            >
              站点发布的监控结果{latestOfficialUpdatedAtMs === null ? "" : ` · 官方更新时间：${formatTime(latestOfficialUpdatedAtMs)}`}
            </p>
          </div>
        </div>
        <Button
          variant="secondary"
          size="sm"
          disabled={isRefreshing}
          onClick={() => void onRefresh().catch(() => undefined)}
          title={`重新采集 ${stationName} 发布的渠道状态`}
        >
          <RefreshCw className={cn("h-3.5 w-3.5", isRefreshing && "animate-spin")} />
          重新采集
        </Button>
      </div>

      {isLoading && !workspace ? <LoadingBody /> : null}
      {!isLoading && !workspace && isError ? <FailureBody onRetryWorkspace={onRetryWorkspace} /> : null}
      {!isLoading && workspace ? (
        <>
          <SourceStateBanner
            workspace={workspace}
            refreshFailed={isRefreshError}
            workspaceReadFailed={isError}
          />
          {showRows ? <PublishedStatusTable rows={workspace.rows} /> : <SourceStateBody state={state} />}
        </>
      ) : null}
    </section>
  );
}

function LoadingBody() {
  return (
    <div className="grid min-h-[220px] gap-3 p-4" aria-label="正在读取站点发布的渠道状态">
      <div className="h-5 w-40 animate-pulse rounded-[4px] bg-muted" />
      <div className="h-[124px] animate-pulse rounded-[var(--surface-radius)] bg-surface-subtle" />
      <div className="h-[42px] animate-pulse rounded-[var(--surface-radius)] bg-surface-subtle" />
    </div>
  );
}

function FailureBody({ onRetryWorkspace }: { onRetryWorkspace: () => Promise<void> }) {
  return (
    <div className="flex min-h-[220px] flex-col items-center justify-center px-4 py-8 text-center">
      <AlertTriangle className="h-5 w-5 text-danger-foreground" />
      <div className="mt-3 text-sm font-medium text-foreground">暂时无法读取官方渠道状态</div>
      <p className="mt-1 max-w-md text-xs leading-5 text-muted-foreground">
        详情页其他信息不受影响。请稍后重试读取此区段。
      </p>
      <Button className="mt-3" variant="secondary" size="sm" onClick={() => void onRetryWorkspace().catch(() => undefined)}>
        <RefreshCw className="h-3.5 w-3.5" />
        重试
      </Button>
    </div>
  );
}

function SourceStateBanner({
  workspace,
  refreshFailed,
  workspaceReadFailed,
}: {
  workspace: StationPublishedStatusWorkspace;
  refreshFailed: boolean;
  workspaceReadFailed: boolean;
}) {
  const stale = workspace.stale;
  const partial = workspace.completeness === "partial" || workspace.sourceState === "degraded";
  if (!workspaceReadFailed && !refreshFailed && !stale && !partial && workspace.sourceState !== "failed" && workspace.sourceState !== "authorization_required") {
    return null;
  }

  const message = workspaceReadFailed
    ? "最新官方状态读取失败；正在显示上次读取的结果。"
    : refreshFailed
    ? "本次官方状态采集未完成；已保留最近一次可用结果。"
    : workspace.sourceState === "authorization_required"
    ? "站点管理端需要重新授权；已保留上次成功采集的官方状态。"
    : workspace.sourceState === "failed"
      ? "最近一次官方状态采集失败；已保留上次成功结果。"
      : partial
        ? "部分站点发布的监控记录未能解析，以下内容为已验证的结果。"
        : "站点发布的状态记录可能已过期。";

  return (
    <div className="flex items-start gap-2 border-b border-warning-border bg-warning-surface px-4 py-2.5 text-xs text-warning-foreground">
      <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
      <span>{message}</span>
    </div>
  );
}

function SourceStateBody({ state }: { state: StationPublishedStatusSourceState | undefined }) {
  const content = sourceStateContent(state);
  return (
    <div className="flex min-h-[220px] flex-col items-center justify-center px-4 py-8 text-center">
      <Clock3 className={cn("h-5 w-5", content.iconClassName)} />
      <div className="mt-3 text-sm font-medium text-foreground">{content.title}</div>
      <p className="mt-1 max-w-md text-xs leading-5 text-muted-foreground">{content.description}</p>
    </div>
  );
}

function sourceStateContent(state: StationPublishedStatusSourceState | undefined) {
  if (state === "never_collected") {
    return {
      title: "尚未采集官方渠道状态",
      description: "此区段只读取站点管理端发布的结果；开始采集后会显示最近 60 次官方记录。",
      iconClassName: "text-muted-foreground",
    };
  }
  if (state === "empty") {
    return {
      title: "站点未发布监控",
      description: "该站点当前没有可展示的官方渠道监控记录。",
      iconClassName: "text-muted-foreground",
    };
  }
  if (state === "unsupported") {
    return {
      title: "当前站点不支持官方渠道状态",
      description: "该站点类型或版本没有可用的结构化官方状态接口。",
      iconClassName: "text-muted-foreground",
    };
  }
  if (state === "authorization_required") {
    return {
      title: "需要重新授权",
      description: "请完成站点窗口授权后重新采集官方渠道状态。",
      iconClassName: "text-warning-foreground",
    };
  }
  if (state === "failed") {
    return {
      title: "官方状态采集失败",
      description: "本次失败不会清除之前保存的官方结果；可稍后重试。",
      iconClassName: "text-danger-foreground",
    };
  }
  return {
    title: "暂无站点发布的渠道状态",
    description: "重新采集后将显示站点管理端已发布的监控结果。",
    iconClassName: "text-muted-foreground",
  };
}

function PublishedStatusTable({ rows }: { rows: StationPublishedStatusRow[] }) {
  return (
    <div className="overflow-x-auto">
      <table className="min-w-[1000px] w-full table-fixed border-collapse text-left text-xs">
        <colgroup>
          <col className="w-[20%]" />
          <col className="w-[14%]" />
          <col className="w-[10%]" />
          <col className="w-[12%]" />
          <col className="w-[12%]" />
          <col className="w-[32%]" />
        </colgroup>
        <thead className="border-b border-border bg-surface-subtle text-muted-foreground">
          <tr>
            <TableHead>监控 / 分组</TableHead>
            <TableHead>模型</TableHead>
            <TableHead>当前状态</TableHead>
            <TableHead>最近可用性</TableHead>
            <TableHead>延迟 / Ping</TableHead>
            <TableHead>最近 60 次</TableHead>
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => {
            const visualMeta = monitorVisualMeta(row);
            const visualClassNames = groupVisualClassNames[visualMeta.platform];
            const groupLabel = monitorGroupLabel(row);
            return (
              <tr key={row.rowKey} className="border-b border-border transition-colors hover:bg-hover/70">
                <TableCell>
                  <div className="flex min-w-0 items-center gap-2.5">
                    <span
                      className={cn(
                        "flex h-8 w-8 shrink-0 items-center justify-center rounded-[8px]",
                        visualClassNames.rateBadge,
                      )}
                      title={`监控类型：${visualMeta.label} · ${row.provider}`}
                    >
                      <Sub2ApiPlatformIcon
                        platform={visualMeta.platform}
                        className={cn("h-4 w-4", visualClassNames.icon)}
                      />
                    </span>
                    <div className="min-w-0">
                      <div className="truncate font-medium text-foreground" title={row.name}>{row.name}</div>
                      {groupLabel ? (
                        <div className="mt-1 truncate text-muted-foreground" title={groupLabel}>
                          {groupLabel}
                        </div>
                      ) : null}
                    </div>
                  </div>
                </TableCell>
                <TableCell>
                  <div className="truncate text-foreground" title={modelTitle(row)}>{modelLabel(row)}</div>
                </TableCell>
                <TableCell>
                  <span title="当前状态来自站点发布的监控结果。">
                    <StatusBadge tone={badgeTone[row.currentOutcome]}>{outcomeLabel(row.currentOutcome)}</StatusBadge>
                  </span>
                </TableCell>
                <TableCell>
                  <span
                    className={row.recentAvailabilityPercent === null ? "text-muted-foreground" : "font-medium text-channel-availability"}
                    title="根据站点发布的最近 60 条监控记录计算。"
                  >
                    {formatAvailability(row.recentAvailabilityPercent)}
                  </span>
                </TableCell>
                <TableCell>
                  <div className="font-medium text-foreground">{formatLatency(row.currentLatencyMs)}</div>
                  <div className="mt-1 text-muted-foreground">Ping {formatLatency(row.currentPingLatencyMs)}</div>
                </TableCell>
                <TableCell className="pr-4">
                  <StatusTrend
                    cells={publishedStatusTrendCells(row)}
                    slotCount={60}
                    ariaLabel={`${row.name} 的站点发布最近 60 次状态记录`}
                  />
                </TableCell>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

function TableHead({ children }: { children: string }) {
  return <th className="h-9 whitespace-nowrap px-3 font-medium">{children}</th>;
}

function TableCell({ children, className }: { children: React.ReactNode; className?: string }) {
  return <td className={cn("px-3 py-2.5 align-middle", className)}>{children}</td>;
}

function monitorGroupLabel(row: StationPublishedStatusRow) {
  return row.groupName?.trim() || null;
}

function monitorVisualMeta(row: StationPublishedStatusRow) {
  return groupVisualMetaFor(
    [row.groupName, row.primaryModel, row.provider].filter(Boolean).join(" "),
    { provider: row.provider },
  );
}

function modelLabel(row: StationPublishedStatusRow) {
  return row.extraModels.length > 0 ? `${row.primaryModel} +${row.extraModels.length}` : row.primaryModel;
}

function modelTitle(row: StationPublishedStatusRow) {
  return row.extraModels.length > 0 ? [row.primaryModel, ...row.extraModels].join("\n") : row.primaryModel;
}

function outcomeLabel(outcome: StationPublishedStatusOutcome) {
  if (outcome === "available") return "正常";
  if (outcome === "degraded") return "降级";
  if (outcome === "unavailable") return "错误";
  return "未知";
}

function publishedStatusTrendCells(row: StationPublishedStatusRow): StatusTrendCell[] {
  return row.recentSamples.map((sample) => ({
    id: sample.id,
    tone: sample.outcome === "unknown" ? "missing" : sample.outcome,
    label: `来源：站点发布\n模型：${sample.model}\n检查时间：${formatTime(sample.checkedAtMs)}\n状态：${outcomeLabel(sample.outcome)}\n延迟：${formatLatency(sample.latencyMs)}\nPing：${formatLatency(sample.pingLatencyMs)}`,
    modelLabel: sample.model,
    timeLabel: `官方检查：${formatTime(sample.checkedAtMs)}`,
    availabilityLabel: `状态：${outcomeLabel(sample.outcome)}`,
    latencyLabel: formatLatency(sample.latencyMs),
    metricLabel: `延迟：${formatLatency(sample.latencyMs)} · Ping：${formatLatency(sample.pingLatencyMs)}`,
  }));
}

function latestOfficialUpdateAtMs(rows: StationPublishedStatusRow[]) {
  return rows.reduce<number | null>((latest, row) => {
    const checkedAtMs = row.upstreamCheckedAtMs;
    if (checkedAtMs === null || !Number.isFinite(checkedAtMs)) return latest;
    return latest === null || checkedAtMs > latest ? checkedAtMs : latest;
  }, null);
}

function formatAvailability(value: number | null) {
  return value === null || !Number.isFinite(value) ? "--" : `${value.toFixed(2)}%`;
}

function formatLatency(value: number | null) {
  return value === null || !Number.isFinite(value) ? "--" : `${value} ms`;
}

function formatTime(value: number | null) {
  if (value === null) return "--";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "--";
  return date.toLocaleString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  });
}
