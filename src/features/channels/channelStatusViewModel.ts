import type {
  ChannelStatusBucket,
  ChannelStatusOutcome,
  ChannelStatusRecentPoint,
  ChannelStatusRow,
  ChannelStatusWorkspace,
  ChannelStatusWorkspaceInput,
  ChannelStatusWorkspaceWindow,
} from "@/lib/types/channelMonitors";
import { normalizeGroupCategory } from "@/lib/groupCategories";
import { groupVisualMetaFor, type StationGroupVisualPlatform } from "@/lib/groupVisualMeta";
import { protocolLabel } from "@/lib/channelMonitorDisplay";

export type ChannelWindow = ChannelStatusWorkspaceWindow;

export type StatusTone = "available" | "degraded" | "unavailable" | "skipped" | "missing" | "running" | "disabled";
export type TrendCellTone = "available" | "degraded" | "unavailable" | "skipped" | "missing" | "dirty" | "corrupt";

export type ChannelStatusFilters = {
  search: string;
  enabled: "all" | "enabled" | "disabled";
  outcome: "all" | ChannelStatusOutcome;
};

export type ChannelStatusSortModel = {
  field: NonNullable<ChannelStatusWorkspaceInput["sort"]>["field"];
  direction: NonNullable<ChannelStatusWorkspaceInput["sort"]>["direction"];
};

export type ChannelStatusInputState = {
  window: ChannelWindow;
  filters: ChannelStatusFilters;
  sort: ChannelStatusSortModel;
  limit?: number;
};

export type TrendCellView = {
  id: string;
  tone: TrendCellTone;
  label: string;
  title: string;
  modelLabel: string;
  timeLabel: string;
  availabilityLabel: string;
  httpStatus: number | null;
  latencyLabel: string;
  latencyMs: number | null;
  startMs: number | null;
  endMs: number | null;
  total: number | null;
};

export type ChannelStatusRowView = {
  rowKey: string;
  monitorId: string;
  monitorName: string;
  targetName: string;
  stationName: string;
  keyName: string | null;
  groupName: string | null;
  visualPlatform: StationGroupVisualPlatform;
  visualPlatformLabel: string;
  enabled: boolean;
  modelLabel: string;
  currentTone: StatusTone;
  currentLabel: string;
  currentReason: string | null;
  runningExecutionId: string | null;
  latestExecutionId: string | null;
  availabilityPercent: number | null;
  availabilityLabel: string;
  latencyMs: number | null;
  latencyLabel: string;
  endpointPingMs: number | null;
  endpointPingLabel: string;
  lastCheckedAtMs: number | null;
  lastCheckedLabel: string;
  recentTrend: TrendCellView[];
  trend: TrendCellView[];
  dirty: boolean;
  corrupt: boolean;
};

export type ChannelStatusWorkspaceView = {
  rows: ChannelStatusRowView[];
};

export const defaultChannelStatusFilters: ChannelStatusFilters = {
  search: "",
  enabled: "all",
  outcome: "all",
};

export const defaultChannelStatusSort: ChannelStatusSortModel = {
  field: "latest_checked_at",
  direction: "desc",
};

export function createChannelStatusWorkspaceInput({
  window,
  filters,
  sort,
  limit = 500,
}: ChannelStatusInputState): ChannelStatusWorkspaceInput {
  return {
    window,
    filter: {
      search: blankToNull(filters.search),
      enabled: filters.enabled === "all" ? null : filters.enabled === "enabled",
      outcome: filters.outcome === "all" ? null : filters.outcome,
      protocolKind: null,
      clientProfileId: null,
      stationId: null,
    },
    sort: {
      field: sort.field,
      direction: sort.direction,
    },
    cursor: null,
    limit,
  };
}

export function buildChannelStatusWorkspaceView(
  workspace: ChannelStatusWorkspace | undefined,
): ChannelStatusWorkspaceView {
  if (!workspace) {
    return {
      rows: [],
    };
  }

  return {
    rows: workspace.rows.map((row) => buildRowView(row, workspace.window)),
  };
}

export function buildRowView(row: ChannelStatusRow, window: ChannelWindow): ChannelStatusRowView {
  const latest = row.latest;
  const selected = row.selectedWindow;
  const runningExecutionId = row.running?.executionId ?? null;
  const currentOutcome = latest?.outcome ?? selected.latestOutcome;
  const currentTone: StatusTone = row.monitor.enabled && !row.monitor.balancePaused
    ? runningExecutionId
      ? "running"
      : currentOutcome
    : "disabled";
  const availabilityPercent = bpsToPercent(selected.effectiveAvailabilityBps);
  const groupEvidence = [
    row.target.groupName,
    row.monitor.primaryModel,
    protocolLabel(row.monitor.protocolKind),
  ].filter((value): value is string => Boolean(value)).join(" ");
  const groupVisualMeta = groupVisualMetaFor(
    groupEvidence,
    null,
    normalizeGroupCategory(row.target.effectiveGroupCategory),
  );

  return {
    rowKey: row.rowKey,
    monitorId: row.monitor.id,
    monitorName: row.monitor.name,
    targetName: row.target.stationKeyName ?? row.target.stationName ?? row.monitor.name,
    stationName: row.target.stationName ?? row.target.stationId,
    keyName: row.target.stationKeyName,
    groupName: row.target.groupName,
    visualPlatform: groupVisualMeta.platform,
    visualPlatformLabel: groupVisualMeta.label,
    enabled: row.monitor.enabled,
    modelLabel: formatModelLabel(row.monitor.primaryModel, row.monitor.fallbackModels),
    currentTone,
    currentLabel: !row.monitor.enabled
      ? "停用"
      : row.monitor.balancePaused
        ? "余额暂停"
        : statusLabel(currentTone),
    currentReason: latest?.terminalReason ?? latest?.failureKind ?? null,
    runningExecutionId,
    latestExecutionId: latest?.executionId ?? null,
    availabilityPercent,
    availabilityLabel: formatAvailability(availabilityPercent),
    latencyMs: latest?.latencyMs ?? null,
    latencyLabel: formatLatency(latest?.latencyMs ?? null),
    endpointPingMs: row.target.endpointPing?.latencyMs ?? null,
    endpointPingLabel: formatLatency(row.target.endpointPing?.latencyMs ?? null),
    lastCheckedAtMs: selected.latestCheckedAtMs,
    lastCheckedLabel: formatTime(selected.latestCheckedAtMs),
    recentTrend: buildTrend(row, "recent"),
    trend: buildTrend(row, window),
    dirty: selected.dirty || row.hourlyBuckets.some((bucket) => bucket.dirty) || row.dailyBuckets.some((bucket) => bucket.dirty),
    corrupt: selected.corrupt || row.hourlyBuckets.some((bucket) => bucket.corrupt) || row.dailyBuckets.some((bucket) => bucket.corrupt),
  };
}

export function buildTrend(row: ChannelStatusRow, window: ChannelWindow): TrendCellView[] {
  const modelLabel = formatModelLabel(row.monitor.primaryModel, row.monitor.fallbackModels);
  if (window === "recent") {
    return [...row.recent].reverse().map((point, index) =>
      recentPointToCell(point, index, row.monitor.primaryModel),
    );
  }
  if (window === "last24h") {
    return row.hourlyBuckets.map((bucket) => bucketToCell(bucket, modelLabel));
  }
  if (window === "last7d") {
    return row.dailyBuckets.slice(-7).map((bucket) => bucketToCell(bucket, modelLabel));
  }
  return row.dailyBuckets.map((bucket) => bucketToCell(bucket, modelLabel));
}

export function availabilityHue(value: number | null): number | null {
  if (value === null || Number.isNaN(value)) {
    return null;
  }
  return Math.max(0, Math.min(100, value)) * 1.2;
}

export function statusLabel(tone: StatusTone) {
  switch (tone) {
    case "available":
      return "正常";
    case "degraded":
      return "降级";
    case "unavailable":
      return "错误";
    case "skipped":
      return "跳过";
    case "running":
      return "运行中";
    case "disabled":
      return "已停用";
    default:
      return "无数据";
  }
}

export function formatAvailability(value: number | null) {
  return value === null ? "--" : `${value.toFixed(2)}%`;
}

export function formatLatency(value: number | null) {
  return value === null ? "--" : `${value} ms`;
}

export function formatTime(value: number | null | undefined) {
  if (value === null || value === undefined) {
    return "--";
  }
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return "--";
  }
  return date.toLocaleString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function formatTrendTime(value: number | null | undefined) {
  if (value === null || value === undefined) {
    return "--";
  }
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return "--";
  }
  return date.toLocaleString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  }).replace(/\//g, "-");
}

function recentPointToCell(point: ChannelStatusRecentPoint, index: number, primaryModel: string): TrendCellView {
  const tone = outcomeToTrendTone(point.outcome);
  const availabilityLabel = outcomeLabelWithUnavailableDetail(
    point.outcome,
    point.httpStatus,
    point.terminalReason,
    point.failureKind,
  );
  const timeLabel = formatTrendTime(point.checkedAtMs);
  const latencyLabel = formatLatency(point.latencyMs);
  const modelLabel = point.effectiveModel ?? primaryModel;
  return {
    id: point.targetResultId || `${point.executionId}-${index}`,
    tone,
    label: availabilityLabel,
    title: [
      `模型：${modelLabel}`,
      `时间：${timeLabel}`,
      `状态：${availabilityLabel}`,
      `延迟：${latencyLabel}`,
      `尝试：${point.attemptCount}`,
      point.failureKind ? `失败分类：${point.failureKind}` : null,
      point.terminalReason ? `原因：${point.terminalReason}` : null,
    ].filter(Boolean).join("\n"),
    modelLabel,
    timeLabel,
    availabilityLabel,
    httpStatus: point.httpStatus,
    latencyLabel,
    latencyMs: point.latencyMs,
    startMs: point.checkedAtMs,
    endMs: point.checkedAtMs,
    total: 1,
  };
}

function bucketToCell(bucket: ChannelStatusBucket, modelLabel: string): TrendCellView {
  const tone = bucket.corrupt ? "corrupt" : bucketStateToTrendTone(bucket);
  const failureCounts = Object.entries(bucket.failureCounts)
    .map(([kind, count]) => `${kind}:${count}`)
    .join(", ");
  const availabilityLabel = bucketStateLabel(bucket.state);
  const timeLabel = `${formatTrendTime(bucket.startMs)} - ${formatTrendTime(bucket.endMs)}`;
  const latencyLabel = formatLatency(bucket.p50LatencyMs);
  return {
    id: `${bucket.kind}-${bucket.startMs}`,
    tone,
    label: availabilityLabel,
    title: [
      `模型：${modelLabel}`,
      `时间：${timeLabel}`,
      `状态：${availabilityLabel}`,
      `延迟：${latencyLabel}`,
      `样本：${bucket.counts.total} · 正常 ${bucket.counts.available} · 降级 ${bucket.counts.degraded} · 错误 ${bucket.counts.unavailable} · 跳过 ${bucket.counts.skipped}`,
      `可用性：${formatAvailability(bpsToPercent(bucket.effectiveAvailabilityBps))}`,
      `P50/P95：${formatLatency(bucket.p50LatencyMs)} / ${formatLatency(bucket.p95LatencyMs)}`,
      failureCounts ? `失败分类：${failureCounts}` : null,
      bucket.corrupt ? "汇总数据异常" : null,
    ].filter(Boolean).join("\n"),
    modelLabel,
    timeLabel,
    availabilityLabel,
    httpStatus: null,
    latencyLabel,
    latencyMs: bucket.p50LatencyMs,
    startMs: bucket.startMs,
    endMs: bucket.endMs,
    total: bucket.counts.total,
  };
}

function bucketStateToTrendTone(bucket: ChannelStatusBucket): TrendCellTone {
  if (bucket.state === "available") return "available";
  if (bucket.state === "degraded") return "degraded";
  if (bucket.state === "unavailable") return "unavailable";
  if (bucket.state === "skipped_only") return "skipped";
  return "missing";
}

function outcomeToTrendTone(outcome: ChannelStatusOutcome): TrendCellTone {
  if (outcome === "available") return "available";
  if (outcome === "degraded") return "degraded";
  if (outcome === "unavailable") return "unavailable";
  if (outcome === "skipped") return "skipped";
  return "missing";
}

const MAX_UNAVAILABLE_DETAIL_LENGTH = 64;

function outcomeLabelWithUnavailableDetail(
  outcome: ChannelStatusOutcome,
  httpStatus: number | null,
  terminalReason: string | null,
  failureKind: string | null,
) {
  const label = outcomeLabel(outcome);
  if (outcome !== "unavailable") {
    return label;
  }
  const detail = httpStatusLabel(httpStatus)
    ?? httpStatusFromReason(terminalReason)
    ?? namedErrorCodeFromReason(terminalReason)
    ?? compactUnavailableReason(terminalReason)
    ?? compactUnavailableReason(failureKind);
  return detail ? `${label} (${detail})` : label;
}

function httpStatusLabel(value: number | null) {
  if (value === null || !Number.isInteger(value) || value < 100 || value > 599) {
    return null;
  }
  return String(value);
}

function httpStatusFromReason(value: string | null) {
  const match = value?.match(/\b([1-5][0-9]{2})\b/);
  return match?.[1] ?? null;
}

function namedErrorCodeFromReason(value: string | null) {
  const match = value?.match(/["']?\b(?:error[_ -]?code|code)\b["']?\s*[:=]\s*["']?([A-Za-z0-9_.-]{2,64})/i);
  return compactUnavailableReason(match?.[1] ?? null);
}

function compactUnavailableReason(value: string | null | undefined) {
  const normalized = redactSensitiveReason(value?.trim().replace(/\s+/g, " ") ?? "");
  if (!normalized) {
    return null;
  }
  return normalized.length > MAX_UNAVAILABLE_DETAIL_LENGTH
    ? `${normalized.slice(0, MAX_UNAVAILABLE_DETAIL_LENGTH - 3)}...`
    : normalized;
}

function redactSensitiveReason(value: string) {
  return value
    .replace(/\bsk-[A-Za-z0-9_-]{8,}\b/g, "sk-***")
    .replace(/\b(Bearer\s+)[A-Za-z0-9._-]{8,}/gi, "$1***")
    .replace(/\b((?:api[_-]?key|token|cookie)=)[^\s&]+/gi, "$1***");
}

function outcomeLabel(outcome: ChannelStatusOutcome) {
  switch (outcome) {
    case "available":
      return "正常";
    case "degraded":
      return "降级";
    case "unavailable":
      return "错误";
    case "skipped":
      return "跳过";
    default:
      return "缺失";
  }
}

function bucketStateLabel(state: ChannelStatusBucket["state"]) {
  switch (state) {
    case "available":
      return "正常";
    case "degraded":
      return "降级";
    case "unavailable":
      return "错误";
    case "skipped_only":
      return "仅跳过";
    case "dirty":
      return "缺失";
    default:
      return "缺失";
  }
}

function formatModelLabel(primary: string, fallbacks: string[]) {
  if (fallbacks.length === 0) {
    return primary;
  }
  return `${primary} +${fallbacks.length}`;
}

function bpsToPercent(value: number | null) {
  return value === null ? null : value / 100;
}

function blankToNull(value: string) {
  const trimmed = value.trim();
  return trimmed.length === 0 ? null : trimmed;
}
