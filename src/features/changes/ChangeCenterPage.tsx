import { useEffect, useMemo, useState } from "react";
import { CheckCheck, CheckCircle2, ChevronLeft, ChevronRight, Eye, RefreshCw, Search, Trash2, XCircle } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import {
  Button,
  ConfirmDialog,
  EmptyState,
  InspectorPanel,
  SegmentedControl,
  SelectControl,
  StatusBadge,
  Toolbar,
  useToast,
} from "@/components/ui";
import {
  clearChangeEvents,
  dismissChangeEvent,
  listChangeEvents,
  markChangeEventRead,
  notifyChangeEventsUpdated,
  resolveChangeEvent,
} from "@/lib/api/changeEvents";
import { getSettings, SETTINGS_UPDATED_EVENT } from "@/lib/api/settings";
import { listStations } from "@/lib/api/stations";
import type { ChangeEvent } from "@/lib/types/changeEvents";
import type { AppSettings } from "@/lib/types/settings";
import {
  buildChangeEventListItem,
  eventTypeLabels,
  filterChangeEvents,
  formatChangeTime,
  markUnreadChangeEventsRead,
  objectTypeLabels,
  paginateChangeEvents,
  parseJsonObject,
  severityLabels,
  severityTone,
  statusLabels,
  unreadRiskCount,
  type ChangeFilter,
} from "./changeEventViewModels";

export function ChangeCenterPage() {
  const toast = useToast();
  const [events, setEvents] = useState<ChangeEvent[]>([]);
  const [stationNamesById, setStationNamesById] = useState<Map<string, string>>(new Map());
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [developerModeEnabled, setDeveloperModeEnabled] = useState(false);
  const [filter, setFilter] = useState<ChangeFilter>({ severity: "all", status: "active", objectType: "all", query: "" });
  const [page, setPage] = useState(1);
  const [clearConfirmOpen, setClearConfirmOpen] = useState(false);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void refresh();
  }, []);

  useEffect(() => {
    function refreshDeveloperMode() {
      void getSettings()
        .then((settings) => setDeveloperModeEnabled(settings.developerModeEnabled))
        .catch(() => setDeveloperModeEnabled(false));
    }

    function handleSettingsUpdated(event: Event) {
      const settings = (event as CustomEvent<AppSettings>).detail;
      if (settings) {
        setDeveloperModeEnabled(settings.developerModeEnabled);
        return;
      }
      refreshDeveloperMode();
    }

    refreshDeveloperMode();
    window.addEventListener(SETTINGS_UPDATED_EVENT, handleSettingsUpdated);
    return () => window.removeEventListener(SETTINGS_UPDATED_EVENT, handleSettingsUpdated);
  }, []);

  async function refresh(showSuccess = false) {
    setLoading(true);
    setError(null);
    try {
      const [nextEvents, stations] = await Promise.all([listChangeEvents(), listStations()]);
      setStationNamesById(new Map(stations.map((station) => [station.id, station.name])));
      setEvents(nextEvents);
      setSelectedId((current) => (current && nextEvents.some((event) => event.id === current) ? current : nextEvents[0]?.id ?? null));
      if (showSuccess) {
        toast.success("变更中心已刷新");
      }
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      toast.error("刷新变更中心失败", message);
    } finally {
      setLoading(false);
    }
  }

  async function runAction(action: () => Promise<ChangeEvent>, successMessage: string) {
    setSaving(true);
    try {
      const updated = await action();
      setEvents((current) => current.map((event) => (event.id === updated.id ? updated : event)));
      notifyChangeEventsUpdated();
      toast.success(successMessage);
    } catch (requestError) {
      toast.error("更新变更状态失败", readError(requestError));
    } finally {
      setSaving(false);
    }
  }

  async function markAllRead() {
    setSaving(true);
    try {
      const result = await markUnreadChangeEventsRead(events, markChangeEventRead);
      setEvents(result.events);
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
      await clearChangeEvents();
      setEvents([]);
      setSelectedId(null);
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
    () => filterChangeEvents(events, filter, { stationNamesById }),
    [events, filter, stationNamesById],
  );
  const pageInfo = useMemo(() => paginateChangeEvents(filteredEvents, page, CHANGE_EVENTS_PAGE_SIZE), [filteredEvents, page]);
  const selected = pageInfo.events.find((event) => event.id === selectedId) ?? pageInfo.events[0] ?? null;
  const selectedItem = selected ? buildChangeEventListItem(selected, { stationNamesById }) : null;
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
          <SummaryTile label="未读风险" value={riskCount} tone={riskCount > 0 ? "text-rose-700" : "text-emerald-700"} />
          <SummaryTile label="严重" value={events.filter((event) => event.severity === "critical" && event.status !== "resolved").length} />
          <SummaryTile label="警告" value={events.filter((event) => event.severity === "warning" && event.status !== "resolved").length} />
          <SummaryTile label="信息" value={events.filter((event) => event.severity === "info").length} />
        </div>

        <div className="grid gap-[var(--shell-page-gap)] xl:grid-cols-[minmax(0,1fr)_420px]">
          <div className="min-w-0 overflow-hidden rounded-[var(--surface-radius)] border border-border bg-white shadow-[var(--surface-shadow)]">
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
            {error && <div className="border-b border-rose-100 bg-rose-50 px-3 py-2 text-sm text-rose-700">{error}</div>}
            {filteredEvents.length === 0 ? (
              <EmptyState
                title={loading ? "正在读取变更" : "暂无变更"}
                description="余额、密钥、采集、价格、倍率、模型和路由状态变化会在这里形成记录。"
              />
            ) : (
              <>
                <div className="divide-y divide-border bg-white">
                  {pageInfo.events.map((event) => (
                    <ChangeEventRow
                      key={event.id}
                      event={event}
                      stationNamesById={stationNamesById}
                      selected={selected?.id === event.id}
                      onSelect={() => setSelectedId(event.id)}
                    />
                  ))}
                </div>
                <div className="flex flex-wrap items-center justify-between gap-2 border-t border-border bg-slate-50 px-3 py-2 text-xs text-slate-500">
                  <span>
                    第 {pageInfo.startIndex}-{pageInfo.endIndex} 条 / 共 {pageInfo.totalCount} 条
                  </span>
                  <div className="flex items-center gap-2">
                    <Button
                      variant="secondary"
                      size="sm"
                      onClick={() => setPage((current) => Math.max(1, current - 1))}
                      disabled={loading || saving || pageInfo.page <= 1}
                    >
                      <ChevronLeft className="h-4 w-4" />
                      上一页
                    </Button>
                    <span className="min-w-[64px] text-center font-medium text-slate-700">
                      {pageInfo.page} / {pageInfo.totalPages}
                    </span>
                    <Button
                      variant="secondary"
                      size="sm"
                      onClick={() => setPage((current) => Math.min(pageInfo.totalPages, current + 1))}
                      disabled={loading || saving || pageInfo.page >= pageInfo.totalPages}
                    >
                      下一页
                      <ChevronRight className="h-4 w-4" />
                    </Button>
                  </div>
                </div>
              </>
            )}
          </div>

          <InspectorPanel title={selectedItem ? selectedItem.title : "变更详情"} description={selected ? eventTypeLabels[selected.eventType] ?? selected.eventType : "选择一条变更"}>
            {selected ? (
              <div className="space-y-4 p-4">
                <div className="flex flex-wrap items-center gap-2">
                  <StatusBadge tone={severityTone[selected.severity]}>{severityLabels[selected.severity]}</StatusBadge>
                  <StatusBadge tone={selected.status === "unread" ? "warning" : "info"}>{statusLabels[selected.status]}</StatusBadge>
                  <span className="text-xs text-muted-foreground">{formatChangeTime(selected.detectedAt)}</span>
                </div>
                <div className="rounded-[var(--surface-radius)] border border-border bg-slate-50 p-3 text-sm leading-6 text-slate-700">
                  {selected.message}
                </div>
                {developerModeEnabled ? (
                  <>
                    <JsonBlock title="旧值" value={parseJsonObject(selected.oldValueJson)} />
                    <JsonBlock title="新值" value={parseJsonObject(selected.newValueJson)} />
                    <JsonBlock title="影响" value={parseJsonObject(selected.impactJson)} />
                    <div className="grid gap-2 text-xs text-muted-foreground">
                      <div>对象：{objectTypeLabels[selected.objectType] ?? selected.objectType} / {selected.objectId ?? "-"}</div>
                      <div>来源：{selected.source}</div>
                      <div>去重键：{selected.dedupeKey}</div>
                    </div>
                  </>
                ) : null}
                <div className="flex flex-wrap justify-end gap-2">
                  <Button variant="outline" disabled={saving || selected.status === "read"} onClick={() => void runAction(() => markChangeEventRead(selected.id), "已标记为已读")}>
                    <Eye className="h-4 w-4" />
                    标记已读
                  </Button>
                  <Button variant="outline" disabled={saving || selected.status === "resolved"} onClick={() => void runAction(() => resolveChangeEvent(selected.id), "已标记为已解决")}>
                    <CheckCircle2 className="h-4 w-4" />
                    解决
                  </Button>
                  <Button variant="danger" disabled={saving || selected.status === "dismissed"} onClick={() => void runAction(() => dismissChangeEvent(selected.id), "已忽略")}>
                    <XCircle className="h-4 w-4" />
                    忽略
                  </Button>
                </div>
              </div>
            ) : (
              <EmptyState title="暂无详情" description="选择一条变更查看摘要和处理状态。" />
            )}
          </InspectorPanel>
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
  selected,
  onSelect,
}: {
  event: ChangeEvent;
  stationNamesById: Map<string, string>;
  selected: boolean;
  onSelect: () => void;
}) {
  const item = buildChangeEventListItem(event, { stationNamesById });
  return (
    <button
      type="button"
      aria-pressed={selected}
      onClick={onSelect}
      className={`grid min-h-[48px] w-full grid-cols-[56px_minmax(0,1fr)_88px] items-center gap-3 px-3 py-2 text-left transition-colors hover:bg-teal-50/45 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-[hsl(var(--accent)/0.35)] ${
        selected ? "bg-teal-50/70" : "bg-white"
      }`}
    >
      <div className="flex flex-col items-start gap-1">
        <StatusBadge tone={severityTone[event.severity]}>{item.severityLabel}</StatusBadge>
      </div>
      <div className="min-w-0">
        <div className="truncate text-[13px] font-semibold text-slate-900">{item.title}</div>
      </div>
      <div className="flex flex-col items-end text-xs text-slate-500">
        <span className="font-medium text-slate-700">{formatChangeTime(event.detectedAt)}</span>
      </div>
    </button>
  );
}

function SummaryTile({ label, value, tone = "text-slate-800" }: { label: string; value: number; tone?: string }) {
  return (
    <div className="rounded-[var(--surface-radius)] border border-border bg-white p-3 shadow-[var(--surface-shadow)]">
      <div className="text-xs text-muted-foreground">{label}</div>
      <div className={`mt-1 text-2xl font-semibold ${tone}`}>{value}</div>
    </div>
  );
}

function JsonBlock({ title, value }: { title: string; value: unknown }) {
  if (value == null) {
    return null;
  }
  return (
    <div className="rounded-[var(--surface-radius)] border border-border bg-white p-3">
      <div className="text-xs font-semibold text-slate-700">{title}</div>
      <pre className="mt-2 max-h-40 overflow-auto rounded-[var(--surface-radius)] bg-slate-50 p-2 text-[11px] leading-5 text-slate-600">
        {typeof value === "string" ? value : JSON.stringify(value, null, 2)}
      </pre>
    </div>
  );
}

function readError(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

const inputClassName =
  "h-8 rounded-[12px] border border-cyan-100 bg-cyan-50/45 px-3 text-sm text-slate-800 outline-none transition focus:border-teal-300 focus:bg-white focus:ring-2 focus:ring-teal-100";

const CHANGE_EVENTS_PAGE_SIZE = 20;
