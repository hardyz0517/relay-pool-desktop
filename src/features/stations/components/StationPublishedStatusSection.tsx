import { AlertTriangle, ChevronDown, Clock3, Database, RefreshCw } from "lucide-react";
import { useState } from "react";
import { Sub2ApiPlatformIcon } from "@/components/group/Sub2ApiPlatformIcon";
import { Button, SegmentedControl, StatusBadge, type StatusTone } from "@/components/ui";
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
  stationType?: string;
  workspace: StationPublishedStatusWorkspace | undefined;
  isLoading: boolean;
  isError: boolean;
  isRefreshing: boolean;
  isRefreshError: boolean;
  onRefresh: () => Promise<void>;
  onRetryWorkspace: () => Promise<void>;
};

type NewApiStatusView = "model" | "group";

const badgeTone: Record<StationPublishedStatusOutcome, StatusTone> = {
  available: "healthy",
  degraded: "warning",
  unavailable: "error",
  unknown: "disabled",
};

export function StationPublishedStatusSection({
  stationName,
  stationType = "sub2api",
  workspace,
  isLoading,
  isError,
  isRefreshing,
  isRefreshError,
  onRefresh,
  onRetryWorkspace,
}: StationPublishedStatusSectionProps) {
  const state = workspace?.sourceState;
  const isNewApi = stationType.trim().toLowerCase() === "newapi";
  const [newApiView, setNewApiView] = useState<NewApiStatusView>("model");
  // A provider can withdraw the capability without changing the endpoint revision.
  // Retained rows remain diagnostic history, but must not override the current
  // unsupported state in the station detail.
  const showRows = Boolean(
    workspace && workspace.sourceState !== "unsupported" && workspace.rows.length > 0,
  );
  const latestOfficialUpdatedAtMs = latestOfficialUpdateAtMs(workspace?.rows ?? []);
  const sourceDescription = isNewApi
    ? "数据来自 NewAPI 管理端性能接口，不是本地主动探针。"
    : "数据由中转站管理端发布，不是本地主动探针。";

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
                ? sourceDescription
                : `${sourceDescription}当前列表${isNewApi ? "最新数据" : "最新官方检查"}：${formatTime(latestOfficialUpdatedAtMs)}`}
            >
              {isNewApi ? "NewAPI 管理端性能数据" : "站点发布的监控结果"}{latestOfficialUpdatedAtMs === null ? "" : ` · ${isNewApi ? "最近数据" : "官方更新时间"}：${formatTime(latestOfficialUpdatedAtMs)}`}
            </p>
          </div>
        </div>
        <div className="flex shrink-0 items-center gap-2">
          {isNewApi && showRows ? (
            <SegmentedControl
              value={newApiView}
              options={[{ value: "model", label: "按模型" }, { value: "group", label: "按分组" }]}
              onChange={setNewApiView}
              ariaLabel="NewAPI 状态视图"
            />
          ) : null}
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
      </div>

      {isLoading && !workspace ? <LoadingBody /> : null}
      {!isLoading && !workspace && isError ? <FailureBody onRetryWorkspace={onRetryWorkspace} /> : null}
      {!isLoading && workspace ? (
        <>
          <SourceStateBanner
            workspace={workspace}
            isNewApi={isNewApi}
            refreshFailed={isRefreshError}
            workspaceReadFailed={isError}
          />
          {showRows ? (
            isNewApi ? (
              <NewApiGroupedStatusViews rows={workspace.rows} view={newApiView} />
            ) : (
              <PublishedStatusTable rows={workspace.rows} />
            )
          ) : <SourceStateBody state={state} isNewApi={isNewApi} />}
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
  isNewApi,
  refreshFailed,
  workspaceReadFailed,
}: {
  workspace: StationPublishedStatusWorkspace;
  isNewApi: boolean;
  refreshFailed: boolean;
  workspaceReadFailed: boolean;
}) {
  const stale = workspace.stale;
  const partial = workspace.completeness === "partial" || workspace.sourceState === "degraded";
  if (!workspaceReadFailed && !refreshFailed && !stale && !partial && workspace.sourceState !== "failed" && workspace.sourceState !== "authorization_required") {
    return null;
  }

  const noun = isNewApi ? "NewAPI 性能数据" : "官方状态";
  const message = workspaceReadFailed
    ? `最新${noun}读取失败；正在显示上次读取的结果。`
    : refreshFailed
    ? `本次${noun}采集未完成；已保留最近一次可用结果。`
    : workspace.sourceState === "authorization_required"
    ? `站点管理端需要重新授权；已保留上次成功采集的${noun}。`
    : workspace.sourceState === "failed"
      ? `最近一次${noun}采集失败；已保留上次成功结果。`
      : partial
        ? `${isNewApi ? "部分 NewAPI 性能记录" : "部分站点发布的监控记录"}未能解析，以下内容为已验证的结果。`
        : `${isNewApi ? "NewAPI 性能记录" : "站点发布的状态记录"}可能已过期。`;

  return (
    <div className="flex items-start gap-2 border-b border-warning-border bg-warning-surface px-4 py-2.5 text-xs text-warning-foreground">
      <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
      <span>{message}</span>
    </div>
  );
}

function SourceStateBody({ state, isNewApi }: { state: StationPublishedStatusSourceState | undefined; isNewApi: boolean }) {
  const content = sourceStateContent(state, isNewApi);
  return (
    <div className="flex min-h-[220px] flex-col items-center justify-center px-4 py-8 text-center">
      <Clock3 className={cn("h-5 w-5", content.iconClassName)} />
      <div className="mt-3 text-sm font-medium text-foreground">{content.title}</div>
      <p className="mt-1 max-w-md text-xs leading-5 text-muted-foreground">{content.description}</p>
    </div>
  );
}

function sourceStateContent(state: StationPublishedStatusSourceState | undefined, isNewApi: boolean) {
  const label = isNewApi ? "NewAPI 性能数据" : "官方渠道状态";
  if (state === "never_collected") {
    return {
      title: `尚未采集${label}`,
      description: isNewApi
        ? "此区段只读取 NewAPI 管理端性能接口；开始采集后会显示最近 60 个性能 bucket。"
        : "此区段只读取站点管理端发布的结果；开始采集后会显示最近 60 次官方记录。",
      iconClassName: "text-muted-foreground",
    };
  }
  if (state === "empty") {
    return {
      title: isNewApi ? "暂无 NewAPI 性能记录" : "站点未发布监控",
      description: isNewApi ? "该站点当前没有可展示的模型 × 分组性能记录。" : "该站点当前没有可展示的官方渠道监控记录。",
      iconClassName: "text-muted-foreground",
    };
  }
  if (state === "unsupported") {
    return {
      title: `当前站点不支持${label}`,
      description: isNewApi ? "该站点类型或版本没有可用的 NewAPI 性能接口。" : "该站点类型或版本没有可用的结构化官方状态接口。",
      iconClassName: "text-muted-foreground",
    };
  }
  if (state === "authorization_required") {
    return {
      title: "需要重新授权",
      description: `请完成站点窗口授权后重新采集${label}。`,
      iconClassName: "text-warning-foreground",
    };
  }
  if (state === "failed") {
    return {
      title: `${label}采集失败`,
      description: `本次失败不会清除之前保存的${isNewApi ? "性能数据" : "官方结果"}；可稍后重试。`,
      iconClassName: "text-danger-foreground",
    };
  }
  return {
    title: isNewApi ? "暂无 NewAPI 性能数据" : "暂无站点发布的渠道状态",
    description: isNewApi ? "重新采集后将显示 NewAPI 管理端的模型 × 分组性能结果。" : "重新采集后将显示站点管理端已发布的监控结果。",
    iconClassName: "text-muted-foreground",
  };
}

type NewApiGroupItem = {
  key: string;
  label: string;
  row: StationPublishedStatusRow;
};

type NewApiGroupSummary = {
  key: string;
  label: string;
  items: NewApiGroupItem[];
  availability: number | null;
  averageLatencyMs: number | null;
  averageTtftMs: number | null;
  averageTps: number | null;
  trend: StatusTrendCell[];
};

function NewApiGroupedStatusViews({ rows, view }: { rows: StationPublishedStatusRow[]; view: NewApiStatusView }) {
  const [expanded, setExpanded] = useState<Set<string>>(() => new Set());
  const summaries = view === "model" ? aggregateNewApiRows(rows, "model") : aggregateNewApiRows(rows, "group");

  return (
    <div className="overflow-x-auto">
      <table className="min-w-[980px] w-full table-fixed border-collapse text-left text-xs">
        <colgroup>
          <col />
          <col className="w-[180px]" />
          <col className="w-[105px]" />
          <col className="w-[120px]" />
          <col className="w-[340px]" />
        </colgroup>
        <thead className="border-b border-border bg-surface-subtle text-muted-foreground">
          <tr>
            <TableHead>{view === "model" ? "模型" : "分组"}</TableHead>
            <TableHead>状态概览</TableHead>
            <TableHead>最近可用性</TableHead>
            <TableHead>首响 / TPS</TableHead>
            <TableHead>最近 60 次</TableHead>
          </tr>
        </thead>
        <tbody>
          {summaries.map((summary) => (
            <NewApiSummaryRows
              key={summary.key}
              summary={summary}
              view={view}
              expanded={expanded.has(summary.key)}
              onToggle={() => setExpanded((current) => toggleExpandedKey(current, summary.key))}
            />
          ))}
        </tbody>
      </table>
    </div>
  );
}

function NewApiSummaryRows({
  summary,
  view,
  expanded,
  onToggle,
}: {
  summary: NewApiGroupSummary;
  view: NewApiStatusView;
  expanded: boolean;
  onToggle: () => void;
}) {
  return (
    <>
      <tr
        className={cn(
          "cursor-pointer border-b border-border transition-colors hover:bg-hover/70 focus:outline-none focus-visible:bg-hover/70",
          expanded && "bg-surface-subtle/55",
        )}
        tabIndex={0}
        aria-expanded={expanded}
        onClick={onToggle}
        onKeyDown={(event) => {
          if (event.key !== "Enter" && event.key !== " ") return;
          event.preventDefault();
          onToggle();
        }}
      >
        <TableCell>
          <div className="flex min-w-0 items-center gap-2">
            <ChevronDown className={cn("h-3.5 w-3.5 shrink-0 text-muted-foreground transition-transform", !expanded && "-rotate-90")} />
            <span className="min-w-0 truncate text-sm font-semibold text-foreground" title={summary.label}>{summary.label}</span>
            <span className="shrink-0 whitespace-nowrap text-xs font-normal text-muted-foreground">
              {summary.items.length} 个{view === "model" ? "分组" : "模型"}
            </span>
          </div>
        </TableCell>
        <TableCell><NewApiStatusOverview items={summary.items} /></TableCell>
        <NewApiMetricCells
          availability={summary.availability}
          firstResponseMs={summary.averageTtftMs}
          tps={summary.averageTps}
          trend={summary.trend}
          trendAriaLabel={`${summary.label} 最近 60 个性能 bucket`}
        />
      </tr>
      {expanded ? summary.items.map((item) => <NewApiChildRow key={item.key} item={item} />) : null}
    </>
  );
}

function NewApiChildRow({ item }: { item: NewApiGroupItem }) {
  const row = item.row;
  return (
    <tr className="border-b border-border bg-surface-subtle/30 transition-colors hover:bg-hover/70">
      <TableCell className="py-2">
        <div className="min-w-0 pl-6">
          <div className="truncate font-medium text-foreground" title={item.label}>{item.label}</div>
        </div>
      </TableCell>
      <TableCell className="py-2">
        <StatusBadge tone={badgeTone[row.currentOutcome]}>{newApiOutcomeLabel(row.currentOutcome)}</StatusBadge>
      </TableCell>
      <NewApiMetricCells
        availability={row.recentAvailabilityPercent}
        firstResponseMs={row.currentTtftMs ?? null}
        tps={row.currentTps ?? null}
        trend={newApiTrendCells(row)}
        trendAriaLabel={`${item.label} 最近 60 个性能 bucket`}
        compact
      />
    </tr>
  );
}

function NewApiMetricCells({
  availability,
  firstResponseMs,
  tps,
  trend,
  trendAriaLabel,
  compact = false,
}: {
  availability: number | null;
  firstResponseMs: number | null;
  tps: number | null;
  trend: StatusTrendCell[];
  trendAriaLabel: string;
  compact?: boolean;
}) {
  const cellClassName = compact ? "py-2" : undefined;
  return (
    <>
      <TableCell className={cellClassName}>
        <span className={cn("whitespace-nowrap font-medium tabular-nums", availability === null ? "text-muted-foreground" : "text-channel-availability")}>{formatAvailability(availability)}</span>
      </TableCell>
      <TableCell className={cellClassName}>
        <TwoLineMetric
          primary={formatResponseLatency(firstResponseMs)}
          secondary={`TPS ${formatNewApiTps(tps)}`}
          primaryTitle={formatRawMilliseconds(firstResponseMs)}
          secondaryTitle={formatRawNumber(tps)}
        />
      </TableCell>
      <TableCell className={cellClassName}>
        <StatusTrend cells={trend} slotCount={60} ariaLabel={trendAriaLabel} />
      </TableCell>
    </>
  );
}

function TwoLineMetric({
  primary,
  secondary,
  primaryTitle,
  secondaryTitle,
}: {
  primary: string;
  secondary: string;
  primaryTitle?: string;
  secondaryTitle?: string;
}) {
  return (
    <div className="min-w-0 leading-4">
      <div className="whitespace-nowrap font-medium font-mono tabular-nums text-foreground" title={primaryTitle}>{primary}</div>
      <div className="whitespace-nowrap text-[11px] text-muted-foreground" title={secondaryTitle}>{secondary}</div>
    </div>
  );
}

function NewApiStatusOverview({ items }: { items: NewApiGroupItem[] }) {
  const counts: Array<{ outcome: StationPublishedStatusOutcome; label: string; className: string }> = [
    { outcome: "available", label: "正常", className: "text-success-foreground" },
    { outcome: "degraded", label: "欠佳", className: "text-warning-foreground" },
    { outcome: "unavailable", label: "错误", className: "text-danger-foreground" },
    { outcome: "unknown", label: "未知", className: "text-muted-foreground" },
  ];
  const visible = counts
    .map((entry) => ({ ...entry, count: items.filter((item) => item.row.currentOutcome === entry.outcome).length }))
    .filter((entry) => entry.count > 0);
  return (
    <span
      className="flex min-w-0 items-center overflow-hidden whitespace-nowrap text-xs font-medium"
      title={visible.map((entry) => `${entry.count} ${entry.label}`).join(" · ")}
    >
      {visible.map((entry, index) => (
        <span key={entry.outcome} className="inline-flex items-center">
          {index > 0 ? <span className="text-muted-foreground/50"> · </span> : null}
          <span className={entry.className}>{entry.count} {entry.label}</span>
        </span>
      ))}
    </span>
  );
}

function toggleExpandedKey(current: Set<string>, key: string) {
  const next = new Set(current);
  if (next.has(key)) next.delete(key); else next.add(key);
  return next;
}

function aggregateNewApiRows(rows: StationPublishedStatusRow[], dimension: "model" | "group"): NewApiGroupSummary[] {
  const groups = new Map<string, NewApiGroupItem[]>();
  for (const row of rows) {
    const label = dimension === "model" ? row.primaryModel : (row.groupName ?? "未分组");
    const key = `${dimension}:${label}`;
    const items = groups.get(key) ?? [];
    items.push({ key: row.rowKey, label: dimension === "model" ? (row.groupName ?? "未分组") : row.primaryModel, row });
    groups.set(key, items);
  }
  return Array.from(groups, ([key, items]) => {
    // The NewAPI response exposes per-group aggregates but intentionally omits
    // request counts. Keep the parent summary explainable by using an
    // unweighted mean of the available child metrics; missing values stay
    // missing instead of being treated as zero.
    const rates: number[] = items
      .map((item) => item.row.recentAvailabilityPercent ?? item.row.currentSuccessRatePercent ?? null)
      .filter((value): value is number => value !== null && Number.isFinite(value));
    const availability = rates.length > 0 ? rates.reduce((sum, value) => sum + value, 0) / rates.length : null;
    return {
      key,
      label: dimension === "model" ? items[0].row.primaryModel : (items[0].row.groupName ?? "未分组"),
      items: items.sort((a, b) => a.label.localeCompare(b.label)),
      availability,
      averageLatencyMs: averageMetric(items.map((item) => item.row.currentLatencyMs)),
      averageTtftMs: averageMetric(items.map((item) => item.row.currentTtftMs ?? null)),
      averageTps: averageMetric(items.map((item) => item.row.currentTps ?? null)),
      trend: aggregateTrend(items, dimension === "model" ? "分组" : "模型"),
    };
  }).sort((a, b) => a.label.localeCompare(b.label));
}

function aggregateTone(outcomes: StationPublishedStatusOutcome[]): StationPublishedStatusOutcome {
  if (outcomes.length === 0) return "unknown";
  if (outcomes.some((outcome) => outcome === "unavailable")) return "unavailable";
  if (outcomes.some((outcome) => outcome === "degraded")) return "degraded";
  if (outcomes.every((outcome) => outcome === "unknown")) return "unknown";
  if (outcomes.some((outcome) => outcome === "unknown")) return "degraded";
  return "available";
}

function aggregateTrend(items: NewApiGroupItem[], childDimensionLabel: "分组" | "模型"): StatusTrendCell[] {
  const byTime = new Map<number, StationPublishedStatusOutcome[]>();
  for (const item of items) {
    for (const sample of item.row.recentSamples) {
      const outcomes = byTime.get(sample.checkedAtMs) ?? [];
      outcomes.push(sample.outcome);
      byTime.set(sample.checkedAtMs, outcomes);
    }
  }
  return Array.from(byTime.entries()).sort(([left], [right]) => left - right).map(([checkedAtMs, outcomes]) => {
    // A bucket missing from one child is an observed gap, not a healthy
    // result. Pad it as unknown so the aggregate trend reflects all children
    // participating in the parent row.
    while (outcomes.length < items.length) outcomes.push("unknown");
    const tone = aggregateTone(outcomes);
    const available = outcomes.filter((outcome) => outcome === "available").length;
    const degraded = outcomes.filter((outcome) => outcome === "degraded").length;
    const unavailable = outcomes.filter((outcome) => outcome === "unavailable").length;
    const unknown = outcomes.filter((outcome) => outcome === "unknown").length;
    return {
      id: `aggregate-${checkedAtMs}`,
      tone: tone === "unknown" ? "missing" : tone,
      label: `${formatTime(checkedAtMs)}\n${outcomes.length} 个${childDimensionLabel}\n正常 ${available}\n欠佳 ${degraded}\n错误 ${unavailable}\n未知 ${unknown}`,
      modelLabel: "聚合状态",
      timeLabel: formatTime(checkedAtMs),
      availabilityLabel: `正常 ${available} · 欠佳 ${degraded} · 错误 ${unavailable} · 未知 ${unknown}`,
      latencyLabel: "--",
      metricLabel: `${childDimensionLabel} ${outcomes.length} · 正常 ${available} · 欠佳 ${degraded} · 错误 ${unavailable} · 未知 ${unknown}`,
    };
  });
}

function averageMetric(values: Array<number | null>) {
  const present = values.filter((value): value is number => value !== null && Number.isFinite(value));
  return present.length === 0 ? null : present.reduce((sum, value) => sum + value, 0) / present.length;
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
            <TableHead>首响 / Ping</TableHead>
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
                  <TwoLineMetric
                    primary={formatResponseLatency(row.currentLatencyMs)}
                    secondary={`Ping ${formatResponseLatency(row.currentPingLatencyMs)}`}
                    primaryTitle={formatRawMilliseconds(row.currentLatencyMs)}
                    secondaryTitle={formatRawMilliseconds(row.currentPingLatencyMs)}
                  />
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
  if (outcome === "degraded") return "欠佳";
  if (outcome === "unavailable") return "错误";
  return "未知";
}

function newApiOutcomeLabel(outcome: StationPublishedStatusOutcome) {
  return outcomeLabel(outcome);
}

function publishedStatusTrendCells(row: StationPublishedStatusRow): StatusTrendCell[] {
  return row.recentSamples.map((sample) => ({
    id: sample.id,
    tone: sample.outcome === "unknown" ? "missing" : sample.outcome,
    label: `来源：站点发布\n模型：${sample.model}\n检查时间：${formatTime(sample.checkedAtMs)}\n状态：${outcomeLabel(sample.outcome)}\n首响：${formatResponseLatency(sample.latencyMs)}\nPing：${formatResponseLatency(sample.pingLatencyMs)}`,
    modelLabel: sample.model,
    timeLabel: `官方检查：${formatTime(sample.checkedAtMs)}`,
    availabilityLabel: `状态：${outcomeLabel(sample.outcome)}`,
    latencyLabel: formatResponseLatency(sample.latencyMs),
    metricLabel: `首响：${formatResponseLatency(sample.latencyMs)} · Ping：${formatResponseLatency(sample.pingLatencyMs)}`,
  }));
}

function newApiTrendCells(row: StationPublishedStatusRow): StatusTrendCell[] {
  return row.recentSamples.map((sample) => ({
    id: sample.id,
    tone: sample.outcome === "unknown" ? "missing" : sample.outcome,
    label: `来源：NewAPI 管理端性能\n模型：${sample.model}\n检查时间：${formatTime(sample.checkedAtMs)}\n状态：${newApiOutcomeLabel(sample.outcome)}\n成功率：${formatAvailability(sample.successRatePercent ?? null)}\n总响应：${formatResponseLatency(sample.latencyMs)}\n首响：${formatResponseLatency(sample.ttftMs ?? null)}\nTPS：${formatNewApiTps(sample.tps ?? null)}`,
    modelLabel: sample.model,
    timeLabel: `性能 bucket：${formatTime(sample.checkedAtMs)}`,
    availabilityLabel: `状态：${newApiOutcomeLabel(sample.outcome)} · 成功率：${formatAvailability(sample.successRatePercent ?? null)}`,
    latencyLabel: formatResponseLatency(sample.ttftMs ?? null),
    metricLabel: `首响：${formatResponseLatency(sample.ttftMs ?? null)} · TPS：${formatNewApiTps(sample.tps ?? null)} · 总响应 ${formatResponseLatency(sample.latencyMs)}`,
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
  return value === null || !Number.isFinite(value) ? "—" : `${value.toFixed(2)}%`;
}

function formatResponseLatency(value: number | null | undefined) {
  if (value == null || !Number.isFinite(value)) return "—";
  if (Math.abs(value) < 1_000) return `${Math.round(value)} ms`;
  return `${(value / 1_000).toFixed(1)} s`;
}

function formatNewApiTps(value: number | null) {
  if (value === null || !Number.isFinite(value)) return "—";
  return trimTrailingZeroes(value.toFixed(Math.abs(value) < 10 ? 2 : 1));
}

function formatRawMilliseconds(value: number | null | undefined) {
  return value == null || !Number.isFinite(value) ? "无数据" : `${value} ms`;
}

function formatRawNumber(value: number | null | undefined) {
  return value == null || !Number.isFinite(value) ? "无数据" : String(value);
}

function trimTrailingZeroes(value: string) {
  return String(Number(value));
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
