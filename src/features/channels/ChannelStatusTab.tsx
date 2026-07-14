import { useEffect, useMemo, useState, type CSSProperties } from "react";
import {
  closestCenter,
  type DraggableAttributes,
  DndContext,
  KeyboardSensor,
  PointerSensor,
  type DragEndEvent,
  useSensor,
  useSensors,
} from "@dnd-kit/core";
import {
  SortableContext,
  rectSortingStrategy,
  sortableKeyboardCoordinates,
  useSortable,
} from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import type { LucideIcon } from "lucide-react";
import { Radio, RefreshCw, Server, Timer } from "lucide-react";
import {
  usePageActivation,
  usePageRefreshEnabled,
} from "@/components/shell/PageActivity";
import { Button, EmptyState, SegmentedControl, StatusBadge, useToast } from "@/components/ui";
import { readError } from "@/lib/errors";
import { loadChannelStatusWorkspace } from "@/lib/queries/channelQueries";
import { channelStatusQueryOptions } from "@/lib/query/resourceQueries";
import { useActivityQuery } from "@/lib/query/useActivityQuery";
import { parseTimestampLikeDate, toTimestampMillis } from "@/lib/time";
import type { ChannelMonitor, ChannelStatusSummary, ChannelStatusTimelinePoint } from "@/lib/types/channelMonitors";
import type { RequestLog } from "@/lib/types/proxy";
import type { StationKeyHealth } from "@/lib/types/routing";
import type { KeyPoolItem, StationKeyStatus } from "@/lib/types/stationKeys";
import { stationTypeLabels } from "@/lib/types/stations";
import { cn } from "@/lib/utils";
import {
  availabilityTone,
  type AvailabilityTone,
  buildMonitorTimelineOutcomes,
  buildRecentOutcomes,
  type ChannelWindow,
  enabledStationKeyMonitorsByKey,
  monitorRunToStationKeyStatus,
  orderChannelsBySavedOrder,
  resolveChannelLatencyMetrics,
  selectChannelStatusWindowSummary,
  type RecentOutcome,
} from "./channelStatusViewModel";

const availabilityToneClassName: Record<AvailabilityTone, string> = {
  muted: "text-muted-foreground",
  danger: "text-danger-foreground",
  warning: "text-warning-foreground",
  success: "text-success-foreground",
};

type ChannelHealth = {
  id: string;
  keyName: string;
  stationName: string;
  stationType: string;
  modelSummary: string;
  status: StationKeyStatus;
  latencyMs: number | null;
  endpointPingMs: number | null;
  availabilityPercent: number | null;
  lastCheckedAt: string | null;
  lastUsedAt: string | null;
  lastError: string | null;
  successCount: number;
  failureCount: number;
  consecutiveFailures: number;
  cooldownUntil: string | null;
  recentOutcomes: RecentOutcome[];
  availabilityLabel: string;
};

const statusTone: Record<StationKeyStatus, "healthy" | "warning" | "error" | "disabled" | "info"> = {
  healthy: "healthy",
  warning: "warning",
  error: "error",
  disabled: "disabled",
  unchecked: "info",
};

const statusLabel: Record<StationKeyStatus, string> = {
  healthy: "健康",
  warning: "警告",
  error: "错误",
  disabled: "禁用",
  unchecked: "未检查",
};

const outcomeClassName: Record<RecentOutcome, string> = {
  success: "bg-success-foreground",
  warning: "bg-warning-foreground",
  failed: "bg-danger-solid",
  unknown: "bg-muted-foreground/45",
};

export function ChannelStatusTab({ refreshToken }: { refreshToken: number }) {
  const toast = useToast();
  const refreshEnabled = usePageRefreshEnabled();
  useActivityQuery(refreshEnabled, channelStatusQueryOptions());
  const [keys, setKeys] = useState<KeyPoolItem[]>([]);
  const [logs, setLogs] = useState<RequestLog[]>([]);
  const [health, setHealth] = useState<StationKeyHealth[]>([]);
  const [monitors, setMonitors] = useState<ChannelMonitor[]>([]);
  const [statusSummaries, setStatusSummaries] = useState<ChannelStatusSummary[]>([]);
  const [channelOrder, setChannelOrder] = useState<string[]>([]);
  const [timeWindow, setTimeWindow] = useState<ChannelWindow>("recent");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  usePageActivation(({ isInitial }) => {
    void refresh(false, isInitial);
  });

  useEffect(() => {
    if (refreshToken > 0) {
      void refresh();
    }
  }, [refreshToken]);

  const visibleLogs = useMemo(() => filterLogsByWindow(logs, timeWindow), [logs, timeWindow]);
  const channels = useMemo(
    () =>
      orderChannelsBySavedOrder(
        buildChannels(keys, visibleLogs, health, monitors, statusSummaries, timeWindow),
        channelOrder,
      ),
    [channelOrder, health, keys, monitors, statusSummaries, timeWindow, visibleLogs],
  );
  const channelIds = useMemo(() => channels.map((channel) => channel.id), [channels]);
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 6 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  );

  function handleDragEnd(event: DragEndEvent) {
    const { active, over } = event;
    if (!over || active.id === over.id) {
      return;
    }

    const activeIndex = channelIds.indexOf(String(active.id));
    const overIndex = channelIds.indexOf(String(over.id));
    if (activeIndex === -1 || overIndex === -1) {
      return;
    }

    const nextOrder = [...channelIds];
    const [moved] = nextOrder.splice(activeIndex, 1);
    nextOrder.splice(overIndex, 0, moved);
    setChannelOrder(nextOrder);
  }

  async function refresh(showSuccess = false, showLoading = true) {
    if (showLoading) {
      setLoading(true);
    }
    setError(null);
    try {
      const workspace = await loadChannelStatusWorkspace();
      const nextMonitors = workspace.channelStatusSummaries.map((summary) => summary.monitor);
      setKeys(workspace.keyPoolItems);
      setLogs(workspace.requestLogs);
      setHealth(workspace.stationKeyHealth);
      setMonitors(nextMonitors);
      setStatusSummaries(workspace.channelStatusSummaries);
      if (showSuccess) {
        toast.success("渠道状态已刷新");
      }
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      toast.error("刷新渠道状态失败", message);
    } finally {
      if (showLoading) {
        setLoading(false);
      }
    }
  }

  return (
    <>
      <div className="flex flex-wrap items-center justify-between gap-2">
        <SegmentedControl
          ariaLabel="渠道状态范围"
          value={timeWindow}
          options={[
            { value: "recent", label: "最近请求" },
            { value: "24h", label: "24 小时" },
            { value: "7d", label: "7 天" },
          ]}
          onChange={setTimeWindow}
        />
        <div className="flex items-center gap-2">
          <Button variant="secondary" disabled={loading} onClick={() => void refresh(true)}>
            <RefreshCw className="h-4 w-4" />
            刷新
          </Button>
        </div>
      </div>

      {error && <div className="rounded-xl border border-danger-border bg-danger-surface px-3 py-2 text-sm text-danger-foreground">{error}</div>}
      {channels.length === 0 ? (
        <EmptyState
          title={loading ? "正在读取渠道状态" : "暂无可展示的密钥"}
          description="在密钥池打开监控后，对应密钥会在这里同步显示。"
        />
      ) : (
        <DndContext sensors={sensors} collisionDetection={closestCenter} onDragEnd={handleDragEnd}>
          <SortableContext items={channelIds} strategy={rectSortingStrategy}>
            <div className="grid grid-cols-1 justify-start gap-3 sm:grid-cols-[repeat(auto-fill,minmax(320px,360px))]">
              {channels.map((channel) => (
                <SortableChannelHealthCard key={channel.id} channel={channel} />
              ))}
            </div>
          </SortableContext>
        </DndContext>
      )}
    </>
  );
}

function SortableChannelHealthCard({ channel }: { channel: ChannelHealth }) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({
    id: channel.id,
  });
  const style: CSSProperties = {
    transform: CSS.Transform.toString(transform),
    transition,
  };

  return (
    <div
      ref={setNodeRef}
      style={style}
      className={cn("will-change-transform", isDragging && "opacity-70")}
    >
      <ChannelHealthCard
        channel={channel}
        dragAttributes={attributes}
        dragListeners={listeners}
        isDragging={isDragging}
      />
    </div>
  );
}

function ChannelHealthCard({
  channel,
  dragAttributes,
  dragListeners,
  isDragging = false,
}: {
  channel: ChannelHealth;
  dragAttributes?: DraggableAttributes;
  dragListeners?: ReturnType<typeof useSortable>["listeners"];
  isDragging?: boolean;
}) {
  const typeLabel = stationTypeLabels[channel.stationType as keyof typeof stationTypeLabels] ?? channel.stationType;
  const cooldownActive = isFutureTime(channel.cooldownUntil);
  const availability = formatAvailability(channel.availabilityPercent);

  return (
    <section
      className={cn(
        "relative rounded-[var(--surface-radius)] border border-border bg-surface p-3.5 pt-6 shadow-[var(--surface-shadow)] transition-shadow",
        isDragging && "shadow-[var(--surface-shadow-hover)]",
      )}
    >
      <button
        type="button"
        aria-label={`拖拽排序 ${channel.keyName}`}
        title="拖拽排序"
        className="absolute left-1/2 top-2 inline-flex h-5 w-12 -translate-x-1/2 cursor-grab flex-col items-center justify-center gap-[2px] rounded-[6px] text-muted-foreground/45 transition-colors hover:bg-muted hover:text-muted-foreground active:cursor-grabbing focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/30"
        {...dragAttributes}
        {...dragListeners}
      >
        <span className="h-[2px] w-5 rounded-full bg-current" />
        <span className="h-[2px] w-7 rounded-full bg-current" />
        <span className="h-[2px] w-4 rounded-full bg-current" />
      </button>
      <div className="flex items-start justify-between gap-3">
        <div className="flex min-w-0 items-start gap-2.5">
          <div className={cn("flex h-9 w-9 shrink-0 items-center justify-center rounded-[8px]", iconTone(channel.status))}>
            <Server className="h-4 w-4" />
          </div>
          <div className="min-w-0">
            <div className="truncate text-[15px] font-semibold leading-5 text-foreground">{channel.keyName}</div>
            <div className="mt-1 flex min-w-0 items-center gap-1.5">
              <span className="shrink-0 rounded-[6px] bg-success-surface px-1.5 py-0.5 text-[11px] font-medium leading-4 text-success-foreground">
                {typeLabel}
              </span>
              <span className="min-w-0 truncate text-xs text-muted-foreground">
                {channel.stationName} · {channel.modelSummary}
              </span>
            </div>
          </div>
        </div>
        <div className="flex shrink-0 items-start gap-1.5">
          <StatusBadge tone={statusTone[channel.status]} className="h-6 border-0 px-2.5">
            {cooldownActive ? "冷却中" : statusLabel[channel.status]}
          </StatusBadge>
        </div>
      </div>

      <div className="mt-3 grid grid-cols-2 gap-2">
        <ChannelMetric icon={Timer} label="对话延迟" value={formatLatency(channel.latencyMs)} />
        <ChannelMetric icon={Radio} label="端点 PING" value={formatLatency(channel.endpointPingMs)} />
      </div>

      <div className="mt-3 border-t border-border pt-3">
        <div className="flex items-end justify-between gap-3">
          <div className="min-w-0 text-xs font-medium text-muted-foreground">{channel.availabilityLabel}</div>
          <div
            className={cn(
              "shrink-0 text-3xl font-semibold leading-8 tracking-normal",
              availabilityToneClassName[availabilityTone(channel)],
            )}
          >
            {availability}
          </div>
        </div>
      </div>

      <div className="mt-2.5 border-t border-border pt-2.5">
        <div className="mb-1.5 flex items-center justify-between text-[11px] text-muted-foreground/70">
          <span>较早</span>
          <span>最后检查 {formatCompactTime(channel.lastCheckedAt)}</span>
        </div>
        <div className="grid grid-cols-[repeat(60,minmax(2px,1fr))] gap-[2px]">
          {channel.recentOutcomes.map((outcome, index) => (
            <span
              key={`${channel.id}-${index}`}
              className={cn("h-5 rounded-[2px]", outcomeClassName[outcome])}
              title={outcomeLabel(outcome)}
            />
          ))}
        </div>
        <div className="mt-1 flex justify-between text-[10px] leading-3 text-muted-foreground/70">
          <span>过去</span>
          <span>现在</span>
        </div>
      </div>

    </section>
  );
}

function ChannelMetric({
  icon: Icon,
  label,
  value,
}: {
  icon: LucideIcon;
  label: string;
  value: string;
}) {
  return (
    <div className="min-w-0 rounded-[8px] border border-border bg-surface-subtle px-3 py-2.5">
      <div className="flex items-center gap-1.5 text-[11px] font-medium text-muted-foreground/70">
        <Icon className="h-3.5 w-3.5" />
        <span className="truncate">{label}</span>
      </div>
      <div className="mt-2 truncate text-[18px] font-semibold leading-6 text-foreground">{value}</div>
    </div>
  );
}

function formatLatency(value: number | null) {
  return value === null ? "--" : `${value}ms`;
}

function formatAvailability(value: number | null) {
  return value === null ? "--" : `${value.toFixed(2)}%`;
}

function filterLogsByWindow(logs: RequestLog[], timeWindow: ChannelWindow) {
  if (timeWindow === "recent") {
    return logs;
  }
  const windowMs = timeWindow === "24h" ? 24 * 60 * 60 * 1000 : 7 * 24 * 60 * 60 * 1000;
  const cutoff = Date.now() - windowMs;
  return logs.filter((log) => toTime(log.startedAt) >= cutoff);
}

function buildChannels(
  keys: KeyPoolItem[],
  logs: RequestLog[],
  health: StationKeyHealth[],
  monitors: ChannelMonitor[],
  statusSummaries: ChannelStatusSummary[],
  timeWindow: ChannelWindow,
): ChannelHealth[] {
  const healthByKey = new Map(health.map((item) => [item.stationKeyId, item] as const));
  const monitorByKey = enabledStationKeyMonitorsByKey(monitors);
  const summaryByMonitor = new Map(statusSummaries.map((summary) => [summary.monitor.id, summary] as const));
  return keys.flatMap((key) => {
    const monitor = monitorByKey.get(key.id);
    if (!monitor) {
      return [];
    }
    const keyHealth = healthByKey.get(key.id);
    const backendSummary = summaryByMonitor.get(monitor.id);
    const windowSummary = backendSummary ? selectChannelStatusWindowSummary(backendSummary, timeWindow) : null;
    const aggregateHealth = timeWindow === "recent" ? keyHealth : null;
    const latestStatus = windowSummary?.latestStatus
      ? monitorRunToStationKeyStatus({ status: windowSummary.latestStatus })
      : "unchecked";
    const keyLogs = logs
      .filter((log) => log.stationKeyId === key.id)
      .sort((a, b) => toTime(a.startedAt) - toTime(b.startedAt));
    const totalHealthRequests = (aggregateHealth?.successCount ?? 0) + (aggregateHealth?.failureCount ?? 0);
    const availabilityPercent =
      windowSummary?.availabilityPercent ??
      (totalHealthRequests === 0
        ? timeWindow !== "recent" || key.successRate === null
          ? null
          : key.successRate * 100
        : ((aggregateHealth?.successCount ?? 0) / totalHealthRequests) * 100);
    const recentLogs = keyLogs.slice(-60);
    const requestLatencyMs = averageDurationMs(recentLogs);
    const healthLatencyMs =
      windowSummary?.avgLatencyMs ?? aggregateHealth?.avgLatencyMs ?? (timeWindow === "recent" ? key.avgLatencyMs : null);
    const { conversationLatencyMs, endpointPingMs } = resolveChannelLatencyMetrics({
      requestLatencyMs: windowSummary?.avgLatencyMs ?? requestLatencyMs,
      healthLatencyMs,
      endpointPingMs:
        windowSummary?.avgEndpointPingMs ?? (timeWindow === "recent" ? key.endpointPingMs : null),
    });
    const recentOutcomes = windowSummary
      ? buildMonitorTimelineOutcomes(windowSummary.timeline)
      : buildRecentOutcomes(recentLogs, aggregateHealth);
    const lastError =
      windowSummary?.latestErrorMessage ??
        aggregateHealth?.lastErrorSummary ??
        (timeWindow === "recent" ? key.lastErrorSummary : null) ??
        [...keyLogs].reverse().find((log) => log.errorMessage)?.errorMessage ??
        null;
    const nonSuccessCount =
      windowSummary === null ? aggregateHealth?.failureCount ?? 0 : windowSummary.failureCount + windowSummary.warningCount;

    return [{
      id: key.id,
      keyName: key.name,
      stationName: key.stationName,
      stationType: key.stationType,
      modelSummary: key.modelScopeSummary || key.groupName || key.tierLabel || "全部模型",
      status: key.enabled
        ? cooldownStatus(latestStatus, keyHealth?.cooldownUntil ?? key.cooldownUntil)
        : "disabled",
      latencyMs: conversationLatencyMs,
      endpointPingMs,
      availabilityPercent,
      lastCheckedAt:
        windowSummary?.lastCheckedAt ??
        aggregateHealth?.updatedAt ??
        (timeWindow === "recent" ? key.lastCheckedAt : null),
      lastUsedAt:
        windowSummary?.latestStatus === "success"
          ? windowSummary.lastCheckedAt
          : aggregateHealth?.lastSuccessAt ?? (timeWindow === "recent" ? key.lastUsedAt : null),
      lastError,
      successCount: windowSummary?.successCount ?? aggregateHealth?.successCount ?? 0,
      failureCount: nonSuccessCount,
      consecutiveFailures: windowSummary
        ? countConsecutiveNonSuccess(windowSummary.timeline)
        : aggregateHealth?.consecutiveFailures ?? key.consecutiveFailures,
      cooldownUntil: keyHealth?.cooldownUntil ?? key.cooldownUntil,
      recentOutcomes,
      availabilityLabel: availabilityLabelForWindow(timeWindow),
    }];
  });
}

function availabilityLabelForWindow(timeWindow: ChannelWindow) {
  if (timeWindow === "24h") {
    return "可用性 · 24 小时";
  }
  if (timeWindow === "7d") {
    return "可用性 · 7 天";
  }
  return "可用性 · 近 60 次";
}

function countConsecutiveNonSuccess(timeline: ChannelStatusTimelinePoint[]) {
  let consecutiveFailures = 0;
  for (const point of timeline) {
    if (point.status === "success") {
      break;
    }
    consecutiveFailures += 1;
  }
  return consecutiveFailures;
}

function averageDurationMs(logs: RequestLog[]) {
  const durations = logs.flatMap((log) => (typeof log.durationMs === "number" ? [log.durationMs] : []));
  if (durations.length === 0) {
    return null;
  }
  return Math.round(durations.reduce((sum, duration) => sum + duration, 0) / durations.length);
}

function cooldownStatus(status: StationKeyStatus, cooldownUntil: string | null): StationKeyStatus {
  if (isFutureTime(cooldownUntil)) {
    return "warning";
  }
  return status;
}

function outcomeLabel(outcome: RecentOutcome) {
  if (outcome === "success") return "成功";
  if (outcome === "warning") return "兜底";
  if (outcome === "failed") return "失败";
  return "暂无";
}

function iconTone(status: StationKeyStatus) {
  if (status === "healthy") {
    return "bg-success-surface text-success-foreground";
  }
  if (status === "warning") {
    return "bg-warning-surface text-warning-foreground";
  }
  if (status === "error") {
    return "bg-danger-surface text-danger-foreground";
  }
  return "bg-muted text-muted-foreground";
}

function formatCompactTime(value: string | null) {
  if (!value) {
    return "--";
  }
  const date = parseTimestampLikeDate(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return date.toLocaleString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function isFutureTime(value: string | null) {
  if (!value) {
    return false;
  }
  const date = parseTimestampLikeDate(value);
  return !Number.isNaN(date.getTime()) && date.getTime() > Date.now();
}

function toTime(value: string) {
  return toTimestampMillis(value);
}
