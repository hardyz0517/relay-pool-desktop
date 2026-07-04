import { useEffect, useMemo, useState } from "react";
import { Copy, Edit3, LayoutTemplate, Play, Plus, RefreshCw, Trash2 } from "lucide-react";
import { Button, DataTableLite, EmptyState, StatusBadge, useToast, type DataTableColumn } from "@/components/ui";
import {
  createChannelMonitor,
  deleteChannelMonitor,
  listChannelMonitorRuns,
  listChannelMonitorTemplates,
  listChannelMonitors,
  runChannelMonitorNow,
  updateChannelMonitor,
} from "@/lib/api/channelMonitors";
import { listKeyPoolItems } from "@/lib/api/stationKeys";
import { listStations } from "@/lib/api/stations";
import type { ChannelMonitor, ChannelMonitorRequestTemplate, ChannelMonitorRun, CreateChannelMonitorInput } from "@/lib/types/channelMonitors";
import type { KeyPoolItem } from "@/lib/types/stationKeys";
import type { Station } from "@/lib/types/stations";
import { ChannelMonitorForm } from "./ChannelMonitorForm";
import {
  formatInterval,
  formatRunTimestamp,
  formatTargetLabel,
  formatTemplateLabel,
  getRunStatusView,
  monitorToCreateInput,
} from "./channelMonitorViewModel";

type ChannelMonitoringTabProps = {
  onHealthChanged: () => void;
};

type ActionState = {
  monitorId: string;
  kind: "run" | "duplicate" | "delete";
} | null;

export function ChannelMonitoringTab({ onHealthChanged }: ChannelMonitoringTabProps) {
  const toast = useToast();
  const [monitors, setMonitors] = useState<ChannelMonitor[]>([]);
  const [stations, setStations] = useState<Station[]>([]);
  const [keys, setKeys] = useState<KeyPoolItem[]>([]);
  const [templates, setTemplates] = useState<ChannelMonitorRequestTemplate[]>([]);
  const [runsByMonitor, setRunsByMonitor] = useState(new Map<string, ChannelMonitorRun[]>());
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [actionState, setActionState] = useState<ActionState>(null);
  const [error, setError] = useState<string | null>(null);
  const [formOpen, setFormOpen] = useState(false);
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
      const [nextMonitors, nextStations, nextKeys, nextTemplates] = await Promise.all([
        listChannelMonitors(),
        listStations(),
        listKeyPoolItems(),
        listChannelMonitorTemplates(),
      ]);
      const runEntries = await Promise.all(
        nextMonitors.map(async (monitor) => {
          try {
            return [monitor.id, await listChannelMonitorRuns(monitor.id)] as const;
          } catch {
            return [monitor.id, [] as ChannelMonitorRun[]] as const;
          }
        }),
      );
      setMonitors(nextMonitors);
      setStations(nextStations);
      setKeys(nextKeys);
      setTemplates(nextTemplates);
      setRunsByMonitor(new Map(runEntries));
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

  const columns: DataTableColumn<ChannelMonitor>[] = [
    {
      key: "name",
      header: "监控",
      className: "min-w-[180px]",
      render: (monitor) => (
        <div className="min-w-0">
          <div className="truncate font-medium text-slate-800">{monitor.name}</div>
          <div className="truncate text-[11px] text-muted-foreground">{monitor.note || "无备注"}</div>
        </div>
      ),
    },
    {
      key: "target",
      header: "目标",
      className: "min-w-[190px]",
      render: (monitor) => (
        <span className="block max-w-[260px] truncate">
          {formatTargetLabel(monitor.targetType, monitor.stationId, monitor.stationKeyId, stations, keys)}
        </span>
      ),
    },
    {
      key: "template",
      header: "模板",
      className: "min-w-[160px]",
      render: (monitor) => (
        <span className="block max-w-[220px] truncate">{formatTemplateLabel(templateById.get(monitor.templateId))}</span>
      ),
    },
    {
      key: "model",
      header: "模型",
      className: "min-w-[130px]",
      render: (monitor) => <span className="block max-w-[180px] truncate">{monitor.fallbackModels[0] ?? "-"}</span>,
    },
    {
      key: "interval",
      header: "频率",
      className: "min-w-[130px]",
      render: (monitor) => formatInterval(monitor.intervalSeconds, monitor.jitterSeconds),
    },
    {
      key: "enabled",
      header: "状态",
      render: (monitor) => (
        <StatusBadge tone={monitor.enabled ? "healthy" : "disabled"}>
          {monitor.enabled ? "启用" : "停用"}
        </StatusBadge>
      ),
    },
    {
      key: "latest",
      header: "最近结果",
      className: "min-w-[150px]",
      render: (monitor) => <LatestRunCell run={getLatestRun(runsByMonitor.get(monitor.id) ?? [])} />,
    },
    {
      key: "actions",
      header: "操作",
      className: "min-w-[230px] text-right",
      render: (monitor) => {
        const running = actionState?.monitorId === monitor.id && actionState.kind === "run";
        const duplicating = actionState?.monitorId === monitor.id && actionState.kind === "duplicate";
        const deleting = actionState?.monitorId === monitor.id && actionState.kind === "delete";
        return (
          <div className="flex justify-end gap-1.5">
            <Button size="sm" variant="outline" disabled={Boolean(actionState)} onClick={() => void handleRunNow(monitor)}>
              <Play className="h-3.5 w-3.5" />
              {running ? "运行中" : "运行"}
            </Button>
            <Button size="sm" variant="ghost" disabled={Boolean(actionState)} onClick={() => openEdit(monitor)}>
              <Edit3 className="h-3.5 w-3.5" />
              编辑
            </Button>
            <Button size="sm" variant="ghost" disabled={Boolean(actionState)} onClick={() => void handleDuplicate(monitor)}>
              <Copy className="h-3.5 w-3.5" />
              {duplicating ? "复制中" : "复制"}
            </Button>
            <Button size="sm" variant="danger" disabled={Boolean(actionState)} onClick={() => void handleDelete(monitor)}>
              <Trash2 className="h-3.5 w-3.5" />
              {deleting ? "删除中" : "删除"}
            </Button>
          </div>
        );
      },
    },
  ];

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
          <Button variant="outline" disabled title="模板管理将在 Task 7 接入">
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
        <DataTableLite rows={monitors} columns={columns} getRowKey={(monitor) => monitor.id} />
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
    </>
  );
}

function LatestRunCell({ run }: { run: ChannelMonitorRun | null }) {
  const statusView = getRunStatusView(run?.status ?? null);
  return (
    <div className="flex min-w-0 items-center gap-2">
      <StatusBadge tone={statusView.tone}>{statusView.label}</StatusBadge>
      <span className="truncate text-xs text-muted-foreground">
        {run ? `${formatRunTimestamp(run.startedAt)} · ${run.latencyMs ?? run.durationMs ?? "-"}ms` : "未运行"}
      </span>
    </div>
  );
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

function readError(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}
