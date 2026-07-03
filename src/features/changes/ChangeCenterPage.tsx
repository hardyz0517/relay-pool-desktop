import { useEffect, useMemo, useState } from "react";
import { CheckCircle2, Eye, RefreshCw, Search, XCircle } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import {
  Button,
  DataTableLite,
  EmptyState,
  InspectorPanel,
  SegmentedControl,
  SelectControl,
  StatusBadge,
  Toolbar,
  useToast,
  type DataTableColumn,
} from "@/components/ui";
import {
  dismissChangeEvent,
  listChangeEvents,
  markChangeEventRead,
  resolveChangeEvent,
} from "@/lib/api/changeEvents";
import type { ChangeEvent } from "@/lib/types/changeEvents";
import {
  eventTypeLabels,
  filterChangeEvents,
  formatChangeTime,
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
  const [filter, setFilter] = useState<ChangeFilter>({ severity: "all", status: "active", query: "" });
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
      toast.success(successMessage);
    } catch (requestError) {
      toast.error("更新变更状态失败", readError(requestError));
    } finally {
      setSaving(false);
    }
  }

  const filteredEvents = useMemo(() => filterChangeEvents(events, filter), [events, filter]);
  const selected = filteredEvents.find((event) => event.id === selectedId) ?? filteredEvents[0] ?? null;
  const riskCount = unreadRiskCount(events);

  const columns: DataTableColumn<ChangeEvent>[] = [
    {
      key: "severity",
      header: "级别",
      className: "w-20",
      render: (event) => <StatusBadge tone={severityTone[event.severity]}>{severityLabels[event.severity]}</StatusBadge>,
    },
    {
      key: "event",
      header: "变更",
      render: (event) => (
        <div className="min-w-0">
          <div className="truncate font-semibold text-slate-800">{event.title}</div>
          <div className="max-w-[520px] truncate text-xs text-muted-foreground">{event.message}</div>
        </div>
      ),
    },
    {
      key: "type",
      header: "类型",
      className: "w-28",
      render: (event) => eventTypeLabels[event.eventType] ?? event.eventType,
    },
    {
      key: "status",
      header: "状态",
      className: "w-24",
      render: (event) => statusLabels[event.status] ?? event.status,
    },
    {
      key: "time",
      header: "时间",
      className: "w-32",
      render: (event) => formatChangeTime(event.detectedAt),
    },
  ];

  return (
    <PageScaffold
      title="变更中心"
      description="记录余额、倍率、价格、模型、Key、采集和路由状态的变化。"
      actions={
        <Button variant="secondary" onClick={() => void refresh(true)} disabled={loading || saving}>
          <RefreshCw className="h-4 w-4" />
          刷新
        </Button>
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
                description="余额、Key、采集、价格、倍率、模型和路由状态变化会在这里形成记录。"
              />
            ) : (
              <DataTableLite
                columns={columns}
                rows={filteredEvents}
                getRowKey={(event) => event.id}
                selectedKey={selected?.id}
                onRowClick={(event) => setSelectedId(event.id)}
                className="rounded-none border-0 shadow-none"
              />
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
                <JsonBlock title="变化前" value={parseJsonObject(selected.oldValueJson)} />
                <JsonBlock title="变化后" value={parseJsonObject(selected.newValueJson)} />
                <JsonBlock title="影响" value={parseJsonObject(selected.impactJson)} />
                <div className="grid gap-2 text-xs text-muted-foreground">
                  <div>对象：{selected.objectType} / {selected.objectId ?? "-"}</div>
                  <div>来源：{selected.source}</div>
                  <div>Dedupe：{selected.dedupeKey}</div>
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
