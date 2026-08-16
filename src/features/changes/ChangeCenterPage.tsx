import { useMemo, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { Check, CheckCheck, ChevronDown, ChevronUp, MoreHorizontal, RefreshCw, Route, Search, Settings } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import {
  Button,
  ConfirmDialog,
  EmptyState,
  IconButton,
  Pagination,
  SegmentedControl,
  SelectControl,
  Toolbar,
  useToast,
} from "@/components/ui";
import { readError } from "@/lib/errors";
import {
  alertingActivityQueryOptions,
  alertingDeliveriesQueryOptions,
  alertingOccurrencesQueryOptions,
} from "@/lib/queries/alertingQueries";
import { settingsQueryOptions, stationsQueryOptions } from "@/lib/query/resourceQueries";
import { queryKeys } from "@/lib/query/queryKeys";
import { useActivityQuery } from "@/lib/query/useActivityQuery";
import type { AlertingActivity, AlertingCursor, AlertingIncident } from "@/lib/types/alerting";
import type { RoutingDeepLink } from "@/lib/types/routingDeepLinks";
import { clearAlertingActivity, markAlertingSeen, markAllAlertingSeen } from "@/lib/api/alerting";
import { collectorFailureTaskLabel } from "./collectorIncidentLabels";

type ChangeCenterRoutingDeepLink = Extract<
  RoutingDeepLink,
  { kind: "request" } | { kind: "station-key" } | { kind: "station" }
> & { source: "change_center" };

type ChangeCenterPageProps = {
  onOpenRoutingDeepLink?: (link: ChangeCenterRoutingDeepLink) => void;
  onOpenSettings?: () => void;
  selectedView?: ChangeCenterView;
  onSelectedViewChange?: (view: ChangeCenterView) => void;
};

type ChangeCenterLinkEvent = Pick<AlertingIncident, "stationId"> & Partial<Pick<AlertingActivity, "objectType" | "objectId" | "stationKeyId">> & {
  requestLogId?: string | null;
  objectType?: string | null;
  objectId?: string | null;
  stationKeyId?: string | null;
};

function createChangeCenterRoutingLink(event: ChangeCenterLinkEvent): ChangeCenterRoutingDeepLink | null {
  if (event.requestLogId) {
    return { kind: "request", requestLogId: event.requestLogId, source: "change_center" };
  }
  if (event.objectType === "station_key" && (event.stationKeyId || event.objectId)) {
    return { kind: "station-key", stationKeyId: event.stationKeyId ?? event.objectId!, source: "change_center" };
  }
  if (event.objectType === "station" && (event.stationId || event.objectId)) {
    return { kind: "station", stationId: event.stationId ?? event.objectId!, source: "change_center" };
  }
  return event.stationId ? { kind: "station", stationId: event.stationId, source: "change_center" } : null;
}

export type ChangeCenterView = "all" | "unread";
type Severity = "all" | "critical" | "warning" | "info";

const PAGE_SIZE_OPTIONS = [20, 50, 100];
export const CHANGE_CENTER_DEFAULT_VIEW: ChangeCenterView = "all";
export const CHANGE_CENTER_VIEW_OPTIONS: Array<{ value: ChangeCenterView; label: string }> = [
  { value: "all", label: "全部" },
  { value: "unread", label: "未读" },
];
export const CHANGE_CENTER_CLEAR_SCOPE_BY_VIEW = {
  all: "all",
  unread: "all",
} as const satisfies Record<ChangeCenterView, "incidents" | "information" | "all">;
export const CHANGE_CENTER_MARK_SEEN_SCOPE_BY_VIEW = CHANGE_CENTER_CLEAR_SCOPE_BY_VIEW;

export function ChangeCenterPage({
  onOpenRoutingDeepLink,
  onOpenSettings,
  selectedView,
  onSelectedViewChange,
}: ChangeCenterPageProps = {}) {
  const toast = useToast();
  const queryClient = useQueryClient();
  const [internalView, setInternalView] = useState<ChangeCenterView>(CHANGE_CENTER_DEFAULT_VIEW);
  const view = selectedView ?? internalView;
  const [severity, setSeverity] = useState<Severity>("all");
  const [query, setQuery] = useState("");
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(20);
  const [pageCursors, setPageCursors] = useState<Record<number, AlertingCursor | null>>({ 1: null });
  const [busyIncidentId, setBusyIncidentId] = useState<string | null>(null);
  const [isMarkingAllSeen, setIsMarkingAllSeen] = useState(false);
  const [isClearingAll, setIsClearingAll] = useState(false);
  const [isClearConfirmOpen, setIsClearConfirmOpen] = useState(false);
  const [expandedIncidentKey, setExpandedIncidentKey] = useState<string | null>(null);
  const stationsQuery = useActivityQuery(stationsQueryOptions());
  const settingsQuery = useActivityQuery(settingsQueryOptions());
  const developerModeEnabled = settingsQuery.data?.developerModeEnabled ?? false;
  const activityQuery = useActivityQuery({
    ...alertingActivityQueryOptions({
      severity: severity === "all" ? null : severity,
      recordType: null,
      unreadOnly: view === "unread",
      cursor: pageCursors[page] ?? null,
      limit: pageSize,
    }),
    enabled: true,
  });
  const activities = useMemo<AlertingActivity[]>(
    () => activityQuery.data?.items ?? [],
    [activityQuery.data?.items],
  );
  const pageData = activityQuery.data;
  const activeQuery = activityQuery;
  const stationNames = useMemo(
    () => new Map((stationsQuery.data ?? []).map((station) => [station.id, station.name] as const)),
    [stationsQuery.data],
  );
  const filteredActivities = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return activities.filter((activity) => {
      if (view === "unread" && activity.seenAtMs != null) return false;
      if (!needle) return true;
      return [activity.eventType, activity.recordType === "incident" ? activity.groupName : null, activity.conditionKey, activity.stationId, activity.lifecycleState, activity.reasonCode, activity.objectId]
        .filter((value): value is string => Boolean(value))
        .some((value) => value.toLowerCase().includes(needle));
    });
  }, [activities, query, view]);
  const pageInfo = useMemo(() => {
    return {
      currentPage: page,
      totalPages: pageData?.nextCursor ? page + 1 : page,
      startIndex: filteredActivities.length === 0 ? 0 : 1,
      endIndex: filteredActivities.length,
      total: filteredActivities.length,
      items: filteredActivities,
    };
  }, [filteredActivities, pageData?.nextCursor, page]);
  const shouldShowPagination =
    pageInfo.currentPage > 1 || pageInfo.total >= 20 || Boolean(pageData?.nextCursor);

  async function refresh(showSuccess = false) {
    try {
      await Promise.all([
        queryClient.refetchQueries({ queryKey: ["alertingCurrent"], type: "active" }),
        queryClient.refetchQueries({ queryKey: ["alertingActivity"], type: "active" }),
        queryClient.refetchQueries({ queryKey: queryKeys.stations, type: "active" }),
      ]);
      if (showSuccess) toast.success("变更中心已刷新");
    } catch (requestError) {
      toast.error("刷新失败", readError(requestError));
    }
  }

  function changeView(next: ChangeCenterView) {
    if (selectedView == null) {
      setInternalView(next);
    }
    onSelectedViewChange?.(next);
    setPage(1);
    setPageCursors({ 1: null });
  }

  function changeSeverity(next: Severity) {
    setSeverity(next);
    setPage(1);
    setPageCursors({ 1: null });
  }

  function changePage(nextPage: number) {
    if (nextPage > page && pageData?.nextCursor) {
      setPageCursors((current) => ({ ...current, [nextPage]: pageData.nextCursor }));
    }
    setPage(nextPage);
  }

  async function markSeen(activity: AlertingActivity) {
    setBusyIncidentId(activity.id);
    try {
      await markAlertingSeen(activity.recordType === "change"
        ? { recordType: "change", id: activity.id }
        : { recordType: "incident", id: activity.id, episodeNumber: activity.episodeNumber });
      await queryClient.invalidateQueries({ queryKey: ["alertingCurrent"] });
      await queryClient.invalidateQueries({ queryKey: ["alertingActivity"] });
    } catch (requestError) {
      toast.error("标记变更已读失败", readError(requestError));
    } finally {
      setBusyIncidentId(null);
    }
  }

  async function markAllSeen() {
    setIsMarkingAllSeen(true);
    try {
      const markedCount = await markAllAlertingSeen({
        severity: severity === "all" ? null : severity,
        recordScope: CHANGE_CENTER_MARK_SEEN_SCOPE_BY_VIEW[view],
      });
      await queryClient.invalidateQueries({ queryKey: ["alertingCurrent"] });
      await queryClient.invalidateQueries({ queryKey: ["alertingActivity"] });
      toast.success(markedCount > 0 ? `已将 ${markedCount} 条变更标记为已读` : "没有需要标记的未读变更");
    } catch (requestError) {
      toast.error("一键标记已读失败", readError(requestError));
    } finally {
      setIsMarkingAllSeen(false);
    }
  }

  function requestClearAll() {
    if (activities.length === 0) return;
    setIsClearConfirmOpen(true);
  }

  async function confirmClearAll() {
    setIsClearingAll(true);
    try {
      const clearedCount = await clearAlertingActivity({
        severity: severity === "all" ? null : severity,
        lifecycleState: null,
        recordScope: CHANGE_CENTER_CLEAR_SCOPE_BY_VIEW[view],
      });
      await queryClient.invalidateQueries({ queryKey: ["alertingCurrent"] });
      await queryClient.invalidateQueries({ queryKey: ["alertingActivity"] });
      setExpandedIncidentKey(null);
      setPage(1);
      setPageCursors({ 1: null });
      toast.success(clearedCount > 0 ? `已清空 ${clearedCount} 条变更记录` : "没有可清空的变更记录");
    } catch (requestError) {
      toast.error("清空变更失败", readError(requestError));
    } finally {
      setIsClearingAll(false);
      setIsClearConfirmOpen(false);
    }
  }

  return (
    <PageScaffold
      title="变更中心"
      actions={
        <>
          {onOpenSettings ? (
            <IconButton className="h-9 w-9" label="变更中心设置" title="变更中心设置" onClick={onOpenSettings}>
              <Settings className="h-5 w-5" />
            </IconButton>
          ) : null}
        </>
      }
    >
      <div className="grid gap-[var(--shell-page-gap)]">
        <div className="grid gap-3 md:grid-cols-3">
          <SummaryTile label="活动问题" value={pageData?.activeCount ?? 0} />
          <SummaryTile label="未读提醒" value={pageData?.unseenCount ?? 0} />
          <SummaryTile label="当前加载" value={activities.length} />
        </div>
        <div className="min-w-0">
          <div data-testid="change-center-toolbar-surface" className="rounded-[var(--surface-radius)] border border-border bg-surface shadow-[var(--surface-shadow)]">
            <Toolbar className="flex-wrap">
              <div className="flex min-w-0 flex-1 flex-wrap items-center gap-2">
                <SegmentedControl value={view} options={CHANGE_CENTER_VIEW_OPTIONS} onChange={changeView} />
                <SelectControl ariaLabel="严重度" className={inputClassName} value={severity} options={[{ value: "all", label: "全部类型" }, { value: "critical", label: "严重" }, { value: "warning", label: "警告" }, { value: "info", label: "信息" }]} onChange={changeSeverity} />
                <div className="relative">
                  <Search className="pointer-events-none absolute left-2.5 top-2 h-4 w-4 text-muted-foreground" />
                  <input aria-label="搜索问题" className={`${inputClassName} pl-8`} value={query} placeholder="搜索事件或站点" onChange={(event) => { setQuery(event.target.value); setPage(1); }} />
                </div>
              </div>
              <div className="flex shrink-0 items-center gap-2">
                <Button variant="secondary" onClick={() => void markAllSeen()} disabled={isMarkingAllSeen || (pageData?.unseenCount ?? 0) === 0}><CheckCheck className="h-4 w-4" />一键已读</Button>
                <Button variant="secondary" onClick={() => void refresh(true)} disabled={activeQuery.isFetching}><RefreshCw className="h-4 w-4" />刷新</Button>
                <details className="relative">
                  <summary aria-label="更多操作" title="更多操作" className="flex h-9 w-9 cursor-pointer list-none items-center justify-center rounded-[4px] border border-border bg-surface text-muted-foreground outline-none transition hover:bg-selected hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring [&::-webkit-details-marker]:hidden"><MoreHorizontal className="h-4 w-4" /></summary>
                  <div className="absolute right-0 z-10 mt-1 w-32 rounded-[4px] border border-border bg-surface p-1 shadow-[var(--surface-shadow)]">
                    <Button className="w-full justify-start" size="sm" variant="ghost" onClick={requestClearAll} disabled={isClearingAll || activities.length === 0}>清空变更</Button>
                  </div>
                </details>
              </div>
            </Toolbar>
          </div>
          <div data-testid="change-center-list-surface" className="mt-3 min-w-0 overflow-hidden rounded-[var(--surface-radius)] border border-border bg-surface shadow-[var(--surface-shadow)]">
            {activeQuery.error ? <div className="border-b border-danger-border bg-danger-surface px-3 py-2 text-sm text-danger-foreground">{readError(activeQuery.error)}</div> : null}
            {pageInfo.items.length === 0 ? <EmptyState title={activeQuery.isPending ? "正在加载变更" : "暂无变更"} description="告警、恢复状态和信息类变更会按时间显示在这里。" /> : (
              <div className="divide-y divide-border bg-surface">
                {pageInfo.items.map((activity) => {
                  const key = `${activity.recordType}:${activity.id}:${activity.episodeNumber ?? 0}`;
                  const common = { stationName: activity.stationId ? stationNames.get(activity.stationId) : null, developerModeEnabled, expanded: expandedIncidentKey === key, onToggle: () => setExpandedIncidentKey((current) => current === key ? null : key), onOpenRoutingDeepLink };
                  return activity.recordType === "change"
                    ? <ChangeRow key={key} activity={activity} busy={busyIncidentId === activity.id} onMarkSeen={() => void markSeen(activity)} {...common} />
                    : <IncidentRow key={key} incident={activity} busy={busyIncidentId === activity.id} onMarkSeen={() => void markSeen(activity)} {...common} />;
                })}
              </div>
            )}
          </div>
          {shouldShowPagination ? <div data-testid="change-center-pagination-surface" className="mt-4 flex min-h-12 flex-wrap items-center justify-between gap-3 border border-border bg-surface px-3 py-2 text-xs text-muted-foreground">
            <div className="flex flex-wrap items-center gap-3"><span>第 {pageInfo.currentPage} 页：{pageInfo.startIndex}-{pageInfo.endIndex}</span><label className="flex items-center gap-2"><span>每页数量</span><select aria-label="每页数量" value={pageSize} onChange={(event) => { setPageSize(Number(event.target.value)); setPage(1); setPageCursors({ 1: null }); }} className="h-8 rounded-[4px] border border-border bg-surface px-2 text-sm text-foreground outline-none focus:border-ring">{PAGE_SIZE_OPTIONS.map((size) => <option key={size} value={size}>{size}</option>)}</select></label></div>
            <Pagination ariaLabel="变更中心分页" page={pageInfo.currentPage} totalPages={pageInfo.totalPages} disabled={activeQuery.isFetching} onPageChange={changePage} />
          </div> : null}
        </div>
      </div>
      <ConfirmDialog
        open={isClearConfirmOpen}
        title="清空变更记录"
        description="确认永久清空当前标签和严重程度范围内的变更记录吗？活动告警对应的异常仍存在时，会在下一次采集后重新生成。"
        confirmLabel="永久清空"
        confirming={isClearingAll}
        onCancel={() => setIsClearConfirmOpen(false)}
        onConfirm={() => void confirmClearAll()}
      />
    </PageScaffold>
  );
}

function IncidentRow({ incident, stationName, busy, developerModeEnabled, expanded, onToggle, onMarkSeen, onOpenRoutingDeepLink }: { incident: AlertingIncident; stationName: string | null | undefined; busy: boolean; developerModeEnabled: boolean; expanded: boolean; onToggle: () => void; onMarkSeen: () => void; onOpenRoutingDeepLink?: (link: ChangeCenterRoutingDeepLink) => void }) {
  const routingLink = createChangeCenterRoutingLink(incident);
  const taskLabel = collectorFailureTaskLabel(incident);
  const title = incidentTitle(incident, taskLabel);
  const objectTitle = incidentObjectTitle(incident, stationName);
  return <div className="group bg-surface">
    <div className={`grid min-h-[60px] w-full items-center gap-3 pl-3 pr-2 py-2 text-left ${developerModeEnabled ? "grid-cols-[28px_8px_minmax(0,1fr)_96px_28px]" : "grid-cols-[8px_minmax(0,1fr)_96px_28px]"}`}>
    {developerModeEnabled ? <IconButton className="h-7 w-7 text-muted-foreground" label={expanded ? "收起问题" : "展开问题"} onClick={onToggle}>{expanded ? <ChevronUp className="h-4 w-4" /> : <ChevronDown className="h-4 w-4" />}</IconButton> : null}
    <SeverityDot severity={incident.severity} />
    <div className="min-w-0"><div className={`truncate text-[13px] ${incident.seenAtMs == null ? "font-semibold" : "font-medium"} text-foreground`}>{objectTitle}</div><div className="truncate text-xs text-muted-foreground">{title} · {incidentSummary(incident, stationName)}</div></div>
    <div className="flex w-[96px] items-center justify-end whitespace-nowrap text-xs text-muted-foreground"><span>{formatChangeTime(incident.lastSeenAtMs)}</span></div>
    <StatusSlot seenAtMs={incident.seenAtMs} busy={busy} onMarkSeen={onMarkSeen} routingLink={routingLink} onOpenRoutingDeepLink={onOpenRoutingDeepLink} />
    </div>
    {developerModeEnabled && expanded ? <IncidentDetail incident={incident} /> : null}
  </div>;
}

export function incidentTitle(
  incident: Pick<AlertingIncident, "eventType" | "groupName">,
  taskLabel?: string | null,
) {
  if (incident.eventType === "group_missing" && incident.groupName) {
    return `${eventLabel(incident.eventType)} · ${incident.groupName}`;
  }
  return `${eventLabel(incident.eventType)}${taskLabel ? ` · ${taskLabel}` : ""}`;
}

export function incidentSummary(
  incident: Pick<AlertingIncident, "conditionKey" | "eventType" | "groupName" | "lifecycleState" | "occurrenceCount" | "severity" | "stationId">,
  stationName: string | null | undefined,
) {
  const station = stationName ?? incident.stationId ?? incident.conditionKey;
  if (incident.severity === "info") {
    const detail = incident.eventType === "group_missing"
      ? "远程分组未找到"
      : eventLabel(incident.eventType);
    return incident.eventType === "group_missing" ? detail : `${station} · ${detail}`;
  }
  const stateLabel = incident.lifecycleState === "resolved" ? "已恢复" : incident.lifecycleState === "recovering" ? "恢复中" : incident.lifecycleState === "pending" ? "检测中" : "未处理";
  return `${station} · ${stateLabel} · 已出现 ${incident.occurrenceCount} 次`;
}

function ChangeRow({ activity, stationName, busy, developerModeEnabled, expanded, onToggle, onMarkSeen, onOpenRoutingDeepLink }: { activity: Extract<AlertingActivity, { recordType: "change" }>; stationName: string | null | undefined; busy: boolean; developerModeEnabled: boolean; expanded: boolean; onToggle: () => void; onMarkSeen: () => void; onOpenRoutingDeepLink?: (link: ChangeCenterRoutingDeepLink) => void }) {
  const routingLink = createChangeCenterRoutingLink(activity);
  const rateTransition = changeRateTransition(activity);
  return <div className="group bg-surface">
    <div className={`grid min-h-[60px] w-full items-center gap-3 pl-3 pr-2 py-2 text-left ${developerModeEnabled ? "grid-cols-[28px_8px_minmax(0,1fr)_96px_28px]" : "grid-cols-[8px_minmax(0,1fr)_96px_28px]"}`}>
      {developerModeEnabled ? <IconButton className="h-7 w-7 text-muted-foreground" label={expanded ? "收起变更" : "展开变更"} onClick={onToggle}>{expanded ? <ChevronUp className="h-4 w-4" /> : <ChevronDown className="h-4 w-4" />}</IconButton> : null}
      <SeverityDot severity={activity.severity} />
      <div className="min-w-0"><div className={`truncate text-[13px] ${activity.seenAtMs == null ? "font-semibold" : "font-medium"} text-foreground`}>{changeObjectTitle(activity, stationName)}</div><div className="truncate text-xs text-muted-foreground">{eventLabel(activity.eventType)}{rateTransition ? <> <span className="font-semibold text-foreground">{rateTransition.oldValue} → {rateTransition.newValue}</span></> : activity.eventType === "group_added" ? null : <> · {changeSummary(activity)}</>}</div></div>
      <div className="flex w-[96px] items-center justify-end whitespace-nowrap text-xs text-muted-foreground"><span>{formatChangeTime(activity.activityAtMs)}</span></div>
      <StatusSlot seenAtMs={activity.seenAtMs} busy={busy} onMarkSeen={onMarkSeen} routingLink={routingLink} onOpenRoutingDeepLink={onOpenRoutingDeepLink} />
    </div>
    {developerModeEnabled && expanded ? <ChangeDetail activity={activity} /> : null}
  </div>;
}

function StatusSlot({ seenAtMs, busy, onMarkSeen, routingLink, onOpenRoutingDeepLink }: { seenAtMs: number | null; busy: boolean; onMarkSeen: () => void; routingLink: ChangeCenterRoutingDeepLink | null; onOpenRoutingDeepLink?: (link: ChangeCenterRoutingDeepLink) => void }) {
  const unread = seenAtMs == null;
  return <div className="status-slot relative flex h-7 w-7 items-center justify-center">
    {unread ? <span aria-label="未读" title="未读" className="absolute left-1/2 top-1/2 h-2 w-2 -translate-x-1/2 -translate-y-1/2 rounded-full bg-primary-solid transition-opacity group-hover:opacity-0 group-focus-within:opacity-0" /> : null}
    {unread ? <IconButton className="absolute left-1/2 top-1/2 h-7 w-7 -translate-x-1/2 -translate-y-1/2 pointer-events-none opacity-0 text-muted-foreground transition-opacity group-hover:pointer-events-auto group-hover:opacity-100 group-focus-within:pointer-events-auto group-focus-within:opacity-100 hover:bg-selected hover:text-primary" label="标记已读" disabled={busy} onClick={onMarkSeen}><Check className="h-4 w-4" /></IconButton> : null}
    {!unread && onOpenRoutingDeepLink && routingLink ? <IconButton className="absolute left-1/2 top-1/2 h-7 w-7 -translate-x-1/2 -translate-y-1/2 pointer-events-none opacity-0 text-muted-foreground transition-opacity group-hover:pointer-events-auto group-hover:opacity-100 group-focus-within:pointer-events-auto group-focus-within:opacity-100 hover:bg-selected hover:text-primary" label="打开站点" onClick={() => onOpenRoutingDeepLink(routingLink)}><Route className="h-4 w-4" /></IconButton> : null}
  </div>;
}

function SeverityDot({ severity }: { severity: AlertingActivity["severity"] }) {
  const className = severity === "critical" ? "bg-danger-foreground" : severity === "warning" ? "bg-warning-foreground" : "bg-info-foreground";
  return <span aria-label={severityLabel(severity)} title={severityLabel(severity)} className={`h-2 w-2 rounded-full ${className}`} />;
}

function incidentObjectTitle(incident: AlertingIncident, stationName: string | null | undefined) {
  const station = stationName ?? incident.stationId ?? "全局";
  return incident.groupName ? `${station} · ${incident.groupName}` : station;
}

export function changeObjectTitle(activity: Extract<AlertingActivity, { recordType: "change" }>, stationName: string | null | undefined) {
  const groupName = stringValue(parseAuditObject(activity.newValueJson)?.groupName);
  const station = stationName ?? activity.stationId ?? "全局";
  return groupName ? `${station} · ${groupName}` : station;
}

function changeRateTransition(activity: Extract<AlertingActivity, { recordType: "change" }>) {
  if (activity.eventType !== "group_rate_changed") return null;
  const details = parseAuditObject(activity.newValueJson);
  const oldValue = scalarValue(details?.oldEffectiveRateMultiplier);
  const newValue = scalarValue(details?.newEffectiveRateMultiplier);
  return oldValue && newValue ? { oldValue, newValue } : null;
}

function ChangeDetail({ activity }: { activity: Extract<AlertingActivity, { recordType: "change" }> }) {
  return <div className="grid gap-3 border-t border-border bg-muted/20 px-12 py-3 text-xs md:grid-cols-2">
    <section><div className="mb-2 font-semibold text-foreground">来源</div><div className="space-y-1 text-muted-foreground"><div>{sourceLabel(activity.source)} · {reasonLabel(activity.reasonCode)}</div><div className="break-all">{activity.objectType ?? "对象"}{activity.objectId ? ` · ${activity.objectId}` : ""}</div></div></section>
    <section><div className="mb-2 font-semibold text-foreground">变更内容</div><div className="space-y-1 text-muted-foreground">{activity.oldValueJson ? <div>原值：{formatAuditValue(activity.oldValueJson)}</div> : null}{activity.newValueJson ? <div>新值：{formatAuditValue(activity.newValueJson)}</div> : null}{activity.impactJson ? <div>影响：{formatAuditValue(activity.impactJson)}</div> : null}{!activity.oldValueJson && !activity.newValueJson && !activity.impactJson ? <div>未记录结构化详情</div> : null}</div></section>
  </div>;
}

function IncidentDetail({ incident }: { incident: AlertingIncident }) {
  const occurrencesQuery = useActivityQuery(alertingOccurrencesQueryOptions({ incidentId: incident.id, episodeNumber: incident.episodeNumber, limit: 20 }));
  const deliveriesQuery = useActivityQuery(alertingDeliveriesQueryOptions({ incidentId: incident.id, episodeNumber: incident.episodeNumber, limit: 20 }));
  return <div className="grid gap-3 border-t border-border bg-muted/20 px-12 py-3 text-xs md:grid-cols-2">
    <section><div className="mb-2 font-semibold text-foreground">出现历史</div>{occurrencesQuery.isPending ? <div className="text-muted-foreground">正在加载…</div> : occurrencesQuery.data?.items.length ? <div className="space-y-1">{occurrencesQuery.data.items.map((item) => <div key={item.id} className="flex items-center justify-between gap-2"><span className="truncate text-muted-foreground">{observationLabel(item.observationKind)} · {item.reasonCode ?? item.source}</span><span className="shrink-0 text-foreground">{formatChangeTime(item.observedAtMs)}</span></div>)}</div> : <div className="text-muted-foreground">暂无出现记录</div>}</section>
    <section><div className="mb-2 font-semibold text-foreground">投递历史</div>{deliveriesQuery.isPending ? <div className="text-muted-foreground">正在加载…</div> : deliveriesQuery.data?.items.length ? <div className="space-y-1">{deliveriesQuery.data.items.map((item) => <div key={item.id} className="flex items-center justify-between gap-2"><span className="truncate text-muted-foreground">{channelLabel(item.channel)} · {deliveryKindLabel(item.deliveryKind)}</span><span className="shrink-0 text-foreground">{deliveryStatusLabel(item.status)}</span></div>)}</div> : <div className="text-muted-foreground">暂无投递记录</div>}</section>
  </div>;
}

function eventLabel(eventType: string) {
  const labels: Record<string, string> = {
    collector_failed: "采集失败",
    station_down: "站点不可用",
    balance_low: "余额偏低",
    balance_depleted: "余额耗尽",
    price_expired: "价格已过期",
    key_invalid: "密钥无效",
    route_impacted: "路由受影响",
    group_missing: "分组缺失",
    key_group_unresolved: "密钥分组无法解析",
    group_added: "新增分组",
    rate_changed: "倍率变化",
    group_rate_changed: "分组倍率变化",
    price_changed: "价格变化",
    model_added: "新增模型",
    model_removed: "模型移除",
    audit_change: "配置发生变化",
  };
  return labels[eventType] ?? eventType;
}

export function changeSummary(activity: Extract<AlertingActivity, { recordType: "change" }>) {
  const details = parseAuditObject(activity.newValueJson);
  const groupName = stringValue(details?.groupName);
  if (activity.eventType === "group_rate_changed") {
    const oldRate = scalarValue(details?.oldEffectiveRateMultiplier);
    const newRate = scalarValue(details?.newEffectiveRateMultiplier);
    if (oldRate && newRate) return `${groupName ? `${groupName} · ` : ""}倍率 ${oldRate} → ${newRate}`;
  }
  if (activity.eventType === "group_added" && groupName) return `${groupName} · 新增分组`;
  if (groupName) return `${groupName} · ${reasonLabel(activity.reasonCode)}`;
  return reasonLabel(activity.reasonCode);
}

function parseAuditObject(value: string | null): Record<string, unknown> | null {
  if (!value) return null;
  try {
    const parsed: unknown = JSON.parse(value);
    return parsed != null && typeof parsed === "object" && !Array.isArray(parsed) ? parsed as Record<string, unknown> : null;
  } catch {
    return null;
  }
}

function stringValue(value: unknown) {
  return typeof value === "string" && value.trim() ? value.trim() : null;
}

function scalarValue(value: unknown) {
  return typeof value === "number" || typeof value === "string" ? String(value) : null;
}

function formatAuditValue(value: string) {
  try {
    const parsed: unknown = JSON.parse(value);
    if (parsed == null || typeof parsed !== "object") return String(parsed);
    return JSON.stringify(parsed);
  } catch {
    return value;
  }
}

function reasonLabel(value: string | null) {
  return ({ group_added: "新增分组", group_rate_changed: "分组倍率变化", key_group_bound: "密钥已绑定分组", price_changed: "价格变化", model_added: "新增模型", model_removed: "模型移除" } as Record<string, string>)[value ?? ""] ?? value ?? "配置发生变化";
}

function sourceLabel(value: string | null) {
  return ({ collector: "采集器", migration: "数据迁移", user: "用户操作" } as Record<string, string>)[value ?? ""] ?? value ?? "系统";
}

function severityLabel(value: string) {
  return ({ critical: "严重", warning: "警告", info: "信息" } as Record<string, string>)[value] ?? value;
}

function observationLabel(value: string) {
  return ({ observed: "已观察", recovered: "已恢复", triggered: "已触发" } as Record<string, string>)[value] ?? value;
}

function channelLabel(value: string) {
  return ({ in_app: "应用内", desktop: "桌面" } as Record<string, string>)[value] ?? value;
}

function deliveryKindLabel(value: string) {
  return ({ initial: "首次提醒", repeat: "重复提醒", recovery: "恢复提醒" } as Record<string, string>)[value] ?? value;
}

function deliveryStatusLabel(value: string) {
  return ({ pending: "待投递", claimed: "投递中", delivered: "已投递", failed: "失败", suppressed: "已抑制" } as Record<string, string>)[value] ?? value;
}

function formatChangeTime(value: number) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "时间未知";
  return date.toLocaleString("zh-CN", { month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit" });
}

function SummaryTile({ label, value }: { label: string; value: number }) {
  return <div className="rounded-[var(--surface-radius)] border border-border bg-surface p-3 shadow-[var(--surface-shadow)]"><div className="text-xs text-muted-foreground">{label}</div><div className="mt-1 text-2xl font-semibold text-foreground">{value}</div></div>;
}

const inputClassName = "h-8 rounded-[12px] border border-border bg-surface px-3 text-sm text-foreground shadow-surface outline-none transition focus:border-ring focus:ring-2 focus:ring-ring/30";
