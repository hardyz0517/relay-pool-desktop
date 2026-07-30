import type {
  ChannelStatusBucket,
  ChannelStatusOutcome,
  ChannelStatusRecentPoint,
  ChannelStatusRow,
  ChannelStatusWorkspace,
  ChannelStatusWorkspaceInput,
  ChannelStatusWorkspaceWindow,
} from "@/lib/types/channelMonitors";

export type ChannelWindow = ChannelStatusWorkspaceWindow;

export type AvailabilityTone = "muted" | "danger" | "warning" | "success";
export type StatusTone = "available" | "degraded" | "unavailable" | "skipped" | "missing" | "running" | "disabled";
export type TrendCellTone = "available" | "degraded" | "unavailable" | "skipped" | "missing" | "dirty" | "corrupt";

export type ChannelStatusFilters = {
  search: string;
  enabled: "all" | "enabled" | "disabled";
  outcome: "all" | ChannelStatusOutcome;
  protocolKind: string;
  clientProfileId: string;
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
  enabled: boolean;
  protocolLabel: string;
  profileLabel: string;
  modelLabel: string;
  currentTone: StatusTone;
  currentLabel: string;
  currentReason: string | null;
  runningExecutionId: string | null;
  latestExecutionId: string | null;
  availabilityPercent: number | null;
  strictAvailabilityPercent: number | null;
  availabilityLabel: string;
  latencyMs: number | null;
  latencyLabel: string;
  lastCheckedAtMs: number | null;
  lastCheckedLabel: string;
  attemptsLabel: string;
  fallbackLabel: string;
  trend: TrendCellView[];
  dirty: boolean;
  corrupt: boolean;
};

export type ChannelStatusWorkspaceView = {
  generatedAtLabel: string;
  freshnessLabel: string;
  aggregateLabel: string;
  rows: ChannelStatusRowView[];
};

export const defaultChannelStatusFilters: ChannelStatusFilters = {
  search: "",
  enabled: "all",
  outcome: "all",
  protocolKind: "",
  clientProfileId: "",
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
      protocolKind: blankToNull(filters.protocolKind),
      clientProfileId: blankToNull(filters.clientProfileId),
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
      generatedAtLabel: "--",
      freshnessLabel: "尚未加载",
      aggregateLabel: "0 行",
      rows: [],
    };
  }

  return {
    generatedAtLabel: formatTime(workspace.generatedAtMs),
    freshnessLabel: formatFreshness(workspace),
    aggregateLabel: `${workspace.aggregate.returnedRows}/${workspace.aggregate.totalRows} 行 · ${workspace.aggregate.runningRows} 运行中`,
    rows: workspace.rows.map((row) => buildRowView(row, workspace.window)),
  };
}

export function buildRowView(row: ChannelStatusRow, window: ChannelWindow): ChannelStatusRowView {
  const latest = row.latest;
  const selected = row.selectedWindow;
  const runningExecutionId = row.running?.executionId ?? null;
  const currentOutcome = selected.latestOutcome;
  const currentTone: StatusTone = row.monitor.enabled
    ? runningExecutionId
      ? "running"
      : currentOutcome
    : "disabled";
  const availabilityPercent = bpsToPercent(selected.effectiveAvailabilityBps);
  const strictAvailabilityPercent = bpsToPercent(selected.strictAvailabilityBps);

  return {
    rowKey: row.rowKey,
    monitorId: row.monitor.id,
    monitorName: row.monitor.name,
    targetName: row.target.stationKeyName ?? row.target.stationName ?? row.monitor.name,
    stationName: row.target.stationName ?? row.target.stationId,
    keyName: row.target.stationKeyName,
    enabled: row.monitor.enabled,
    protocolLabel: normalizeProtocolLabel(row.monitor.protocolKind),
    profileLabel: `${row.monitor.clientProfileId}@${row.monitor.clientProfileVersion}`,
    modelLabel: formatModelLabel(row.monitor.primaryModel, row.monitor.fallbackModels),
    currentTone,
    currentLabel: statusLabel(currentTone),
    currentReason: latest?.terminalReason ?? latest?.failureKind ?? null,
    runningExecutionId,
    latestExecutionId: latest?.executionId ?? null,
    availabilityPercent,
    strictAvailabilityPercent,
    availabilityLabel: formatAvailability(availabilityPercent),
    latencyMs: latest?.latencyMs ?? null,
    latencyLabel: formatLatency(latest?.latencyMs ?? null),
    lastCheckedAtMs: selected.latestCheckedAtMs,
    lastCheckedLabel: formatTime(selected.latestCheckedAtMs),
    attemptsLabel: latest ? `${latest.attemptCount} 次` : "--",
    fallbackLabel: latest?.usedFallback ? `fallback · ${latest.effectiveModel ?? "未知模型"}` : latest?.effectiveModel ?? "primary",
    trend: buildTrend(row, window),
    dirty: selected.dirty || row.hourlyBuckets.some((bucket) => bucket.dirty) || row.dailyBuckets.some((bucket) => bucket.dirty),
    corrupt: selected.corrupt || row.hourlyBuckets.some((bucket) => bucket.corrupt) || row.dailyBuckets.some((bucket) => bucket.corrupt),
  };
}

export function buildTrend(row: ChannelStatusRow, window: ChannelWindow): TrendCellView[] {
  if (window === "recent") {
    return row.recent.map(recentPointToCell);
  }
  if (window === "last24h") {
    return row.hourlyBuckets.map(bucketToCell);
  }
  if (window === "last7d") {
    return row.dailyBuckets.slice(-7).map(bucketToCell);
  }
  return row.dailyBuckets.map(bucketToCell);
}

export function availabilityTone(value: number | null): AvailabilityTone {
  if (value === null) {
    return "muted";
  }
  if (value < 50) {
    return "danger";
  }
  if (value < 75) {
    return "warning";
  }
  return "success";
}

export function statusLabel(tone: StatusTone) {
  switch (tone) {
    case "available":
      return "可用";
    case "degraded":
      return "降级";
    case "unavailable":
      return "不可用";
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

function recentPointToCell(point: ChannelStatusRecentPoint, index: number): TrendCellView {
  const tone = outcomeToTrendTone(point.outcome);
  return {
    id: point.targetResultId || `${point.executionId}-${index}`,
    tone,
    label: outcomeLabel(point.outcome),
    title: [
      outcomeLabel(point.outcome),
      `时间：${formatTime(point.checkedAtMs)}`,
      `延迟：${formatLatency(point.latencyMs)}`,
      `尝试：${point.attemptCount}`,
      point.failureKind ? `失败分类：${point.failureKind}` : null,
      point.terminalReason ? `原因：${point.terminalReason}` : null,
    ].filter(Boolean).join("\n"),
    latencyMs: point.latencyMs,
    startMs: point.checkedAtMs,
    endMs: point.checkedAtMs,
    total: 1,
  };
}

function bucketToCell(bucket: ChannelStatusBucket): TrendCellView {
  const tone = bucket.corrupt ? "corrupt" : bucket.dirty ? "dirty" : bucketStateToTrendTone(bucket);
  const failureCounts = Object.entries(bucket.failureCounts)
    .map(([kind, count]) => `${kind}:${count}`)
    .join(", ");
  return {
    id: `${bucket.kind}-${bucket.startMs}`,
    tone,
    label: bucketStateLabel(bucket.state),
    title: [
      `${formatTime(bucket.startMs)} - ${formatTime(bucket.endMs)}`,
      `状态：${bucketStateLabel(bucket.state)}`,
      `样本：${bucket.counts.total} · 可用 ${bucket.counts.available} · 降级 ${bucket.counts.degraded} · 不可用 ${bucket.counts.unavailable} · 跳过 ${bucket.counts.skipped}`,
      `可用率：${formatAvailability(bpsToPercent(bucket.effectiveAvailabilityBps))}`,
      `P50/P95：${formatLatency(bucket.p50LatencyMs)} / ${formatLatency(bucket.p95LatencyMs)}`,
      failureCounts ? `失败分类：${failureCounts}` : null,
      bucket.dirty ? "rollup 需要重建" : null,
      bucket.corrupt ? "rollup 数据异常" : null,
    ].filter(Boolean).join("\n"),
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

function outcomeLabel(outcome: ChannelStatusOutcome) {
  switch (outcome) {
    case "available":
      return "可用";
    case "degraded":
      return "降级";
    case "unavailable":
      return "不可用";
    case "skipped":
      return "跳过";
    default:
      return "缺失";
  }
}

function bucketStateLabel(state: ChannelStatusBucket["state"]) {
  switch (state) {
    case "available":
      return "可用";
    case "degraded":
      return "降级";
    case "unavailable":
      return "不可用";
    case "skipped_only":
      return "仅跳过";
    case "dirty":
      return "待重建";
    default:
      return "缺失";
  }
}

function statusSummaryLabel(outcome: ChannelStatusOutcome) {
  return outcomeLabel(outcome);
}

function formatFreshness(workspace: ChannelStatusWorkspace) {
  const parts = [
    `最新 ${formatTime(workspace.freshness.newestResultAtMs)}`,
    workspace.freshness.hasDirtyRollups ? "有待重建 rollup" : null,
    workspace.freshness.hasCorruptRollups ? "有异常 rollup" : null,
    workspace.freshness.runningExecutionCount > 0 ? `${workspace.freshness.runningExecutionCount} 个执行中` : null,
    `当前窗口 ${statusSummaryLabel(workspace.aggregate.unavailableRows > 0 ? "unavailable" : workspace.aggregate.degradedRows > 0 ? "degraded" : workspace.aggregate.availableRows > 0 ? "available" : "missing")}`,
  ];
  return parts.filter(Boolean).join(" · ");
}

function formatModelLabel(primary: string, fallbacks: string[]) {
  if (fallbacks.length === 0) {
    return primary;
  }
  return `${primary} +${fallbacks.length}`;
}

function normalizeProtocolLabel(value: string) {
  if (value === "openai_chat") return "OpenAI Chat";
  if (value === "openai_responses") return "OpenAI Responses";
  if (value === "anthropic_messages") return "Anthropic";
  if (value === "gemini_native") return "Gemini";
  if (value === "xai_grok") return "xAI/Grok";
  if (value === "generic_openai") return "OpenAI-compatible";
  return value;
}

function bpsToPercent(value: number | null) {
  return value === null ? null : value / 100;
}

function blankToNull(value: string) {
  const trimmed = value.trim();
  return trimmed.length === 0 ? null : trimmed;
}
