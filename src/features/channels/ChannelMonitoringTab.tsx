import { useEffect, useMemo, useState, type ReactNode } from "react";
import { Copy, Edit3, LayoutTemplate, Play, Plus, RefreshCw, Trash2 } from "lucide-react";
import { Button, EmptyState, IconButton, StatusBadge, useToast } from "@/components/ui";
import {
  createChannelMonitor,
  deleteChannelMonitor,
  listChannelMonitorSummaries,
  listChannelMonitorTemplates,
  runChannelMonitorNow,
  updateChannelMonitor,
} from "@/lib/api/channelMonitors";
import { readError } from "@/lib/errors";
import { listKeyPoolItems } from "@/lib/api/stationKeys";
import { listStations } from "@/lib/api/stations";
import type { ChannelMonitor, ChannelMonitorRequestTemplate, ChannelMonitorRun, CreateChannelMonitorInput } from "@/lib/types/channelMonitors";
import type { KeyPoolItem } from "@/lib/types/stationKeys";
import type { Station } from "@/lib/types/stations";
import { ChannelMonitorForm } from "./ChannelMonitorForm";
import { ChannelMonitorTemplateManager } from "./ChannelMonitorTemplateManager";
import {
  formatInterval,
  formatRunTimestamp,
  formatTargetLabel,
  formatTemplateLabel,
  getRunStatusView,
  monitorToDraft,
  monitorToCreateInput,
  validateMonitorDraft,
} from "./channelMonitorViewModel";

type ChannelMonitoringTabProps = {
  onHealthChanged: () => void;
};

type ActionState = {
  monitorId: string;
  kind: "run" | "duplicate" | "delete";
} | null;

const monitorGridClassName =
  "w-full grid-cols-[minmax(0,0.9fr)_minmax(0,1.1fr)_minmax(0,1.15fr)_minmax(0,0.75fr)_minmax(0,1fr)_minmax(0,0.5fr)] items-center gap-3";

export function ChannelMonitoringTab({ onHealthChanged }: ChannelMonitoringTabProps) {
  const toast = useToast();
  const [monitors, setMonitors] = useState<ChannelMonitor[]>([]);
  const [stations, setStations] = useState<Station[]>([]);
  const [keys, setKeys] = useState<KeyPoolItem[]>([]);
  const [templates, setTemplates] = useState<ChannelMonitorRequestTemplate[]>([]);
  const [runsByMonitor, setRunsByMonitor] = useState(new Map<string, ChannelMonitorRun[]>());
  const [runLoadFailedIds, setRunLoadFailedIds] = useState(new Set<string>());
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [actionState, setActionState] = useState<ActionState>(null);
  const [error, setError] = useState<string | null>(null);
  const [formOpen, setFormOpen] = useState(false);
  const [templateManagerOpen, setTemplateManagerOpen] = useState(false);
  const [editingMonitor, setEditingMonitor] = useState<ChannelMonitor | null>(null);

  useEffect(() => {
    void refresh();
  }, []);

  const templateById = useMemo(
    () => new Map(templates.map((template) => [template.id, template] as const)),
    [templates],
  );
  const summary = useMemo(() => {
    const enabledCount = monitors.filter((monitor) => monitor.enabled).length;
    const stationTargetCount = monitors.filter((monitor) => monitor.targetType === "station").length;
    const latestRuns = monitors
      .map((monitor) => getLatestRun(runsByMonitor.get(monitor.id) ?? []))
      .filter((run): run is ChannelMonitorRun => Boolean(run));
    const attentionCount = latestRuns.filter((run) => run.status === "warning" || run.status === "failed").length;
    return {
      total: monitors.length,
      enabledCount,
      stationTargetCount,
      attentionCount,
    };
  }, [monitors, runsByMonitor]);

  async function refresh(showSuccess = false) {
    setLoading(true);
    setError(null);
    try {
      const [summaries, nextStations, nextKeys, nextTemplates] = await Promise.all([
        listChannelMonitorSummaries(),
        listStations(),
        listKeyPoolItems(),
        listChannelMonitorTemplates(),
      ]);
      const nextMonitors = summaries.map((summary) => summary.monitor);
      setMonitors(nextMonitors);
      setStations(nextStations);
      setKeys(nextKeys);
      setTemplates(nextTemplates);
      setRunsByMonitor(new Map(summaries.map((summary) => [summary.monitor.id, summary.recentRuns] as const)));
      setRunLoadFailedIds(new Set(summaries.filter((summary) => summary.runsLoadStatus === "failed").map((summary) => summary.monitor.id)));
      if (showSuccess) {
        toast.success("渠道监控已刷新");
      }
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      toast.error("读取渠道监控失败", message);
    } finally {
      setLoading(false);
    }
  }

  function openCreate() {
    setEditingMonitor(null);
    setFormOpen(true);
  }

  function openEdit(monitor: ChannelMonitor) {
    setEditingMonitor(monitor);
    setFormOpen(true);
  }

  function closeForm() {
    if (saving) {
      return;
    }
    setFormOpen(false);
    setEditingMonitor(null);
  }

  async function handleSave(input: CreateChannelMonitorInput) {
    setSaving(true);
    setError(null);
    try {
      if (editingMonitor) {
        await updateChannelMonitor({ ...input, id: editingMonitor.id });
        toast.success("监控已更新");
      } else {
        await createChannelMonitor(input);
        toast.success("监控已创建");
      }
      setFormOpen(false);
      setEditingMonitor(null);
      await refresh();
    } catch (requestError) {
      toast.error("保存监控失败", readError(requestError));
    } finally {
      setSaving(false);
    }
  }

  async function handleRunNow(monitor: ChannelMonitor) {
    setActionState({ monitorId: monitor.id, kind: "run" });
    setError(null);
    try {
      await runChannelMonitorNow(monitor.id);
      await refresh();
      onHealthChanged();
      toast.success("监控已运行");
    } catch (requestError) {
      toast.error("运行监控失败", readError(requestError));
    } finally {
      setActionState(null);
    }
  }

  async function handleDuplicate(monitor: ChannelMonitor) {
    const validationError = validateMonitorDraft(monitorToDraft(monitor), { templates, keys });
    if (validationError) {
      toast.error("复制监控失败", validationError);
      return;
    }
    setActionState({ monitorId: monitor.id, kind: "duplicate" });
    setError(null);
    try {
      await createChannelMonitor(monitorToCreateInput(monitor));
      await refresh();
      toast.success("监控已复制");
    } catch (requestError) {
      toast.error("复制监控失败", readError(requestError));
    } finally {
      setActionState(null);
    }
  }

  async function handleDelete(monitor: ChannelMonitor) {
    if (!window.confirm(`确认删除监控「${monitor.name}」？`)) {
      return;
    }
    setActionState({ monitorId: monitor.id, kind: "delete" });
    setError(null);
    try {
      await deleteChannelMonitor(monitor.id);
      await refresh();
      toast.success("监控已删除");
    } catch (requestError) {
      toast.error("删除监控失败", readError(requestError));
    } finally {
      setActionState(null);
    }
  }

  return (
    <>
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="flex flex-wrap gap-2">
          <SummaryPill label="监控" value={`${summary.total}`} />
          <SummaryPill label="启用" value={`${summary.enabledCount}`} />
          <SummaryPill label="整站目标" value={`${summary.stationTargetCount}`} />
          <SummaryPill label="需关注" value={`${summary.attentionCount}`} tone={summary.attentionCount > 0 ? "warning" : "neutral"} />
        </div>
        <div className="flex flex-wrap justify-end gap-2">
          <Button variant="secondary" onClick={() => void refresh(true)} disabled={loading}>
            <RefreshCw className="h-4 w-4" />
            刷新
          </Button>
          <Button variant="outline" onClick={() => setTemplateManagerOpen(true)}>
            <LayoutTemplate className="h-4 w-4" />
            模板管理
          </Button>
          <Button onClick={openCreate}>
            <Plus className="h-4 w-4" />
            新增监控
          </Button>
        </div>
      </div>

      {error && <div className="rounded-[var(--surface-radius)] border border-rose-100 bg-rose-50 px-3 py-2 text-sm text-rose-700">{error}</div>}

      {monitors.length === 0 ? (
        <EmptyState
          title={loading ? "正在读取渠道监控" : "暂无渠道监控"}
          description="新增监控后可手动运行探测任务。"
          action={
            !loading && (
              <Button onClick={openCreate}>
                <Plus className="h-4 w-4" />
                新增监控
              </Button>
            )
          }
        />
      ) : (
        <MonitorList
          actionState={actionState}
          keys={keys}
          monitors={monitors}
          runLoadFailedIds={runLoadFailedIds}
          runsByMonitor={runsByMonitor}
          stations={stations}
          templateById={templateById}
          onDelete={handleDelete}
          onDuplicate={handleDuplicate}
          onEdit={openEdit}
          onRunNow={handleRunNow}
        />
      )}

      <ChannelMonitorForm
        open={formOpen}
        monitor={editingMonitor}
        stations={stations}
        keys={keys}
        templates={templates}
        saving={saving}
        onClose={closeForm}
        onSubmit={handleSave}
      />
      <ChannelMonitorTemplateManager
        open={templateManagerOpen}
        templates={templates}
        onClose={() => setTemplateManagerOpen(false)}
        onChanged={() => refresh()}
      />
    </>
  );
}

function MonitorList({
  actionState,
  keys,
  monitors,
  runLoadFailedIds,
  runsByMonitor,
  stations,
  templateById,
  onDelete,
  onDuplicate,
  onEdit,
  onRunNow,
}: {
  actionState: ActionState;
  keys: KeyPoolItem[];
  monitors: ChannelMonitor[];
  runLoadFailedIds: Set<string>;
  runsByMonitor: Map<string, ChannelMonitorRun[]>;
  stations: Station[];
  templateById: Map<string, ChannelMonitorRequestTemplate>;
  onDelete: (monitor: ChannelMonitor) => void;
  onDuplicate: (monitor: ChannelMonitor) => void | Promise<void>;
  onEdit: (monitor: ChannelMonitor) => void;
  onRunNow: (monitor: ChannelMonitor) => void | Promise<void>;
}) {
  return (
    <div className="min-w-0 overflow-hidden">
      <div className={`hidden lg:grid ${monitorGridClassName} border-b border-slate-200 px-3 pb-2 text-[11px] font-medium text-slate-500`}>
        <TableHeadCell>监控</TableHeadCell>
        <TableHeadCell>目标</TableHeadCell>
        <TableHeadCell>测试模板</TableHeadCell>
        <TableHeadCell align="center">调度</TableHeadCell>
        <TableHeadCell>健康</TableHeadCell>
        <div className="min-w-0 truncate text-right">操作</div>
      </div>
      <div className="space-y-2 lg:space-y-0 lg:divide-y lg:divide-slate-100">
        {monitors.map((monitor) => (
          <MonitorRow
            key={monitor.id}
            actionState={actionState}
            keys={keys}
            monitor={monitor}
            runLoadFailed={runLoadFailedIds.has(monitor.id)}
            runs={runsByMonitor.get(monitor.id) ?? []}
            stations={stations}
            template={templateById.get(monitor.templateId)}
            onDelete={onDelete}
            onDuplicate={onDuplicate}
            onEdit={onEdit}
            onRunNow={onRunNow}
          />
        ))}
      </div>
    </div>
  );
}

function MonitorRow({
  actionState,
  keys,
  monitor,
  runLoadFailed,
  runs,
  stations,
  template,
  onDelete,
  onDuplicate,
  onEdit,
  onRunNow,
}: {
  actionState: ActionState;
  keys: KeyPoolItem[];
  monitor: ChannelMonitor;
  runLoadFailed: boolean;
  runs: ChannelMonitorRun[];
  stations: Station[];
  template?: ChannelMonitorRequestTemplate;
  onDelete: (monitor: ChannelMonitor) => void;
  onDuplicate: (monitor: ChannelMonitor) => void | Promise<void>;
  onEdit: (monitor: ChannelMonitor) => void;
  onRunNow: (monitor: ChannelMonitor) => void | Promise<void>;
}) {
  const running = actionState?.monitorId === monitor.id && actionState.kind === "run";
  const duplicating = actionState?.monitorId === monitor.id && actionState.kind === "duplicate";
  const deleting = actionState?.monitorId === monitor.id && actionState.kind === "delete";
  const modelLabel = monitor.fallbackModels[0] ?? "";
  const targetLabel = formatTargetLabel(monitor.targetType, monitor.stationId, monitor.stationKeyId, stations, keys);
  const templateLabel = formatTemplateLabel(template);
  const intervalLabel = formatInterval(monitor.intervalSeconds, monitor.jitterSeconds);
  const latestRun = getLatestRun(runs);
  return (
    <>
      <div className={`hidden lg:grid ${monitorGridClassName} group min-h-[62px] px-3 py-2.5 text-left text-[13px] text-slate-700 transition-colors hover:bg-slate-50/45`}>
        <div className="min-w-0">
          <div className="truncate font-semibold text-slate-900">{monitor.name}</div>
          {monitor.note && <div className="mt-0.5 truncate text-xs text-muted-foreground">{monitor.note}</div>}
        </div>

        <div className="min-w-0 truncate text-slate-700">
          {targetLabel}
        </div>

        <div className="min-w-0">
          <div className="truncate text-slate-800">{templateLabel}</div>
          {modelLabel && <div className="mt-0.5 truncate text-xs text-muted-foreground">{modelLabel}</div>}
        </div>

        <div className="flex min-w-0 flex-col items-center gap-1">
          <StatusBadge tone={monitor.enabled ? "healthy" : "disabled"}>
            {monitor.enabled ? "启用" : "停用"}
          </StatusBadge>
          <div className="max-w-full truncate text-xs text-muted-foreground">
            {intervalLabel}
          </div>
        </div>

        <LatestRunCell run={latestRun} historyFailed={runLoadFailed} />

        <MonitorDesktopActions
          actionState={actionState}
          deleting={deleting}
          duplicating={duplicating}
          monitor={monitor}
          running={running}
          onDelete={onDelete}
          onDuplicate={onDuplicate}
          onEdit={onEdit}
          onRunNow={onRunNow}
        />
      </div>

      <section className="rounded-[var(--surface-radius)] border border-border bg-white p-3.5 text-[13px] shadow-[var(--surface-shadow)] lg:hidden">
        <div className="space-y-3">
          <MonitorCardField label="监控" value={monitor.name} strong />
          <MonitorCardField label="目标" value={targetLabel} />
          <MonitorCardField label="测试模板" value={templateLabel}>
            {modelLabel && <span className="ml-2 text-xs text-muted-foreground">{modelLabel}</span>}
          </MonitorCardField>
          <MonitorCardField label="调度" value={intervalLabel}>
            <StatusBadge tone={monitor.enabled ? "healthy" : "disabled"} className="ml-2">
              {monitor.enabled ? "启用" : "停用"}
            </StatusBadge>
          </MonitorCardField>
          <MonitorCardField label="健康">
            <LatestRunCell run={latestRun} historyFailed={runLoadFailed} align="end" />
          </MonitorCardField>
        </div>

        <div className="mt-3 flex flex-wrap items-center gap-1.5 border-t border-slate-100 pt-3">
          <Button size="sm" variant="ghost" disabled={Boolean(actionState)} onClick={() => void onRunNow(monitor)}>
            <Play className="h-3.5 w-3.5" />
            {running ? "运行中" : "立即检测"}
          </Button>
          <Button size="sm" variant="ghost" disabled={Boolean(actionState)} onClick={() => onEdit(monitor)}>
            <Edit3 className="h-3.5 w-3.5" />
            编辑
          </Button>
          <Button size="sm" variant="ghost" disabled={Boolean(actionState)} onClick={() => void onDuplicate(monitor)}>
            <Copy className="h-3.5 w-3.5" />
            {duplicating ? "复制中" : "复制"}
          </Button>
          <Button size="sm" variant="ghost" className="text-rose-600 hover:bg-rose-50 hover:text-rose-600" disabled={Boolean(actionState)} onClick={() => void onDelete(monitor)}>
            <Trash2 className="h-3.5 w-3.5" />
            {deleting ? "删除中" : "删除"}
          </Button>
        </div>
      </section>
    </>
  );
}

function MonitorDesktopActions({
  actionState,
  deleting,
  duplicating,
  monitor,
  running,
  onDelete,
  onDuplicate,
  onEdit,
  onRunNow,
}: {
  actionState: ActionState;
  deleting: boolean;
  duplicating: boolean;
  monitor: ChannelMonitor;
  running: boolean;
  onDelete: (monitor: ChannelMonitor) => void;
  onDuplicate: (monitor: ChannelMonitor) => void | Promise<void>;
  onEdit: (monitor: ChannelMonitor) => void;
  onRunNow: (monitor: ChannelMonitor) => void | Promise<void>;
}) {
  return (
    <div
      className="flex min-w-0 items-center justify-end gap-1 overflow-hidden lg:opacity-0 lg:transition-opacity lg:group-hover:opacity-100 lg:group-focus-within:opacity-100"
      onClick={(event) => event.stopPropagation()}
      onKeyDown={(event) => event.stopPropagation()}
    >
      <IconButton
        className={running ? "h-7 w-7 shrink-0 animate-pulse rounded-[7px] text-teal-700" : "h-7 w-7 shrink-0 rounded-[7px] text-slate-500 hover:bg-teal-50 hover:text-teal-700"}
        disabled={Boolean(actionState)}
        label={running ? `运行中 ${monitor.name}` : `运行 ${monitor.name}`}
        onClick={() => void onRunNow(monitor)}
      >
        <Play className="h-4 w-4" />
      </IconButton>
      <IconButton className="h-7 w-7 shrink-0 rounded-[7px] text-slate-500 hover:bg-slate-100 hover:text-slate-800" disabled={Boolean(actionState)} label={`编辑 ${monitor.name}`} onClick={() => onEdit(monitor)}>
        <Edit3 className="h-4 w-4" />
      </IconButton>
      <IconButton className="h-7 w-7 shrink-0 rounded-[7px] text-slate-500 hover:bg-slate-100 hover:text-slate-800" disabled={Boolean(actionState)} label={duplicating ? `复制中 ${monitor.name}` : `复制 ${monitor.name}`} onClick={() => void onDuplicate(monitor)}>
        <Copy className="h-4 w-4" />
      </IconButton>
      <IconButton className="h-7 w-7 shrink-0 rounded-[7px] text-slate-500 hover:bg-rose-50 hover:text-rose-600" disabled={Boolean(actionState)} label={deleting ? `删除中 ${monitor.name}` : `删除 ${monitor.name}`} onClick={() => void onDelete(monitor)}>
        <Trash2 className="h-4 w-4" />
      </IconButton>
    </div>
  );
}

function MonitorCardField({
  children,
  label,
  strong = false,
  value,
}: {
  children?: ReactNode;
  label: string;
  strong?: boolean;
  value?: string;
}) {
  return (
    <div className="grid grid-cols-[5.5rem_minmax(0,1fr)] items-start gap-3">
      <div className="text-xs leading-5 text-slate-500">{label}</div>
      <div className={`min-w-0 text-right leading-5 text-slate-800 ${strong ? "font-semibold text-slate-950" : ""}`}>
        {value && <span className="break-words">{value}</span>}
        {children}
      </div>
    </div>
  );
}

function TableHeadCell({
  align = "start",
  children,
}: {
  align?: "start" | "center";
  children: string;
}) {
  return (
    <div className={`min-w-0 truncate ${align === "center" ? "text-center" : ""}`}>
      {children}
    </div>
  );
}

function LatestRunCell({
  align = "start",
  run,
  historyFailed,
}: {
  align?: "end" | "start";
  run: ChannelMonitorRun | null;
  historyFailed: boolean;
}) {
  const alignClassName = align === "end" ? "justify-end text-right" : "";
  if (historyFailed) {
    return (
      <div className={`flex min-w-0 items-center gap-2 ${alignClassName}`}>
        <StatusBadge tone="warning">未读取</StatusBadge>
        <span className="truncate text-xs text-muted-foreground">运行历史读取失败</span>
      </div>
    );
  }

  const statusView = getRunStatusView(run?.status ?? null);
  const durationLabel = formatRunDuration(run);
  return (
    <div className={`flex min-w-0 items-center gap-2 ${alignClassName}`}>
      <StatusBadge tone={statusView.tone}>{statusView.label}</StatusBadge>
      <span className="truncate text-xs text-muted-foreground">
        {run ? `${formatRunTimestamp(run.startedAt)} · ${durationLabel}` : "未运行"}
      </span>
    </div>
  );
}

function formatRunDuration(run: ChannelMonitorRun | null) {
  const durationMs = run?.latencyMs ?? run?.durationMs;
  return typeof durationMs === "number" ? `${durationMs}ms` : "--";
}

function SummaryPill({
  label,
  value,
  tone = "neutral",
}: {
  label: string;
  value: string;
  tone?: "neutral" | "warning";
}) {
  return (
    <span className={`inline-flex h-8 items-center gap-1.5 rounded-[8px] border px-2.5 text-xs font-medium ${
      tone === "warning"
        ? "border-amber-200 bg-amber-50 text-amber-700"
        : "border-border bg-white text-slate-600"
    }`}>
      <span>{label}</span>
      <span className="text-sm font-semibold text-slate-800">{value}</span>
    </span>
  );
}

function getLatestRun(runs: ChannelMonitorRun[]) {
  return [...runs].sort((a, b) => toTime(b.startedAt) - toTime(a.startedAt))[0] ?? null;
}

function toTime(value: string) {
  const numeric = Number(value);
  const date = Number.isFinite(numeric) && numeric > 1000000000000 ? new Date(numeric) : new Date(value);
  return date.getTime();
}

