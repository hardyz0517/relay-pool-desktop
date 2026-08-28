import {
  AlertTriangle,
  CircleX,
  Clock3,
  Gauge,
  KeyRound,
  LayoutGrid,
  RefreshCw,
  Table2,
  Timer,
  type LucideIcon,
} from "lucide-react";
import { useState, type CSSProperties, type ReactNode } from "react";
import { Sub2ApiPlatformIcon } from "@/components/group/Sub2ApiPlatformIcon";
import { StatusTrend } from "@/components/status/StatusTrend";
import {
  Button,
  EmptyState,
  Pagination,
  SegmentedControl,
  SelectControl,
  StatusBadge,
  type StatusTone,
} from "@/components/ui";
import { readError } from "@/lib/errors";
import { groupVisualMetaFor } from "@/lib/groupVisualMeta";
import { groupVisualClassNames } from "@/lib/groupVisualStyles";
import { stationsQueryOptions } from "@/lib/query/resourceQueries";
import { useActivityQuery } from "@/lib/query/useActivityQuery";
import { cn } from "@/lib/utils";
import { availabilityHue } from "./channelStatusViewModel";
import type { OfficialStatusRowView } from "./officialStatusViewModel";
import {
  OFFICIAL_STATUS_PAGE_SIZE_OPTIONS,
  useOfficialStatusController,
} from "./useOfficialStatusController";

type OfficialStatusViewMode = "table" | "cards";

const outcomeBadgeTone: Record<OfficialStatusRowView["currentOutcome"], StatusTone> = {
  available: "healthy",
  degraded: "warning",
  unavailable: "error",
  unknown: "disabled",
};

const outcomeDotClassName: Record<OfficialStatusRowView["currentOutcome"], string> = {
  available: "bg-channel-health-bar",
  degraded: "bg-channel-health-degraded-bar",
  unavailable: "bg-channel-health-danger-bar",
  unknown: "bg-channel-health-empty-bar",
};

export function OfficialStatusTab() {
  const controller = useOfficialStatusController();
  const stationsQuery = useActivityQuery(stationsQueryOptions());
  const [viewMode, setViewMode] = useState<OfficialStatusViewMode>("table");
  const error = controller.query.error ? readError(controller.query.error) : null;

  return (
    <div className="space-y-3">
      <div data-tour="channels-official-summary">
        <OfficialStatusToolbar
          controller={controller}
          stations={stationsQuery.data ?? []}
          stationCatalogFailed={stationsQuery.isError}
          viewMode={viewMode}
          onViewModeChange={setViewMode}
        />
      </div>

      {stationsQuery.isError ? (
        <div className="text-xs text-danger-foreground" role="status">
          站点目录读取失败，站点筛选暂不可用。
        </div>
      ) : null}

      {error ? (
        <div className="flex items-start gap-2 rounded-[var(--surface-radius)] border border-danger-border bg-danger-surface px-3 py-2 text-sm text-danger-foreground">
          <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
          <div>
            <div className="font-medium">官方状态读取失败</div>
            <div className="text-xs">{error}</div>
          </div>
        </div>
      ) : null}

      <div data-tour="channels-official-results">
        {controller.query.isPending ? (
          <div
            className="h-48 animate-pulse rounded-[var(--surface-radius)] border border-border bg-surface-subtle"
            aria-label="加载官方状态"
          />
        ) : viewMode === "cards" ? (
          <OfficialStatusCardGrid rows={controller.view.rows} />
        ) : (
          <OfficialStatusTable rows={controller.view.rows} />
        )}
      </div>

      {controller.pageInfo.totalPages > 1 || controller.pageInfo.currentPage > 1 ? (
        <div
          data-testid="official-status-pagination-surface"
          className="flex min-h-12 flex-wrap items-center justify-between gap-3 border border-border bg-surface px-3 py-2 text-xs text-muted-foreground"
        >
          <div className="flex flex-wrap items-center gap-3">
            <span>
              第 {controller.pageInfo.currentPage} 页：{controller.pageInfo.startIndex}-{controller.pageInfo.endIndex}
              {controller.pageInfo.total > 0 ? ` / 共 ${controller.pageInfo.total} 条` : ""}
            </span>
            <label className="flex items-center gap-2">
              <span>每页数量</span>
              <select
                aria-label="每页数量"
                value={controller.pageSize}
                onChange={(event) => controller.setPageSize(Number(event.target.value))}
                className="h-8 rounded-[4px] border border-border bg-surface px-2 text-sm text-foreground outline-none focus:border-ring"
              >
                {OFFICIAL_STATUS_PAGE_SIZE_OPTIONS.map((size) => (
                  <option key={size} value={size}>{size}</option>
                ))}
              </select>
            </label>
          </div>
          <Pagination
            ariaLabel="官方状态分页"
            page={controller.pageInfo.currentPage}
            totalPages={controller.pageInfo.totalPages}
            disabled={controller.paginationBusy}
            onPageChange={(page) => void controller.changePage(page)}
          />
        </div>
      ) : null}
    </div>
  );
}

function OfficialStatusToolbar({
  controller,
  stations,
  stationCatalogFailed,
  viewMode,
  onViewModeChange,
}: {
  controller: ReturnType<typeof useOfficialStatusController>;
  stations: Array<{ id: string; name: string }>;
  stationCatalogFailed: boolean;
  viewMode: OfficialStatusViewMode;
  onViewModeChange: (value: OfficialStatusViewMode) => void;
}) {
  return (
    <div className="rounded-[var(--surface-radius)] border border-border bg-surface p-3 shadow-[var(--surface-shadow)]">
      <div className="flex flex-wrap items-center gap-2">
        <SegmentedControl
          ariaLabel="官方状态视图"
          value={viewMode}
          options={[
            { value: "table", label: "表格", icon: Table2 },
            { value: "cards", label: "卡片", icon: LayoutGrid },
          ]}
          onChange={onViewModeChange}
        />
        <input
          aria-label="搜索官方状态"
          value={controller.filters.search}
          onChange={(event) => controller.setSearch(event.target.value)}
          placeholder="搜索站点 / 监控 / Provider / 分组 / 模型"
          className="h-8 min-w-[220px] flex-1 rounded-[var(--surface-radius)] border border-border bg-surface px-3 text-sm outline-none transition focus:border-ring/40 focus:ring-2 focus:ring-ring/20"
        />
        <SelectControl
          ariaLabel="站点"
          value={controller.filters.stationId}
          options={[
            { value: "", label: "全部站点" },
            ...stations.map((station) => ({ value: station.id, label: station.name })),
          ]}
          onChange={controller.setStationId}
          disabled={stationCatalogFailed}
          searchable
          searchPlaceholder="搜索站点"
          emptyLabel="无匹配站点"
          className="min-w-[120px]"
        />
        <SelectControl
          ariaLabel="监控状态"
          value={controller.filters.outcome}
          options={[
            { value: "all", label: "全部监控状态" },
            { value: "available", label: "可用" },
            { value: "degraded", label: "降级" },
            { value: "unavailable", label: "错误" },
            { value: "unknown", label: "未知" },
          ]}
          onChange={controller.setOutcome}
          className="min-w-[120px]"
        />
        <SelectControl
          ariaLabel="数据采集状态"
          value={controller.filters.sourceState}
          options={[
            { value: "all", label: "全部数据采集" },
            { value: "available", label: "采集正常" },
            { value: "degraded", label: "部分解析" },
            { value: "failed", label: "采集失败" },
            { value: "authorization_required", label: "采集需授权" },
          ]}
          onChange={controller.setSourceState}
          className="min-w-[132px]"
        />
        <Button
          variant="secondary"
          disabled={controller.query.isFetching}
          onClick={() => void controller.refresh()}
        >
          <RefreshCw className={cn("h-4 w-4", controller.query.isFetching && "animate-spin")} />
          刷新
        </Button>
      </div>
    </div>
  );
}

function OfficialStatusTable({ rows }: { rows: OfficialStatusRowView[] }) {
  if (rows.length === 0) {
    return <EmptyState title="暂无官方状态" description="尚未采集到符合筛选条件的官方渠道状态。" />;
  }

  return (
    <div className="overflow-hidden rounded-[var(--surface-radius)] border border-border bg-surface shadow-[var(--surface-shadow)]">
      <div className="overflow-x-auto">
        <table className="min-w-[1040px] w-full table-fixed border-collapse bg-surface text-left text-sm">
          <colgroup>
            <col className="w-[20%]" />
            <col className="w-[10%]" />
            <col className="w-[10%]" />
            <col className="w-[10%]" />
            <col className="w-[12%]" />
            <col className="w-[38%]" />
          </colgroup>
          <thead className="border-b border-border bg-surface text-xs font-medium text-muted-foreground">
            <tr>
              <HeaderCell>监控 / 站点</HeaderCell>
              <HeaderCell>模型</HeaderCell>
              <HeaderCell>当前状态</HeaderCell>
              <HeaderCell>可用性</HeaderCell>
              <HeaderCell>最近检查</HeaderCell>
              <HeaderCell>趋势</HeaderCell>
            </tr>
          </thead>
          <tbody>
            {rows.map((row) => {
              const visualMeta = officialMonitorVisualMeta(row);
              const platformClassNames = groupVisualClassNames[visualMeta.platform];
              return (
                <tr key={row.rowKey} className="border-t border-border hover:bg-hover/70">
                  <BodyCell>
                    <div className="flex min-w-0 items-center gap-2.5">
                      <span
                        className={cn(
                          "flex h-8 w-8 shrink-0 items-center justify-center rounded-[8px]",
                          platformClassNames.rateBadge,
                        )}
                        title={`${visualMeta.label}${row.groupName ? ` · ${row.groupName}` : ""}`}
                      >
                        <Sub2ApiPlatformIcon platform={visualMeta.platform} className={cn("h-4 w-4", platformClassNames.icon)} />
                      </span>
                      <div className="min-w-0">
                        <div className="truncate font-medium text-foreground" title={row.name}>{row.name}</div>
                        <div className="flex min-w-0 items-center gap-1.5 text-xs text-muted-foreground">
                          <span className="truncate" title={`${row.stationName} · ${row.provider}`}>
                            {row.stationName} · {row.provider}
                          </span>
                          <CollectionIssueIndicator row={row} />
                        </div>
                      </div>
                    </div>
                  </BodyCell>
                  <BodyCell>
                    <div className="truncate text-foreground" title={modelTitle(row)}>{modelLabel(row)}</div>
                  </BodyCell>
                  <BodyCell>
                    <StatusBadge tone={outcomeBadgeTone[row.currentOutcome]}>{row.currentLabel}</StatusBadge>
                  </BodyCell>
                  <BodyCell>
                    <AvailabilityValue value={row.recentAvailabilityPercent} label={row.availabilityLabel} />
                  </BodyCell>
                  <BodyCell>
                    <div
                      className="flex items-center gap-2"
                      title={`官方状态：${row.currentLabel}\n模型延迟：${formatLatency(row.currentLatencyMs)}\n端点 Ping：${formatLatency(row.currentPingLatencyMs)}`}
                    >
                      <span
                        role="img"
                        aria-label={`官方状态：${row.currentLabel}`}
                        className={cn("h-2 w-2 shrink-0 rounded-full", outcomeDotClassName[row.currentOutcome])}
                      />
                      <div>
                        <div className="font-medium text-foreground">{formatLatency(row.currentLatencyMs)}</div>
                        <div className="whitespace-nowrap text-[11px] text-muted-foreground">{row.lastCheckedLabel}</div>
                      </div>
                    </div>
                  </BodyCell>
                  <BodyCell className="pr-5">
                    <StatusTrend cells={row.trend} slotCount={60} ariaLabel={`${row.name} 的站点发布最近 60 次状态记录`} />
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

function OfficialStatusCardGrid({ rows }: { rows: OfficialStatusRowView[] }) {
  if (rows.length === 0) {
    return <EmptyState title="暂无官方状态" description="尚未采集到符合筛选条件的官方渠道状态。" />;
  }

  return (
    <div className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3 2xl:grid-cols-4">
      {rows.map((row) => <OfficialStatusCard key={row.rowKey} row={row} />)}
    </div>
  );
}

function OfficialStatusCard({ row }: { row: OfficialStatusRowView }) {
  const visualMeta = officialMonitorVisualMeta(row);
  const platformClassNames = groupVisualClassNames[visualMeta.platform];

  return (
    <article className="flex h-full flex-col rounded-[var(--surface-radius)] border border-border bg-surface p-3.5 shadow-[var(--surface-shadow)]">
      <div className="flex items-start justify-between gap-3">
        <div className="flex min-w-0 items-start gap-2.5">
          <span
            className={cn(
              "flex h-9 w-9 shrink-0 items-center justify-center rounded-[8px]",
              platformClassNames.rateBadge,
            )}
            title={`${visualMeta.label}${row.groupName ? ` · ${row.groupName}` : ""}`}
          >
            <Sub2ApiPlatformIcon platform={visualMeta.platform} className={cn("h-4 w-4", platformClassNames.icon)} />
          </span>
          <div className="min-w-0">
            <div className="truncate text-[15px] font-semibold leading-5 text-foreground" title={row.name}>{row.name}</div>
            <div className="mt-1 flex min-w-0 items-center gap-1.5 text-xs text-muted-foreground">
              <span className="truncate" title={`${row.stationName} · ${modelLabel(row)}`}>
                {row.stationName} · {modelLabel(row)}
              </span>
              <CollectionIssueIndicator row={row} />
            </div>
          </div>
        </div>
        <StatusBadge
          tone={outcomeBadgeTone[row.currentOutcome]}
          className={cn(
            "shrink-0 border-0 px-2.5",
            row.currentOutcome === "available" && "bg-channel-health-surface text-channel-health-label",
          )}
        >
          {row.currentLabel}
        </StatusBadge>
      </div>

      <div className="mt-3 grid grid-cols-2 gap-2">
        <MetricTile icon={<Timer className="h-3.5 w-3.5" />} label="模型延迟" value={formatLatency(row.currentLatencyMs)} />
        <MetricTile icon={<Gauge className="h-3.5 w-3.5" />} label="端点 Ping" value={formatLatency(row.currentPingLatencyMs)} />
      </div>

      <div className="mt-3 border-t border-border pt-3">
        <div className="flex items-end justify-between gap-3">
          <div className="min-w-0 pb-0.5 text-xs font-medium text-muted-foreground">可用性</div>
          <AvailabilityValue value={row.recentAvailabilityPercent} label={row.availabilityLabel} large />
        </div>
      </div>

      <div className="mt-2.5 border-t border-border pt-2.5">
        <div className="mb-1.5 flex items-center justify-between gap-2 text-[11px] text-muted-foreground/70">
          <span>近 60 次记录</span>
          <span className="truncate" title={row.lastCheckedLabel}>最后检查 {row.lastCheckedLabel}</span>
        </div>
        <StatusTrend cells={row.trend} compact variant="bars" slotCount={60} />
        <div className="mt-1 flex justify-between text-[10px] leading-3 text-muted-foreground/70">
          <span>过去</span>
          <span>现在</span>
        </div>
      </div>
    </article>
  );
}

function HeaderCell({ children }: { children: string }) {
  return <th className="h-8 whitespace-nowrap px-3">{children}</th>;
}

function BodyCell({ children, className }: { children: ReactNode; className?: string }) {
  return <td className={cn("border-b border-border px-3 py-2.5 align-middle", className)}>{children}</td>;
}

function CollectionIssueIndicator({ row }: { row: OfficialStatusRowView }) {
  const issue = collectionIssueMeta(row);
  if (!issue) return null;

  const Icon = issue.icon;
  const collectedAt = formatCollectionTime(row.lastAttemptAtMs);
  const label = collectedAt ? `${issue.label} · 最后采集 ${collectedAt}` : issue.label;

  return (
    <span
      role="img"
      aria-label={issue.label}
      title={label}
      className={cn("inline-flex shrink-0", issue.className)}
    >
      <Icon className="h-3.5 w-3.5" />
    </span>
  );
}

function collectionIssueMeta(row: OfficialStatusRowView): {
  icon: LucideIcon;
  label: string;
  className: string;
} | null {
  if (row.sourceState === "authorization_required") {
    return { icon: KeyRound, label: "数据采集：需要授权", className: "text-warning-foreground" };
  }
  if (row.sourceState === "failed") {
    return { icon: CircleX, label: "数据采集：失败，正在展示上次有效结果", className: "text-danger-foreground" };
  }
  if (row.sourceState === "degraded") {
    return { icon: AlertTriangle, label: "数据采集：部分解析，已保留有效结果", className: "text-warning-foreground" };
  }
  if (row.stale) {
    return { icon: Clock3, label: "数据采集：结果已过期", className: "text-warning-foreground" };
  }
  if (row.sourceState !== "available") {
    return { icon: AlertTriangle, label: `数据采集：${row.sourceStateLabel}`, className: "text-muted-foreground" };
  }
  return null;
}

function formatCollectionTime(value: number | null) {
  if (value === null || !Number.isFinite(value)) return null;
  return new Date(value).toLocaleString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function MetricTile({ icon, label, value }: { icon: ReactNode; label: string; value: string }) {
  return (
    <div className="min-w-0 rounded-[8px] border border-border bg-surface-subtle px-3 py-2.5">
      <div className="flex items-center gap-1.5 text-[11px] font-medium text-muted-foreground/70">
        {icon}
        <span className="truncate">{label}</span>
      </div>
      <div className="mt-2 truncate text-[18px] font-semibold leading-6 text-foreground" title={value}>{value}</div>
    </div>
  );
}

function AvailabilityValue({ value, label, large = false }: { value: number | null; label: string; large?: boolean }) {
  const hue = availabilityHue(value);
  return (
    <div
      className={cn(
        "font-semibold",
        large && "shrink-0 text-3xl leading-8 tracking-normal",
        hue === null ? "text-muted-foreground" : "text-channel-availability",
      )}
      style={hue === null ? undefined : ({ "--channel-availability-hue": hue } as CSSProperties)}
    >
      {label}
    </div>
  );
}

function officialMonitorVisualMeta(row: OfficialStatusRowView) {
  return groupVisualMetaFor(
    [row.groupName, row.primaryModel, row.provider].filter(Boolean).join(" "),
    { provider: row.provider },
  );
}

function modelLabel(row: OfficialStatusRowView) {
  return row.extraModels.length > 0 ? `${row.primaryModel} +${row.extraModels.length}` : row.primaryModel;
}

function modelTitle(row: OfficialStatusRowView) {
  return row.extraModels.length > 0 ? [row.primaryModel, ...row.extraModels].join("\n") : row.primaryModel;
}

function formatLatency(value: number | null) {
  return value === null || !Number.isFinite(value) ? "--" : `${value} ms`;
}
