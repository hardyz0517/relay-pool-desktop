import { useMemo, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { CheckCheck, RefreshCw, Search, Trash2 } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import {
  usePageRefreshEnabled,
} from "@/components/shell/PageActivity";
import {
  Button,
  ConfirmDialog,
  EmptyState,
  Pagination,
  SegmentedControl,
  SelectControl,
  StatusBadge,
  Toolbar,
  useToast,
} from "@/components/ui";
import { readError } from "@/lib/errors";
import {
  clearChangeEvents,
  markChangeEventsRead,
  notifyChangeEventsUpdated,
} from "@/lib/api/changeEvents";
import { useActivityQuery } from "@/lib/query/useActivityQuery";
import {
  changeEventsQueryOptions,
  stationsQueryOptions,
} from "@/lib/query/resourceQueries";
import { queryKeys } from "@/lib/query/queryKeys";
import type { ChangeEvent } from "@/lib/types/changeEvents";
import {
  activeSeverityCount,
  buildChangeEventListItem,
  filterChangeEvents,
  formatChangeTime,
  mergeChangeEventUpdates,
  markUnreadChangeEventsRead,
  objectTypeLabels,
  paginateChangeEvents,
  severityTone,
  unreadRiskCount,
  type ChangeFilter,
} from "./changeEventViewModels";

export function ChangeCenterPage() {
  const toast = useToast();
  const queryClient = useQueryClient();
  const refreshEnabled = usePageRefreshEnabled();
  const eventsQuery = useActivityQuery(refreshEnabled, changeEventsQueryOptions(false));
  const stationsQuery = useActivityQuery(refreshEnabled, stationsQueryOptions());
  const events = eventsQuery.data ?? [];
  const stationNamesById = useMemo(
    () => new Map((stationsQuery.data ?? []).map((station) => [station.id, station.name] as const)),
    [stationsQuery.data],
  );
  const stationCreditPerCnyById = useMemo(
    () => new Map((stationsQuery.data ?? []).map((station) => [station.id, station.creditPerCny] as const)),
    [stationsQuery.data],
  );
  const loading = eventsQuery.isPending && eventsQuery.data === undefined;
  const error = eventsQuery.error ? readError(eventsQuery.error) : null;
  const [filter, setFilter] = useState<ChangeFilter>({ severity: "all", status: "active", objectType: "all", query: "" });
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(CHANGE_EVENTS_DEFAULT_PAGE_SIZE);
  const [clearConfirmOpen, setClearConfirmOpen] = useState(false);
  const [saving, setSaving] = useState(false);

  async function refresh(showSuccess = false) {
    try {
      await Promise.all([
        queryClient.refetchQueries({ queryKey: queryKeys.stations, type: "active" }),
        queryClient.refetchQueries({ queryKey: queryKeys.changeEvents, type: "active" }),
      ]);
      if (showSuccess) {
        toast.success("变更中心已刷新");
      }
    } catch (requestError) {
      const message = readError(requestError);
      toast.error("刷新变更中心失败", message);
    }
  }

  async function markAllRead() {
    setSaving(true);
    try {
      await queryClient.cancelQueries({ queryKey: queryKeys.changeEvents });
      const result = await markUnreadChangeEventsRead(events, markChangeEventsRead);
      queryClient.setQueryData(queryKeys.changeEvents, (latestEvents: typeof events | undefined) =>
        mergeChangeEventUpdates(latestEvents ?? events, result.updatedEvents),
      );
      if (result.changedCount > 0) {
        notifyChangeEventsUpdated();
      }
      toast.success(`已标记 ${result.changedCount} 条变更为已读`);
    } catch (requestError) {
      toast.error("批量标记已读失败", readError(requestError));
    } finally {
      setSaving(false);
    }
  }

  async function clearChangeHistory() {
    setSaving(true);
    try {
      await queryClient.cancelQueries({ queryKey: queryKeys.changeEvents });
      await clearChangeEvents();
      queryClient.setQueryData(queryKeys.changeEvents, []);
      setPage(1);
      notifyChangeEventsUpdated();
      toast.success("变更记录已清除");
      setClearConfirmOpen(false);
    } catch (requestError) {
      toast.error("清除变更记录失败", readError(requestError));
    } finally {
      setSaving(false);
    }
  }

  const filteredEvents = useMemo(
    () => filterChangeEvents(events, filter, { stationNamesById, stationCreditPerCnyById }),
    [events, filter, stationCreditPerCnyById, stationNamesById],
  );
  const pageInfo = useMemo(() => paginateChangeEvents(filteredEvents, page, pageSize), [filteredEvents, page, pageSize]);
  const unreadCount = events.filter((event) => event.status === "unread").length;
  const riskCount = unreadRiskCount(events);
  const objectOptions = useMemo(() => {
    const values = Array.from(new Set(events.map((event) => event.objectType))).sort((a, b) => a.localeCompare(b));
    return values.map((value) => ({ value, label: objectTypeLabels[value] ?? value }));
  }, [events]);

  return (
    <PageScaffold
      title="变更中心"
      actions={
        <div className="flex items-center gap-2">
          <Button variant="danger" onClick={() => setClearConfirmOpen(true)} disabled={loading || saving || events.length === 0}>
            <Trash2 className="h-4 w-4" />
            清除记录
          </Button>
          <Button variant="secondary" onClick={() => void markAllRead()} disabled={loading || saving || unreadCount === 0}>
            <CheckCheck className="h-4 w-4" />
            一键已读
          </Button>
          <Button variant="secondary" onClick={() => void refresh(true)} disabled={loading || saving}>
            <RefreshCw className="h-4 w-4" />
            刷新
          </Button>
        </div>
      }
    >
      <div className="grid gap-[var(--shell-page-gap)]">
        <div className="grid gap-3 md:grid-cols-4">
          <SummaryTile label="未读风险" value={riskCount} tone={riskCount > 0 ? "text-danger-foreground" : "text-success-foreground"} />
          <SummaryTile label="严重" value={activeSeverityCount(events, "critical")} />
          <SummaryTile label="警告" value={activeSeverityCount(events, "warning")} />
          <SummaryTile label="信息" value={activeSeverityCount(events, "info")} />
        </div>

        <div className="min-w-0">
          <div
            data-testid="change-center-toolbar-surface"
            className="overflow-hidden rounded-[var(--surface-radius)] border border-border bg-surface shadow-[var(--surface-shadow)]"
          >
            <Toolbar>
              <div className="flex min-w-0 flex-wrap items-center gap-2">
                <SegmentedControl
                  value={filter.status}
                  options={[
                    { value: "active", label: "活跃" },
                    { value: "unread", label: "未读" },
                    { value: "resolved", label: "已解决" },
                    { value: "all", label: "全部" },
                  ]}
                  onChange={(status) => {
                    setPage(1);
                    setFilter((current) => ({ ...current, status }));
                  }}
                />
                <SelectControl
                  ariaLabel="变更级别"
                  className={inputClassName}
                  value={filter.severity}
                  options={[
                    { value: "all", label: "全部级别" },
                    { value: "critical", label: "严重" },
                    { value: "warning", label: "警告" },
                    { value: "info", label: "信息" },
                  ]}
                  onChange={(severity) => {
                    setPage(1);
                    setFilter((current) => ({ ...current, severity }));
                  }}
                />
                <SelectControl
                  ariaLabel="对象类型"
                  className={inputClassName}
                  value={filter.objectType}
                  options={[
                    { value: "all", label: "全部对象" },
                    ...objectOptions,
                  ]}
                  onChange={(objectType) => {
                    setPage(1);
                    setFilter((current) => ({ ...current, objectType }));
                  }}
                />
                <div className="relative">
                  <Search className="pointer-events-none absolute left-2.5 top-2 h-4 w-4 text-muted-foreground" />
                  <input
                    className={`${inputClassName} pl-8`}
                    value={filter.query}
                    placeholder="搜索变更 / 对象 / 来源"
                    onChange={(event) => {
                      setPage(1);
                      setFilter((current) => ({ ...current, query: event.target.value }));
                    }}
                  />
                </div>
              </div>
            </Toolbar>
          </div>
          <div
            data-testid="change-center-list-surface"
            className="mt-3 min-w-0 overflow-hidden rounded-[var(--surface-radius)] border border-border bg-surface shadow-[var(--surface-shadow)]"
          >
            {error && <div className="border-b border-danger-border bg-danger-surface px-3 py-2 text-sm text-danger-foreground">{error}</div>}
            {filteredEvents.length === 0 ? (
              <EmptyState
                title={loading ? "正在读取变更" : "暂无变更"}
                description="余额、密钥、采集、价格、倍率、模型和路由状态变化会在这里形成记录。"
              />
            ) : (
              <>
                <div className="divide-y divide-border bg-surface">
                  {pageInfo.events.map((event) => (
                    <ChangeEventRow
                      key={event.id}
                      event={event}
                      stationNamesById={stationNamesById}
                      stationCreditPerCnyById={stationCreditPerCnyById}
                      deferStationIdentifierFallback={stationsQuery.isPending && stationsQuery.data === undefined}
                    />
                  ))}
                </div>
              </>
            )}
          </div>
          {filteredEvents.length > 0 && (
            <div
              data-testid="change-center-pagination-surface"
              className="mt-4 flex min-h-12 flex-wrap items-center justify-between gap-3 border border-border bg-surface px-3 py-2 text-xs text-muted-foreground"
            >
              <div className="flex flex-wrap items-center gap-3">
                <span>
                  第 {pageInfo.startIndex}-{pageInfo.endIndex} 条 / 共 {pageInfo.totalCount} 条
                </span>
                <label className="flex items-center gap-2">
                  <span>每页</span>
                  <select
                    aria-label="每页记录数"
                    value={pageSize}
                    onChange={(event) => {
                      setPageSize(Number(event.target.value));
                      setPage(1);
                    }}
                    className="h-8 rounded-[4px] border border-border bg-surface px-2 text-sm text-foreground outline-none transition-colors focus:border-ring focus:ring-2 focus:ring-ring/20"
                  >
                    {[20, 50, 100].map((size) => (
                      <option key={size} value={size}>{size}</option>
                    ))}
                  </select>
                </label>
              </div>

              <Pagination
                ariaLabel="变更中心分页"
                page={pageInfo.page}
                totalPages={pageInfo.totalPages}
                disabled={loading || saving}
                onPageChange={setPage}
              />
            </div>
          )}
        </div>
        <ConfirmDialog
          open={clearConfirmOpen}
          title="清除变更记录"
          description="确定要清除全部变更记录吗？此操作不会删除中转站、密钥或价格配置，但记录本身无法恢复。"
          confirmLabel="清除"
          confirming={saving}
          onCancel={() => setClearConfirmOpen(false)}
          onConfirm={() => void clearChangeHistory()}
        />
      </div>
    </PageScaffold>
  );
}

function ChangeEventRow({
  event,
  stationNamesById,
  stationCreditPerCnyById,
  deferStationIdentifierFallback,
}: {
  event: ChangeEvent;
  stationNamesById: Map<string, string>;
  stationCreditPerCnyById: Map<string, number>;
  deferStationIdentifierFallback: boolean;
}) {
  const item = buildChangeEventListItem(event, {
    stationNamesById,
    stationCreditPerCnyById,
    deferStationIdentifierFallback,
  });
  return (
    <div className="grid min-h-[48px] w-full grid-cols-[56px_minmax(0,1fr)_88px] items-center gap-3 bg-surface px-3 py-2 text-left">
      <div className="flex flex-col items-start gap-1">
        <StatusBadge tone={severityTone[event.severity]}>{item.severityLabel}</StatusBadge>
      </div>
      <div className="min-w-0">
        <div className="truncate text-[13px] font-semibold text-foreground">{item.title}</div>
      </div>
      <div className="flex flex-col items-end text-xs text-muted-foreground">
        <span className="font-medium text-foreground">{formatChangeTime(event.detectedAt)}</span>
      </div>
    </div>
  );
}

function SummaryTile({ label, value, tone = "text-foreground" }: { label: string; value: number; tone?: string }) {
  return (
    <div className="rounded-[var(--surface-radius)] border border-border bg-surface p-3 shadow-[var(--surface-shadow)]">
      <div className="text-xs text-muted-foreground">{label}</div>
      <div className={`mt-1 text-2xl font-semibold ${tone}`}>{value}</div>
    </div>
  );
}


const inputClassName =
  "h-8 rounded-[12px] border border-info-border bg-info-surface px-3 text-sm text-foreground outline-none transition focus:border-ring focus:bg-surface focus:ring-2 focus:ring-ring/20";

const CHANGE_EVENTS_DEFAULT_PAGE_SIZE = 20;
