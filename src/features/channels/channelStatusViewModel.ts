import type { RequestLog } from "@/lib/types/proxy";
import type { StationKeyHealth } from "@/lib/types/routing";
import type { ChannelMonitor, ChannelMonitorRun } from "@/lib/types/channelMonitors";
import type { StationKeyStatus } from "@/lib/types/stationKeys";

export type RecentOutcome = "success" | "warning" | "failed" | "unknown";

export type ChannelAvailabilityState = {
  status: StationKeyStatus;
  availabilityPercent: number | null;
};

export type OrderedChannel = {
  id: string;
};

export type ChannelLatencyMetricInput = {
  requestLatencyMs: number | null;
  healthLatencyMs: number | null;
};

export type ChannelLatencyMetrics = {
  conversationLatencyMs: number | null;
  endpointPingMs: number | null;
};

type HealthOutcomeSummary = Pick<StationKeyHealth, "successCount" | "failureCount">;
type StationKeyMonitorSummary = Pick<
  ChannelMonitor,
  "id" | "targetType" | "stationKeyId" | "enabled" | "updatedAt"
>;
type MonitorRunSummary = Pick<ChannelMonitorRun, "status" | "startedAt">;

export function enabledStationKeyMonitorsByKey(monitors: StationKeyMonitorSummary[]) {
  const monitorByKey = new Map<string, StationKeyMonitorSummary>();
  for (const monitor of monitors) {
    if (!monitor.enabled || monitor.targetType !== "station_key" || !monitor.stationKeyId) {
      continue;
    }
    const existing = monitorByKey.get(monitor.stationKeyId);
    if (!existing || toTime(monitor.updatedAt) >= toTime(existing.updatedAt)) {
      monitorByKey.set(monitor.stationKeyId, monitor);
    }
  }
  return monitorByKey;
}

export function monitorRunToStationKeyStatus(
  run: Pick<ChannelMonitorRun, "status"> | null | undefined,
): StationKeyStatus {
  if (!run) {
    return "unchecked";
  }
  if (run.status === "success") {
    return "healthy";
  }
  if (run.status === "failed") {
    return "error";
  }
  return "warning";
}

export function buildMonitorRecentOutcomes(runs: MonitorRunSummary[]) {
  return padRecentOutcomes(
    [...runs]
      .sort((a, b) => toTime(a.startedAt) - toTime(b.startedAt))
      .slice(-60)
      .map(monitorRunToOutcome),
  );
}

export function availabilityToneClassName(channel: ChannelAvailabilityState) {
  if (channel.status === "disabled" || channel.availabilityPercent === null) {
    return "text-slate-500";
  }
  if (channel.availabilityPercent < 50) {
    return "text-rose-600";
  }
  if (channel.availabilityPercent < 75) {
    return "text-orange-600";
  }
  return "text-emerald-600";
}

export function buildRecentOutcomes(
  logs: RequestLog[],
  health: HealthOutcomeSummary | null | undefined,
) {
  const logOutcomes = logs.slice(-60).map(logToOutcome);
  if (logOutcomes.length > 0) {
    return padRecentOutcomes(logOutcomes);
  }

  const healthOutcomes = healthToRecentOutcomes(health);
  return padRecentOutcomes(healthOutcomes);
}

export function orderChannelsBySavedOrder<TChannel extends OrderedChannel>(
  channels: TChannel[],
  savedOrder: string[],
) {
  if (savedOrder.length === 0) {
    return channels;
  }

  const channelById = new Map(channels.map((channel) => [channel.id, channel] as const));
  const orderedChannels = savedOrder.flatMap((id) => {
    const channel = channelById.get(id);
    return channel ? [channel] : [];
  });
  const orderedIds = new Set(orderedChannels.map((channel) => channel.id));
  const newChannels = channels.filter((channel) => !orderedIds.has(channel.id));
  return orderedChannels.concat(newChannels);
}

export function resolveChannelLatencyMetrics({
  requestLatencyMs,
  healthLatencyMs,
}: ChannelLatencyMetricInput): ChannelLatencyMetrics {
  return {
    conversationLatencyMs: requestLatencyMs ?? healthLatencyMs,
    // Health latency is a model probe/request duration, not a separate endpoint ping measurement.
    endpointPingMs: null,
  };
}

export function logToOutcome(log: RequestLog): RecentOutcome {
  if (log.status === "success") {
    return "success";
  }
  if (log.status === "fallback" || log.fallbackCount > 0) {
    return "warning";
  }
  if (log.status === "failed") {
    return "failed";
  }
  return "unknown";
}

function monitorRunToOutcome(run: Pick<ChannelMonitorRun, "status">): RecentOutcome {
  if (run.status === "success") {
    return "success";
  }
  if (run.status === "warning" || run.status === "skipped") {
    return "warning";
  }
  if (run.status === "failed") {
    return "failed";
  }
  return "unknown";
}

function padRecentOutcomes(outcomes: RecentOutcome[]) {
  const recentOutcomes = outcomes.slice(-60);
  const unknownOutcomes: RecentOutcome[] = Array.from(
    { length: 60 - recentOutcomes.length },
    () => "unknown",
  );
  return unknownOutcomes.concat(recentOutcomes);
}

function healthToRecentOutcomes(health: HealthOutcomeSummary | null | undefined) {
  if (!health) {
    return [];
  }
  const successCount = Math.max(0, health.successCount);
  const failureCount = Math.max(0, health.failureCount);
  const total = successCount + failureCount;
  if (total === 0) {
    return [];
  }

  if (total <= 60) {
    return [
      ...Array.from({ length: failureCount }, () => "failed" as const),
      ...Array.from({ length: successCount }, () => "success" as const),
    ];
  }

  const successSlots = Math.round((successCount / total) * 60);
  const failureSlots = 60 - successSlots;
  return [
    ...Array.from({ length: failureSlots }, () => "failed" as const),
    ...Array.from({ length: successSlots }, () => "success" as const),
  ];
}

function toTime(value: string) {
  const numeric = Number(value);
  const date = Number.isFinite(numeric) && numeric > 1000000000000 ? new Date(numeric) : new Date(value);
  return date.getTime();
}
