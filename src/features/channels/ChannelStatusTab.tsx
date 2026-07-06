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
import { Button, EmptyState, SegmentedControl, StatusBadge, useToast } from "@/components/ui";
import { listChannelMonitorRuns, listChannelMonitors } from "@/lib/api/channelMonitors";
import { listRequestLogs } from "@/lib/api/proxy";
import { listStationKeyHealth } from "@/lib/api/routing";
import { pingStationEndpoint } from "@/lib/api/stations";
import { listKeyPoolItems } from "@/lib/api/stationKeys";
import type { ChannelMonitor, ChannelMonitorRun } from "@/lib/types/channelMonitors";
import type { RequestLog } from "@/lib/types/proxy";
import type { StationKeyHealth } from "@/lib/types/routing";
import type { KeyPoolItem, StationKeyStatus } from "@/lib/types/stationKeys";
import { stationTypeLabels } from "@/lib/types/stations";
import { cn } from "@/lib/utils";
import {
  availabilityToneClassName,
  buildMonitorRecentOutcomes,
  buildRecentOutcomes,
  enabledStationKeyMonitorsByKey,
  monitorRunToStationKeyStatus,
  orderChannelsBySavedOrder,
  resolveChannelLatencyMetrics,
  type RecentOutcome,
} from "./channelStatusViewModel";

type ChannelWindow = "recent" | "24h" | "7d";

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
  success: "bg-emerald-500/85",
  warning: "bg-amber-400/90",
  failed: "bg-rose-500/90",
  unknown: "bg-slate-300",
};

export function ChannelStatusTab({ refreshToken }: { refreshToken: number }) {
  const toast = useToast();
  const [keys, setKeys] = useState<KeyPoolItem[]>([]);
  const [logs, setLogs] = useState<RequestLog[]>([]);
  const [health, setHealth] = useState<StationKeyHealth[]>([]);
  const [monitors, setMonitors] = useState<ChannelMonitor[]>([]);
  const [runsByMonitor, setRunsByMonitor] = useState(new Map<string, ChannelMonitorRun[]>());
  const [channelOrder, setChannelOrder] = useState<string[]>([]);
  const [timeWindow, setTimeWindow] = useState<ChannelWindow>("recent");
  const [loading, setLoading] = useState(true);
  const [pinging, setPinging] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void refresh();
  }, [refreshToken]);

  const visibleLogs = useMemo(() => filterLogsByWindow(logs, timeWindow), [logs, timeWindow]);
  const channels = useMemo(
    () => orderChannelsBySavedOrder(buildChannels(keys, visibleLogs, health, monitors, runsByMonitor), channelOrder),
    [channelOrder, health, keys, monitors, runsByMonitor, visibleLogs],
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

  async function refresh(showSuccess = false) {
    setLoading(true);
    setError(null);
    try {
      const [nextKeys, nextLogs, nextHealth, nextMonitors] = await Promise.all([
        listKeyPoolItems(),
        listRequestLogs(),
        listStationKeyHealth(),
        listChannelMonitors(),
      ]);
      const runEntries = await Promise.all(
        nextMonitors.map(async (monitor) => {
          try {
            return { monitorId: monitor.id, runs: await listChannelMonitorRuns(monitor.id) };
          } catch {
            return { monitorId: monitor.id, runs: [] as ChannelMonitorRun[] };
          }
        }),
      );
      setKeys(nextKeys);
      setLogs(nextLogs);
      setHealth(nextHealth);
      setMonitors(nextMonitors);
      setRunsByMonitor(new Map(runEntries.map((entry) => [entry.monitorId, entry.runs] as const)));
      if (showSuccess) {
        toast.success("渠道状态已刷新");
      }
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      toast.error("刷新渠道状态失败", message);
    } finally {
      setLoading(false);
    }
  }

  async function pingAllVisibleStations() {
    const stationIds = Array.from(new Set(keys.map((key) => key.stationId)));
    if (stationIds.length === 0) {
      toast.info("暂无可 PING 的中转站");
      return;
    }

    setPinging(true);
    try {
      await Promise.all(stationIds.map((stationId) => pingStationEndpoint(stationId)));
      await refresh(false);
      toast.success("端点 PING 已完成");
    } catch (pingError) {
      const message = readError(pingError);
      toast.error("端点 PING 失败", message);
      await refresh(false);
    } finally {
      setPinging(false);
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
          <Button variant="secondary" disabled={pinging || loading} onClick={() => void pingAllVisibleStations()}>
            <Radio className="h-4 w-4" />
            {pinging ? "PING 中" : "PING"}
          </Button>
          <Button variant="secondary" disabled={loading} onClick={() => void refresh(true)}>
            <RefreshCw className="h-4 w-4" />
            刷新
          </Button>
        </div>
      </div>

      {error && <div className="rounded-xl border border-rose-100 bg-rose-50 px-3 py-2 text-sm text-rose-700">{error}</div>}
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
        "relative rounded-[var(--surface-radius)] border border-border bg-white p-3.5 pt-6 shadow-[var(--surface-shadow)] transition-shadow",
        isDragging && "shadow-[var(--surface-shadow-hover)]",
      )}
    >
      <button
        type="button"
        aria-label={`拖拽排序 ${channel.keyName}`}
        title="拖拽排序"
        className="absolute left-1/2 top-2 inline-flex h-5 w-12 -translate-x-1/2 cursor-grab flex-col items-center justify-center gap-[2px] rounded-[6px] text-slate-300 transition-colors hover:bg-slate-100 hover:text-slate-500 active:cursor-grabbing focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[hsl(var(--accent)/0.28)]"
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
            <div className="truncate text-[15px] font-semibold leading-5 text-slate-900">{channel.keyName}</div>
            <div className="mt-1 flex min-w-0 items-center gap-1.5">
              <span className="shrink-0 rounded-[6px] bg-emerald-50 px-1.5 py-0.5 text-[11px] font-medium leading-4 text-emerald-700">
                {typeLabel}
              </span>
              <span className="min-w-0 truncate text-xs text-slate-500">
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

      <div className="mt-3 border-t border-slate-100 pt-3">
        <div className="flex items-end justify-between gap-3">
          <div className="min-w-0 text-xs font-medium text-slate-500">可用性 · 近 60 次</div>
          <div
            className={cn(
              "shrink-0 text-3xl font-semibold leading-8 tracking-normal",
              availabilityToneClassName(channel),
            )}
          >
            {availability}
          </div>
        </div>
      </div>

      <div className="mt-2.5 border-t border-slate-100 pt-2.5">
        <div className="mb-1.5 flex items-center justify-between text-[11px] text-slate-400">
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
        <div className="mt-1 flex justify-between text-[10px] leading-3 text-slate-400">
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
    <div className="min-w-0 rounded-[8px] border border-slate-100 bg-slate-50/70 px-3 py-2.5">
      <div className="flex items-center gap-1.5 text-[11px] font-medium text-slate-400">
        <Icon className="h-3.5 w-3.5" />
        <span className="truncate">{label}</span>
      </div>
      <div className="mt-2 truncate text-[18px] font-semibold leading-6 text-slate-800">{value}</div>
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
  runsByMonitor: Map<string, ChannelMonitorRun[]>,
): ChannelHealth[] {
  const healthByKey = new Map(health.map((item) => [item.stationKeyId, item] as const));
  const monitorByKey = enabledStationKeyMonitorsByKey(monitors);
  return keys.flatMap((key) => {
    const monitor = monitorByKey.get(key.id);
    if (!monitor) {
      return [];
    }
    const keyHealth = healthByKey.get(key.id);
    const monitorRuns = sortRunsAscending(runsByMonitor.get(monitor.id) ?? []);
    const latestRun = monitorRuns.length > 0 ? monitorRuns[monitorRuns.length - 1] : null;
    const keyLogs = logs
      .filter((log) => log.stationKeyId === key.id)
      .sort((a, b) => toTime(a.startedAt) - toTime(b.startedAt));
    const totalHealthRequests = (keyHealth?.successCount ?? 0) + (keyHealth?.failureCount ?? 0);
    const monitorAvailabilityPercent = availabilityFromMonitorRuns(monitorRuns);
    const availabilityPercent =
      monitorAvailabilityPercent ??
      (totalHealthRequests === 0
        ? key.successRate === null
          ? null
          : key.successRate * 100
        : ((keyHealth?.successCount ?? 0) / totalHealthRequests) * 100);
    const recentLogs = keyLogs.slice(-60);
    const requestLatencyMs = averageDurationMs(recentLogs);
    const monitorLatencyMs = latestRun?.latencyMs ?? latestRun?.durationMs ?? null;
    const healthLatencyMs = monitorLatencyMs ?? keyHealth?.avgLatencyMs ?? key.avgLatencyMs;
    const { conversationLatencyMs, endpointPingMs } = resolveChannelLatencyMetrics({
      requestLatencyMs: monitorLatencyMs ?? requestLatencyMs,
      healthLatencyMs,
      endpointPingMs: key.endpointPingMs,
    });
    const recentOutcomes = monitorRuns.length > 0
      ? buildMonitorRecentOutcomes(monitorRuns)
      : buildRecentOutcomes(recentLogs, keyHealth);
    const lastError = latestRun
      ? latestRun.errorMessage
      : keyHealth?.lastErrorSummary ??
        key.lastErrorSummary ??
        [...keyLogs].reverse().find((log) => log.errorMessage)?.errorMessage ??
        null;
    const monitorCounts = countMonitorRuns(monitorRuns);

    return [{
      id: key.id,
      keyName: key.name,
      stationName: key.stationName,
      stationType: key.stationType,
      modelSummary: key.modelScopeSummary || key.groupName || key.tierLabel || "全部模型",
      status: key.enabled
        ? cooldownStatus(monitorRunToStationKeyStatus(latestRun), keyHealth?.cooldownUntil ?? key.cooldownUntil)
        : "disabled",
      latencyMs: conversationLatencyMs,
      endpointPingMs,
      availabilityPercent,
      lastCheckedAt: latestRun?.finishedAt ?? latestRun?.startedAt ?? keyHealth?.updatedAt ?? key.lastCheckedAt,
      lastUsedAt: latestRun?.status === "success" ? latestRun.finishedAt ?? latestRun.startedAt : keyHealth?.lastSuccessAt ?? key.lastUsedAt,
      lastError,
      successCount: monitorRuns.length > 0 ? monitorCounts.successCount : keyHealth?.successCount ?? 0,
      failureCount: monitorRuns.length > 0 ? monitorCounts.failureCount : keyHealth?.failureCount ?? 0,
      consecutiveFailures: monitorRuns.length > 0 ? monitorCounts.consecutiveFailures : keyHealth?.consecutiveFailures ?? key.consecutiveFailures,
      cooldownUntil: keyHealth?.cooldownUntil ?? key.cooldownUntil,
      recentOutcomes,
    }];
  });
}

function sortRunsAscending(runs: ChannelMonitorRun[]) {
  return [...runs].sort((a, b) => toTime(a.startedAt) - toTime(b.startedAt));
}

function availabilityFromMonitorRuns(runs: ChannelMonitorRun[]) {
  if (runs.length === 0) {
    return null;
  }
  const successCount = runs.filter((run) => run.status === "success").length;
  return (successCount / runs.length) * 100;
}

function countMonitorRuns(runs: ChannelMonitorRun[]) {
  let consecutiveFailures = 0;
  for (const run of [...runs].reverse()) {
    if (run.status === "success") {
      break;
    }
    consecutiveFailures += 1;
  }
  return {
    successCount: runs.filter((run) => run.status === "success").length,
    failureCount: runs.filter((run) => run.status === "failed" || run.status === "warning" || run.status === "skipped").length,
    consecutiveFailures,
  };
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
    return "bg-emerald-100 text-emerald-700";
  }
  if (status === "warning") {
    return "bg-amber-100 text-amber-700";
  }
  if (status === "error") {
    return "bg-rose-100 text-rose-700";
  }
  return "bg-slate-100 text-slate-500";
}

function formatCompactTime(value: string | null) {
  if (!value) {
    return "--";
  }
  const numeric = Number(value);
  const date = Number.isFinite(numeric) && numeric > 1000000000000 ? new Date(numeric) : new Date(value);
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
  const numeric = Number(value);
  const date = Number.isFinite(numeric) && numeric > 1000000000000 ? new Date(numeric) : new Date(value);
  return !Number.isNaN(date.getTime()) && date.getTime() > Date.now();
}

function toTime(value: string) {
  const numeric = Number(value);
  const date = Number.isFinite(numeric) && numeric > 1000000000000 ? new Date(numeric) : new Date(value);
  return date.getTime();
}

function readError(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}
