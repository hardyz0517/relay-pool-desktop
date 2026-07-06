import { useEffect, useMemo, useState } from "react";
import { ArrowRight, CheckCheck, CheckCircle2, Eye, RefreshCw, Search, XCircle } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import {
  Button,
  EmptyState,
  InspectorPanel,
  SegmentedControl,
  SelectControl,
  StatusBadge,
  Toolbar,
  useToast,
} from "@/components/ui";
import {
  dismissChangeEvent,
  listChangeEvents,
  markChangeEventRead,
  notifyChangeEventsUpdated,
  resolveChangeEvent,
} from "@/lib/api/changeEvents";
import type { ChangeEvent } from "@/lib/types/changeEvents";
import {
  buildChangeEventListItem,
  type ChangeEventListDiff,
  eventTypeLabels,
  filterChangeEvents,
  formatChangeTime,
  markUnreadChangeEventsRead,
  objectTypeLabels,
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
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [filter, setFilter] = useState<ChangeFilter>({ severity: "all", status: "active", objectType: "all", query: "" });
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void refresh();
  }, []);

  async function refresh(showSuccess = false) {
    setLoading(true);
    setError(null);
    try {
      const nextEvents = await listChangeEvents();
      setEvents(nextEvents);
      setSelectedId((current) => current ?? nextEvents[0]?.id ?? null);
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

  const filteredEvents = useMemo(() => filterChangeEvents(events, filter), [events, filter]);
  const selected = filteredEvents.find((event) => event.id === selectedId) ?? filteredEvents[0] ?? null;
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
                  onChange={(status) => setFilter((current) => ({ ...current, status }))}
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
                  onChange={(severity) => setFilter((current) => ({ ...current, severity }))}
                />
                <SelectControl
                  ariaLabel="对象类型"
                  className={inputClassName}
                  value={filter.objectType}
                  options={[
                    { value: "all", label: "全部对象" },
                    ...objectOptions,
                  ]}
                  onChange={(objectType) => setFilter((current) => ({ ...current, objectType }))}
                />
                <div className="relative">
                  <Search className="pointer-events-none absolute left-2.5 top-2 h-4 w-4 text-muted-foreground" />
                  <input
                    className={`${inputClassName} pl-8`}
                    value={filter.query}
                    placeholder="搜索变更 / 对象 / 来源"
                    onChange={(event) => setFilter((current) => ({ ...current, query: event.target.value }))}
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
              <div className="divide-y divide-border bg-white">
                {filteredEvents.map((event) => (
                  <ChangeEventRow
                    key={event.id}
                    event={event}
                    selected={selected?.id === event.id}
                    onSelect={() => setSelectedId(event.id)}
                  />
                ))}
              </div>
            )}
          </div>

          <InspectorPanel title={selected ? selected.title : "变更详情"} description={selected ? eventTypeLabels[selected.eventType] ?? selected.eventType : "选择一条变更"}>
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
                <JsonBlock title="旧值" value={parseJsonObject(selected.oldValueJson)} />
                <JsonBlock title="新值" value={parseJsonObject(selected.newValueJson)} />
                <JsonBlock title="影响" value={parseJsonObject(selected.impactJson)} />
                <div className="grid gap-2 text-xs text-muted-foreground">
                  <div>对象：{objectTypeLabels[selected.objectType] ?? selected.objectType} / {selected.objectId ?? "-"}</div>
                  <div>来源：{selected.source}</div>
                  <div>去重键：{selected.dedupeKey}</div>
                </div>
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
              <EmptyState title="暂无详情" description="选择一条变更查看变化值和影响范围。" />
            )}
          </InspectorPanel>
        </div>
      </div>
    </PageScaffold>
  );
}

function ChangeEventRow({
  event,
  selected,
  onSelect,
}: {
  event: ChangeEvent;
  selected: boolean;
  onSelect: () => void;
}) {
  const item = buildChangeEventListItem(event);
  return (
    <button
      type="button"
      aria-pressed={selected}
      onClick={onSelect}
      className={`grid min-h-[64px] w-full grid-cols-[56px_minmax(0,1fr)_88px] gap-3 px-3 py-2.5 text-left transition-colors hover:bg-teal-50/45 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-[hsl(var(--accent)/0.35)] ${
        selected ? "bg-teal-50/70" : "bg-white"
      }`}
    >
      <div className="flex flex-col items-start gap-1">
        <StatusBadge tone={severityTone[event.severity]}>{item.severityLabel}</StatusBadge>
      </div>
      <div className="min-w-0">
        <div className="truncate text-[13px] font-semibold text-slate-900">{item.title}</div>
        <div className="mt-1 flex min-w-0 flex-wrap items-center gap-1.5 text-xs text-slate-500">
          <span className="truncate">{item.metaLabel}</span>
          <span className="text-slate-300">/</span>
          <span>{item.statusLabel}</span>
          {item.diff && (item.diff.before || item.diff.after) && <span className="text-slate-300">/</span>}
          <ChangeDiff diff={item.diff} />
        </div>
      </div>
      <div className="flex flex-col items-end text-xs text-slate-500">
        <span className="font-medium text-slate-700">{formatChangeTime(event.detectedAt)}</span>
      </div>
    </button>
  );
}

function ChangeDiff({ diff }: { diff: ChangeEventListDiff | null }) {
  if (!diff || (!diff.before && !diff.after)) {
    return null;
  }

  return (
    <span className="flex min-w-0 flex-wrap items-center gap-1.5 text-xs">
      <span className="text-slate-500">{diff.label}</span>
      {diff.before && diff.after ? (
        <>
          <DiffValue tone="before">{diff.before}</DiffValue>
          <ArrowRight className="h-3.5 w-3.5 text-slate-400" />
          <DiffValue tone="after">{diff.after}</DiffValue>
        </>
      ) : diff.after ? (
        <>
          <span className="rounded-full bg-emerald-50 px-2 py-0.5 font-medium text-emerald-700">新增</span>
          <DiffValue tone="after">{diff.after}</DiffValue>
        </>
      ) : (
        <>
          <span className="rounded-full bg-rose-50 px-2 py-0.5 font-medium text-rose-700">移除</span>
          <DiffValue tone="before">{diff.before}</DiffValue>
        </>
      )}
    </span>
  );
}

function DiffValue({ children, tone }: { children: string | null; tone: "before" | "after" }) {
  if (!children) {
    return null;
  }
  return (
    <span
      className={
        tone === "before"
          ? "max-w-[220px] truncate rounded-[6px] border border-slate-200 bg-white px-2 py-0.5 font-medium text-slate-600"
          : "max-w-[220px] truncate rounded-[6px] border border-teal-100 bg-teal-50 px-2 py-0.5 font-medium text-teal-800"
      }
    >
      {children}
    </span>
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
