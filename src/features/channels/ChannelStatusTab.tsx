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
import { GripVertical, Radio, RefreshCw, Server, Timer } from "lucide-react";
import { Button, EmptyState, SegmentedControl, StatusBadge, useToast } from "@/components/ui";
import { listRequestLogs } from "@/lib/api/proxy";
import { listStationKeyHealth } from "@/lib/api/routing";
import { listKeyPoolItems } from "@/lib/api/stationKeys";
import type { RequestLog } from "@/lib/types/proxy";
import type { StationKeyHealth } from "@/lib/types/routing";
import type { KeyPoolItem, StationKeyStatus } from "@/lib/types/stationKeys";
import { stationTypeLabels } from "@/lib/types/stations";
import { cn } from "@/lib/utils";
import {
  availabilityToneClassName,
  buildRecentOutcomes,
  orderChannelsBySavedOrder,
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
  const [channelOrder, setChannelOrder] = useState<string[]>([]);
  const [timeWindow, setTimeWindow] = useState<ChannelWindow>("recent");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void refresh();
  }, [refreshToken]);

  const visibleLogs = useMemo(() => filterLogsByWindow(logs, timeWindow), [logs, timeWindow]);
  const channels = useMemo(
    () => orderChannelsBySavedOrder(buildChannels(keys, visibleLogs, health), channelOrder),
    [channelOrder, health, keys, visibleLogs],
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
      const [nextKeys, nextLogs, nextHealth] = await Promise.all([
        listKeyPoolItems(),
        listRequestLogs(),
        listStationKeyHealth(),
      ]);
      setKeys(nextKeys);
      setLogs(nextLogs);
      setHealth(nextHealth);
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
        <Button variant="secondary" onClick={() => void refresh(true)}>
          <RefreshCw className="h-4 w-4" />
          刷新
        </Button>
      </div>

      {error && <div className="rounded-xl border border-rose-100 bg-rose-50 px-3 py-2 text-sm text-rose-700">{error}</div>}
      {channels.length === 0 ? (
        <EmptyState
          title={loading ? "正在读取渠道状态" : "暂无可展示的密钥"}
          description="添加并启用密钥后，本地代理请求会在这里形成状态。"
        />
      ) : (
        <DndContext sensors={sensors} collisionDetection={closestCenter} onDragEnd={handleDragEnd}>
          <SortableContext items={channelIds} strategy={rectSortingStrategy}>
            <div className="grid gap-3 md:grid-cols-2 2xl:grid-cols-3">
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
        "rounded-[var(--surface-radius)] border border-border bg-white p-3.5 shadow-[var(--surface-shadow)] transition-shadow",
        isDragging && "shadow-[var(--surface-shadow-hover)]",
      )}
    >
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
          <button
            type="button"
            aria-label={`拖拽排序 ${channel.keyName}`}
            title="拖拽排序"
            className="inline-flex h-6 w-5 cursor-grab items-center justify-center rounded-[6px] text-slate-300 transition-colors hover:bg-slate-100 hover:text-slate-500 active:cursor-grabbing focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[hsl(var(--accent)/0.28)]"
            {...dragAttributes}
            {...dragListeners}
          >
            <GripVertical className="h-4 w-4" />
          </button>
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

      {(cooldownActive || channel.lastError) && (
        <div className="mt-2 truncate text-xs text-muted-foreground">
          {cooldownActive ? `冷却至 ${formatCompactTime(channel.cooldownUntil)} · ` : ""}
          {channel.lastError ?? ""}
        </div>
      )}
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

function buildChannels(keys: KeyPoolItem[], logs: RequestLog[], health: StationKeyHealth[]): ChannelHealth[] {
  const healthByKey = new Map(health.map((item) => [item.stationKeyId, item] as const));
  return keys.map((key) => {
    const keyHealth = healthByKey.get(key.id);
    const keyLogs = logs
      .filter((log) => log.stationKeyId === key.id)
      .sort((a, b) => toTime(a.startedAt) - toTime(b.startedAt));
    const totalHealthRequests = (keyHealth?.successCount ?? 0) + (keyHealth?.failureCount ?? 0);
    const availabilityPercent =
      totalHealthRequests === 0
        ? key.successRate === null
          ? null
          : key.successRate * 100
        : ((keyHealth?.successCount ?? 0) / totalHealthRequests) * 100;
    const recentLogs = keyLogs.slice(-60);
    const latencyMs = averageDurationMs(recentLogs);
    const endpointPingMs = keyHealth?.avgLatencyMs ?? key.avgLatencyMs;
    const recentOutcomes = buildRecentOutcomes(recentLogs, keyHealth);
    const lastError =
      keyHealth?.lastErrorSummary ?? key.lastErrorSummary ?? [...keyLogs].reverse().find((log) => log.errorMessage)?.errorMessage ?? null;

    return {
      id: key.id,
      keyName: key.name,
      stationName: key.stationName,
      stationType: key.stationType,
      modelSummary: key.modelScopeSummary || key.groupName || key.tierLabel || "全部模型",
      status: key.enabled ? cooldownStatus(key.status, keyHealth?.cooldownUntil ?? key.cooldownUntil) : "disabled",
      latencyMs,
      endpointPingMs,
      availabilityPercent,
      lastCheckedAt: keyHealth?.updatedAt ?? key.lastCheckedAt,
      lastUsedAt: keyHealth?.lastSuccessAt ?? key.lastUsedAt,
      lastError,
      successCount: keyHealth?.successCount ?? 0,
      failureCount: keyHealth?.failureCount ?? 0,
      consecutiveFailures: keyHealth?.consecutiveFailures ?? key.consecutiveFailures,
      cooldownUntil: keyHealth?.cooldownUntil ?? key.cooldownUntil,
      recentOutcomes,
    };
  });
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
