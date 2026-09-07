import { Profiler, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { Copy, Edit3, Play, Plus, RefreshCw, Route, Trash2 } from "lucide-react";
import { Button, ConfirmDialog, EmptyState, IconButton, StatusBadge, useToast } from "@/components/ui";
import { PageScaffold } from "@/components/shell/PageScaffold";
import {
  createChannelMonitor,
  deleteChannelMonitor,
  runChannelMonitorNow,
  updateChannelMonitor,
} from "@/lib/api/channelMonitors";
import { readError } from "@/lib/errors";
import { invalidatePricingMonitoringQueries } from "@/lib/query/pricingMonitoringInvalidation";
import { queryKeys } from "@/lib/query/queryKeys";
import {
  channelMonitorLatestSummaryQueryOptions,
  channelMonitorTemplatesQueryOptions,
  channelMonitorsQueryOptions,
  keyPoolQueryOptions,
  monitoringCapabilitiesQueryOptions,
  stationsQueryOptions,
} from "@/lib/query/resourceQueries";
import { useActivityQuery } from "@/lib/query/useActivityQuery";
import type {
  ChannelMonitor,
  ChannelStatusLatestResult,
  ChannelStatusOutcome,
  CreateChannelMonitorInput,
} from "@/lib/types/channelMonitors";
import type { RoutingDeepLink } from "@/lib/types/routingDeepLinks";
import type { KeyPoolItem } from "@/lib/types/stationKeys";
import type { Station } from "@/lib/types/stations";
import { profileLabel, protocolLabel } from "@/lib/channelMonitorDisplay";
import { ChannelMonitorForm } from "./ChannelMonitorForm";
import { recordMonitoringPerformance } from "@/lib/monitoringPerformance";
import {
  formatInterval,
  formatTargetLabel,
  monitorToDraft,
  monitorToCreateInput,
  validateMonitorDraft,
} from "@/lib/channelMonitorViewModel";

type ChannelMonitoringTabProps = {
  headerActions?: ReactNode;
  onHealthChanged: () => void;
  onOpenRoutingDeepLink?: (link: MonitoringRoutingDeepLink) => void;
};

type MonitoringRoutingDeepLink = Extract<RoutingDeepLink, { kind: "station-key" }> & {
  source: "monitoring";
};

type ActionState = {
  monitorId: string;
  kind: "run" | "duplicate" | "delete";
} | null;

const monitorGridClassName =
  "w-full grid-cols-[minmax(0,0.9fr)_minmax(0,1.15fr)_minmax(0,1.15fr)_minmax(0,0.75fr)_minmax(0,0.75fr)] items-center gap-3";

export function ChannelMonitoringTab({
  headerActions,
  onHealthChanged,
  onOpenRoutingDeepLink,
}: ChannelMonitoringTabProps) {
  const toast = useToast();
  const queryClient = useQueryClient();
  const [saving, setSaving] = useState(false);
  const [actionState, setActionState] = useState<ActionState>(null);
  const [error, setError] = useState<string | null>(null);
  const [formOpen, setFormOpen] = useState(false);
  const [editingMonitor, setEditingMonitor] = useState<ChannelMonitor | null>(null);
  const [pendingDeleteMonitor, setPendingDeleteMonitor] = useState<ChannelMonitor | null>(null);
  const firstContentRecorded = useRef(false);
  // Definitions and labels are relatively static. Keep them in their own
  // cache entries so a five-second status refresh never re-reads the whole
  // management workspace. Mutations invalidate these canonical keys.
  const monitorsQuery = useActivityQuery(channelMonitorsQueryOptions());
  const latestSummaryQuery = useActivityQuery(channelMonitorLatestSummaryQueryOptions(5_000));
  const stationsQuery = useActivityQuery(stationsQueryOptions());
  const keyPoolQuery = useActivityQuery(keyPoolQueryOptions());
  const templatesQuery = useActivityQuery(channelMonitorTemplatesQueryOptions());
  const [, setFreshnessTick] = useState(0);
  const capabilitiesQuery = useActivityQuery(monitoringCapabilitiesQueryOptions(formOpen));
  const monitors = monitorsQuery.data ?? [];
  const stations = stationsQuery.data ?? [];
  const keys = keyPoolQuery.data ?? [];
  const templates = templatesQuery.data ?? [];
  const latestStatusByMonitor = useMemo(
    () => buildLatestStatusByMonitor(latestSummaryQuery.data ?? []),
    [latestSummaryQuery.data],
  );
  const loading = [monitorsQuery, stationsQuery, keyPoolQuery, templatesQuery].some(
    (query) => query.isPending && query.data === undefined,
  );
  const queryError = monitorsQuery.error
    ?? stationsQuery.error
    ?? keyPoolQuery.error
    ?? templatesQuery.error
    ?? latestSummaryQuery.error;
  const displayError = error ?? (queryError ? readError(queryError) : null);
  const capabilitiesError = capabilitiesQuery.error ? readError(capabilitiesQuery.error) : null;
  const hasSummary = latestSummaryQuery.data !== undefined;
  const stale = hasSummary && latestSummaryQuery.dataUpdatedAt > 0 && Date.now() - latestSummaryQuery.dataUpdatedAt > 15_000;

  useEffect(() => {
    if (!hasSummary || latestSummaryQuery.dataUpdatedAt <= 0) return;
    const delay = Math.max(0, latestSummaryQuery.dataUpdatedAt + 15_000 - Date.now());
    const timeout = window.setTimeout(() => setFreshnessTick((current) => current + 1), delay);
    return () => window.clearTimeout(timeout);
  }, [hasSummary, latestSummaryQuery.dataUpdatedAt]);

  const summary = useMemo(() => {
    const enabledCount = monitors.filter((monitor) => monitor.enabled).length;
    const balancePausedCount = monitors.filter((monitor) => monitor.enabled && monitor.balancePaused).length;
    const stationTargetCount = monitors.filter((monitor) => monitor.targetType === "station").length;
    const attentionCount = monitors.filter((monitor) => {
      const outcome = latestStatusByMonitor.get(monitor.id)?.outcome;
      return outcome === "degraded" || outcome === "unavailable" || outcome === "skipped";
    }).length;
    return {
      total: monitors.length,
      enabledCount,
      balancePausedCount,
      stationTargetCount,
      attentionCount,
    };
  }, [latestStatusByMonitor, monitors]);

  async function refresh(showSuccess = false) {
    setError(null);
    try {
      // Static labels are intentionally not polled every five seconds, but a
      // manual retry must also recover a failed station/key read. Successful
      // mutation refreshes still use the same canonical invalidation boundary.
      await Promise.all([
        invalidatePricingMonitoringQueries(queryClient),
        queryClient.invalidateQueries({ queryKey: queryKeys.stations }),
        queryClient.invalidateQueries({ queryKey: queryKeys.keyPool }),
      ]);
      if (showSuccess) {
        toast.success("渠道监控已刷新");
      }
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      toast.error("读取渠道监控失败", message);
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
    let capabilities = capabilitiesQuery.data;
    if (!capabilities) {
      try {
        // The capabilities query is intentionally disabled while the form is
        // closed. A duplicate still needs the same canonical validation, so
        // fetch it on demand instead of keeping an always-on subscription.
        capabilities = await queryClient.fetchQuery(monitoringCapabilitiesQueryOptions());
      } catch (requestError) {
        toast.error("复制监控失败", capabilitiesError ?? readError(requestError));
        return;
      }
    }
    const validationError = validateMonitorDraft(monitorToDraft(monitor), {
      templates,
      keys,
      capabilities,
    });
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

  function handleDelete(monitor: ChannelMonitor) {
    setPendingDeleteMonitor(monitor);
  }

  async function handleConfirmDelete() {
    if (!pendingDeleteMonitor) {
      return;
    }
    setActionState({ monitorId: pendingDeleteMonitor.id, kind: "delete" });
    setError(null);
    try {
      await deleteChannelMonitor(pendingDeleteMonitor.id);
      setPendingDeleteMonitor(null);
      await refresh();
      toast.success("监控已删除");
    } catch (requestError) {
      toast.error("删除监控失败", readError(requestError));
    } finally {
      setActionState(null);
    }
  }

  if (formOpen) {
    return (
      <ChannelMonitorForm
        monitor={editingMonitor}
        stations={stations}
        keys={keys}
        templates={templates}
        capabilities={capabilitiesQuery.data}
        capabilitiesError={capabilitiesError}
        saving={saving}
        onClose={closeForm}
        onRetryCapabilities={() => void capabilitiesQuery.refetch()}
        onSubmit={handleSave}
      />
    );
  }

  return (
    <Profiler
      id="channel-monitoring"
      onRender={(_id, phase, actualDuration) => {
        recordMonitoringPerformance({
          name: "channel-monitoring-react-commit",
          durationMs: actualDuration,
          phase,
        });
        if (phase === "mount" && !firstContentRecorded.current) {
          firstContentRecorded.current = true;
          recordMonitoringPerformance({
            name: "channel-monitoring-first-content",
            durationMs: actualDuration,
            phase,
          });
        }
      }}
    >
    <PageScaffold title="渠道监控" actions={headerActions}>
      {(latestSummaryQuery.isFetching || stale) && (
        <div className="mb-3 flex items-center justify-between gap-2 text-xs text-muted-foreground" role="status">
          <span>{latestSummaryQuery.isFetching ? "正在更新监控状态…" : "监控状态数据可能已过期"}</span>
          {stale && !latestSummaryQuery.isFetching ? (
            <button type="button" className="text-primary underline-offset-2 hover:underline" onClick={() => void latestSummaryQuery.refetch()}>
              立即刷新
            </button>
          ) : null}
        </div>
      )}
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="flex flex-wrap gap-2">
          <SummaryPill label="监控" value={`${summary.total}`} />
          <SummaryPill label="启用" value={`${summary.enabledCount}`} />
          {summary.balancePausedCount > 0 ? (
            <SummaryPill label="余额暂停" value={`${summary.balancePausedCount}`} />
          ) : null}
          <SummaryPill label="整站目标" value={`${summary.stationTargetCount}`} />
          <SummaryPill label="需关注" value={`${summary.attentionCount}`} tone={summary.attentionCount > 0 ? "warning" : "neutral"} />
        </div>
        <div className="flex flex-wrap justify-end gap-2">
          <Button variant="secondary" onClick={() => void refresh(true)} disabled={loading}>
            <RefreshCw className="h-4 w-4" />
            刷新
          </Button>
          <Button onClick={openCreate}>
            <Plus className="h-4 w-4" />
            新增监控
          </Button>
        </div>
      </div>

      {displayError && (
        <div className="flex items-center justify-between gap-3 rounded-[var(--surface-radius)] border border-danger-border bg-danger-surface px-3 py-2 text-sm text-danger-foreground">
          <span>{displayError}</span>
          <button
            type="button"
            className="shrink-0 text-danger-foreground underline-offset-2 hover:underline"
            onClick={() => void refresh()}
            disabled={latestSummaryQuery.isFetching}
          >
            重试
          </button>
        </div>
      )}

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
          latestStatusByMonitor={latestStatusByMonitor}
          monitors={monitors}
          stations={stations}
          onDelete={handleDelete}
          onDuplicate={handleDuplicate}
          onEdit={openEdit}
          onOpenRoutingDeepLink={onOpenRoutingDeepLink}
          onRunNow={handleRunNow}
        />
      )}

      <ConfirmDialog
        open={pendingDeleteMonitor !== null}
        title="删除渠道监控"
        description={`确定要删除监控 "${pendingDeleteMonitor?.name ?? ""}" 吗？此操作无法撤销。`}
        confirmLabel="删除"
        confirming={actionState?.kind === "delete"}
        onCancel={() => setPendingDeleteMonitor(null)}
        onConfirm={() => void handleConfirmDelete()}
      />
    </PageScaffold>
    </Profiler>
  );
}

function MonitorList({
  actionState,
  keys,
  latestStatusByMonitor,
  monitors,
  stations,
  onDelete,
  onDuplicate,
  onEdit,
  onOpenRoutingDeepLink,
  onRunNow,
}: {
  actionState: ActionState;
  keys: KeyPoolItem[];
  latestStatusByMonitor: Map<string, ChannelStatusLatestResult>;
  monitors: ChannelMonitor[];
  stations: Station[];
  onDelete: (monitor: ChannelMonitor) => void;
  onDuplicate: (monitor: ChannelMonitor) => void | Promise<void>;
  onEdit: (monitor: ChannelMonitor) => void;
  onOpenRoutingDeepLink?: (link: MonitoringRoutingDeepLink) => void;
  onRunNow: (monitor: ChannelMonitor) => void | Promise<void>;
}) {
  return (
    <div className="min-w-0 overflow-hidden">
      <div className={`hidden lg:grid ${monitorGridClassName} border-b border-border px-3 pb-2 text-[11px] font-medium text-muted-foreground`}>
        <TableHeadCell>监控</TableHeadCell>
        <TableHeadCell>目标</TableHeadCell>
        <TableHeadCell>主模型</TableHeadCell>
        <TableHeadCell align="center">调度</TableHeadCell>
        <div className="min-w-0 truncate text-right">操作</div>
      </div>
      <div className="space-y-2 lg:space-y-0 lg:divide-y lg:divide-border">
        {monitors.map((monitor) => (
          <MonitorRow
            key={monitor.id}
            actionState={actionState}
            keys={keys}
            monitor={monitor}
            latestStatus={latestStatusByMonitor.get(monitor.id) ?? null}
            stations={stations}
            onDelete={onDelete}
            onDuplicate={onDuplicate}
            onEdit={onEdit}
            onOpenRoutingDeepLink={onOpenRoutingDeepLink}
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
  latestStatus,
  stations,
  onDelete,
  onDuplicate,
  onEdit,
  onOpenRoutingDeepLink,
  onRunNow,
}: {
  actionState: ActionState;
  keys: KeyPoolItem[];
  monitor: ChannelMonitor;
  latestStatus: ChannelStatusLatestResult | null;
  stations: Station[];
  onDelete: (monitor: ChannelMonitor) => void;
  onDuplicate: (monitor: ChannelMonitor) => void | Promise<void>;
  onEdit: (monitor: ChannelMonitor) => void;
  onOpenRoutingDeepLink?: (link: MonitoringRoutingDeepLink) => void;
  onRunNow: (monitor: ChannelMonitor) => void | Promise<void>;
}) {
  const running = actionState?.monitorId === monitor.id && actionState.kind === "run";
  const duplicating = actionState?.monitorId === monitor.id && actionState.kind === "duplicate";
  const deleting = actionState?.monitorId === monitor.id && actionState.kind === "delete";
  const modelLabel = monitor.primaryModel.trim() || "未设置";
  const targetLabel = formatTargetLabel(monitor.targetType, monitor.stationId, monitor.stationKeyId, stations, keys);
  const intervalLabel = formatInterval(monitor.intervalSeconds, monitor.jitterSeconds);
  const primaryModelStatus = getPrimaryModelStatusView(latestStatus, modelLabel);
  const routingLink = createMonitoringRoutingLink(monitor);
  return (
    <>
      <div className={`hidden lg:grid ${monitorGridClassName} group min-h-[62px] px-3 py-2.5 text-left text-[13px] text-foreground transition-colors hover:bg-surface-subtle`}>
        <div className="min-w-0">
          <div className="truncate font-semibold text-foreground">{monitor.name}</div>
          {monitor.note && <div className="mt-0.5 truncate text-xs text-muted-foreground">{monitor.note}</div>}
        </div>

        <div className="min-w-0 truncate text-foreground">
          {targetLabel}
        </div>

        <PrimaryModelCell
          modelLabel={modelLabel}
          requestLabel={`${protocolLabel(monitor.protocolKind)} · ${profileLabel(monitor.clientProfileId)}`}
          status={primaryModelStatus}
        />

        <div className="flex min-w-0 flex-col items-center gap-1">
          <StatusBadge tone={monitor.enabled && !monitor.balancePaused ? "healthy" : "disabled"}>
            {!monitor.enabled ? "停用" : monitor.balancePaused ? "余额暂停" : "启用"}
          </StatusBadge>
          <div className="max-w-full truncate text-xs text-muted-foreground">
            {intervalLabel}
          </div>
        </div>

        <MonitorDesktopActions
          actionState={actionState}
          deleting={deleting}
          duplicating={duplicating}
          monitor={monitor}
          routingLink={routingLink}
          running={running}
          onDelete={onDelete}
          onDuplicate={onDuplicate}
          onEdit={onEdit}
          onOpenRoutingDeepLink={onOpenRoutingDeepLink}
          onRunNow={onRunNow}
        />
      </div>

      <section className="rounded-[var(--surface-radius)] border border-border bg-surface p-3.5 text-[13px] shadow-[var(--surface-shadow)] lg:hidden">
        <div className="space-y-3">
          <MonitorCardField label="监控" value={monitor.name} strong />
          <MonitorCardField label="目标" value={targetLabel} />
          <MonitorCardField label="主模型" value={modelLabel}>
            <StatusBadge tone={primaryModelStatus.tone} className="ml-2">
              {primaryModelStatus.label}
            </StatusBadge>
          </MonitorCardField>
          <MonitorCardField label="请求方式" value={`${protocolLabel(monitor.protocolKind)} · ${profileLabel(monitor.clientProfileId)}`} />
          <MonitorCardField label="调度" value={intervalLabel}>
            <StatusBadge tone={monitor.enabled && !monitor.balancePaused ? "healthy" : "disabled"} className="ml-2">
              {!monitor.enabled ? "停用" : monitor.balancePaused ? "余额暂停" : "启用"}
            </StatusBadge>
          </MonitorCardField>
        </div>

        <div className="mt-3 flex flex-wrap items-center gap-1.5 border-t border-border pt-3">
          <IconButton
            className={running ? "h-8 w-8 animate-pulse rounded-[7px] text-primary" : "h-8 w-8 rounded-[7px] text-muted-foreground hover:bg-selected hover:text-primary"}
            disabled={Boolean(actionState)}
            label={running ? `检测中 ${monitor.name}` : `立即检测 ${monitor.name}`}
            onClick={() => void onRunNow(monitor)}
          >
            <Play className="h-4 w-4" />
          </IconButton>
          <Button size="sm" variant="ghost" disabled={Boolean(actionState)} onClick={() => onEdit(monitor)}>
            <Edit3 className="h-3.5 w-3.5" />
            编辑
          </Button>
          <Button size="sm" variant="ghost" disabled={Boolean(actionState)} onClick={() => void onDuplicate(monitor)}>
            <Copy className="h-3.5 w-3.5" />
            {duplicating ? "复制中" : "复制"}
          </Button>
          {onOpenRoutingDeepLink && routingLink ? (
            <Button
              size="sm"
              variant="ghost"
              disabled={Boolean(actionState)}
              onClick={() => onOpenRoutingDeepLink(routingLink)}
            >
              <Route className="h-3.5 w-3.5" />
              路由影响
            </Button>
          ) : null}
          <Button size="sm" variant="ghost" className="text-danger-foreground hover:bg-danger-surface hover:text-danger-foreground" disabled={Boolean(actionState)} onClick={() => void onDelete(monitor)}>
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
  routingLink,
  running,
  onDelete,
  onDuplicate,
  onEdit,
  onOpenRoutingDeepLink,
  onRunNow,
}: {
  actionState: ActionState;
  deleting: boolean;
  duplicating: boolean;
  monitor: ChannelMonitor;
  routingLink: MonitoringRoutingDeepLink | null;
  running: boolean;
  onDelete: (monitor: ChannelMonitor) => void;
  onDuplicate: (monitor: ChannelMonitor) => void | Promise<void>;
  onEdit: (monitor: ChannelMonitor) => void;
  onOpenRoutingDeepLink?: (link: MonitoringRoutingDeepLink) => void;
  onRunNow: (monitor: ChannelMonitor) => void | Promise<void>;
}) {
  return (
    <div
      className="flex min-w-0 items-center justify-end gap-1 overflow-hidden"
      onClick={(event) => event.stopPropagation()}
      onKeyDown={(event) => event.stopPropagation()}
    >
      <IconButton
        className={running ? "h-7 w-7 shrink-0 animate-pulse rounded-[7px] text-primary" : "h-7 w-7 shrink-0 rounded-[7px] text-muted-foreground hover:bg-selected hover:text-primary"}
        disabled={Boolean(actionState)}
        label={running ? `检测中 ${monitor.name}` : `立即检测 ${monitor.name}`}
        onClick={() => void onRunNow(monitor)}
      >
        <Play className="h-4 w-4" />
      </IconButton>
      <IconButton className="h-7 w-7 shrink-0 rounded-[7px] text-muted-foreground hover:bg-muted hover:text-foreground" disabled={Boolean(actionState)} label={`编辑 ${monitor.name}`} onClick={() => onEdit(monitor)}>
        <Edit3 className="h-4 w-4" />
      </IconButton>
      <IconButton className="h-7 w-7 shrink-0 rounded-[7px] text-muted-foreground hover:bg-muted hover:text-foreground" disabled={Boolean(actionState)} label={duplicating ? `复制中 ${monitor.name}` : `复制 ${monitor.name}`} onClick={() => void onDuplicate(monitor)}>
        <Copy className="h-4 w-4" />
      </IconButton>
      {onOpenRoutingDeepLink && routingLink ? (
        <IconButton
          className="h-7 w-7 shrink-0 rounded-[7px] text-muted-foreground hover:bg-selected hover:text-primary"
          disabled={Boolean(actionState)}
          label={`查看路由影响 ${monitor.name}`}
          onClick={() => onOpenRoutingDeepLink(routingLink)}
        >
          <Route className="h-4 w-4" />
        </IconButton>
      ) : null}
      <IconButton className="h-7 w-7 shrink-0 rounded-[7px] text-muted-foreground hover:bg-danger-surface hover:text-danger-foreground" disabled={Boolean(actionState)} label={deleting ? `删除中 ${monitor.name}` : `删除 ${monitor.name}`} onClick={() => void onDelete(monitor)}>
        <Trash2 className="h-4 w-4" />
      </IconButton>
    </div>
  );
}

function PrimaryModelCell({
  modelLabel,
  requestLabel,
  status,
}: {
  modelLabel: string;
  requestLabel: string;
  status: PrimaryModelStatusView;
}) {
  return (
    <div className="min-w-0">
      <div className="flex min-w-0 items-center gap-2">
        <span className="min-w-0 truncate text-foreground">{modelLabel}</span>
        <StatusBadge tone={status.tone}>{status.label}</StatusBadge>
      </div>
      <div className="mt-0.5 truncate text-xs text-muted-foreground">{requestLabel}</div>
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
      <div className="text-xs leading-5 text-muted-foreground">{label}</div>
      <div className={`min-w-0 text-right leading-5 text-foreground ${strong ? "font-semibold text-foreground" : ""}`}>
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

type PrimaryModelStatusView = {
  label: "正常" | "欠佳" | "失败" | "未运行";
  tone: "healthy" | "warning" | "error" | "info";
};

function getPrimaryModelStatusView(
  latest: ChannelStatusLatestResult | null,
  primaryModel: string,
): PrimaryModelStatusView {
  if (!latest) {
    return { label: "未运行", tone: "info" };
  }
  if (latest.outcome === "unavailable" || latest.outcome === "skipped") {
    return { label: "失败", tone: "error" };
  }
  if (latest.outcome === "degraded" || latest.outcome === "missing") {
    return { label: "欠佳", tone: "warning" };
  }
  const normalizedPrimary = normalizeModelName(primaryModel);
  const effectiveModel = normalizeModelName(latest.effectiveModel);
  const usedDifferentModel = latest.usedFallback ||
    Boolean(effectiveModel && effectiveModel !== normalizedPrimary);
  return usedDifferentModel ? { label: "欠佳", tone: "warning" } : { label: "正常", tone: "healthy" };
}

function normalizeModelName(model: string | null) {
  return (model ?? "").trim().toLowerCase();
}

function createMonitoringRoutingLink(monitor: ChannelMonitor): MonitoringRoutingDeepLink | null {
  if (monitor.targetType !== "station_key" || !monitor.stationKeyId) {
    return null;
  }
  return {
    kind: "station-key",
    stationKeyId: monitor.stationKeyId,
    source: "monitoring",
  };
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
        ? "border-warning-border bg-warning-surface text-warning-foreground"
        : "border-border bg-surface text-muted-foreground"
    }`}>
      <span>{label}</span>
      <span className="text-sm font-semibold text-foreground">{value}</span>
    </span>
  );
}

function buildLatestStatusByMonitor(
  rows: Array<{
    monitor?: { id: string };
    monitorId?: string;
    latest: ChannelStatusLatestResult | null;
  }>,
) {
  const statuses = new Map<string, ChannelStatusLatestResult>();
  for (const row of rows) {
    if (!row.latest) {
      continue;
    }
    const monitorId = row.monitor?.id ?? row.monitorId;
    if (!monitorId) continue;
    const current = statuses.get(monitorId);
    if (!current || compareLatestStatus(row.latest, current) > 0) {
      statuses.set(monitorId, row.latest);
    }
  }
  return statuses;
}

function compareLatestStatus(left: ChannelStatusLatestResult, right: ChannelStatusLatestResult) {
  const severity = (outcome: ChannelStatusOutcome) => {
    switch (outcome) {
      case "unavailable": return 5;
      case "degraded": return 4;
      case "skipped": return 3;
      case "available": return 2;
      case "missing": return 1;
    }
  };
  const severityDifference = severity(left.outcome) - severity(right.outcome);
  if (severityDifference !== 0) {
    return severityDifference;
  }
  return (left.finishedAtMs ?? 0) - (right.finishedAtMs ?? 0);
}
