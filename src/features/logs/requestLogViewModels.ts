import { parseTimestampLikeDate } from "@/lib/time";
import type { RequestLog } from "@/lib/types/proxy";
import type { KeyPoolItem } from "@/lib/types/stationKeys";

export const requestLogFieldLabels = {
  reasoningEffort: "推理强度",
} as const;

const reasoningLabels: Record<string, string> = {
  none: "None",
  minimal: "Minimal",
  low: "Low",
  medium: "Medium",
  high: "High",
  xhigh: "XHigh",
  max: "Max",
};

const billingModeLabels: Record<string, string> = {
  token: "按量",
  per_request: "按次",
  image: "按量",
  video: "按量",
};

export function formatLogTime(value: string, includeDate = false) {
  const date = parseTimestampLikeDate(value);
  if (Number.isNaN(date.getTime())) return value;
  return includeDate
    ? date.toLocaleString("zh-CN", {
        year: "numeric",
        month: "2-digit",
        day: "2-digit",
        hour: "2-digit",
        minute: "2-digit",
        second: "2-digit",
        hour12: false,
      })
    : date.toLocaleTimeString("zh-CN", {
        hour: "2-digit",
        minute: "2-digit",
        second: "2-digit",
        hour12: false,
      });
}

export function formatKeyName(log: RequestLog, keyById: Map<string, KeyPoolItem>) {
  if (!log.stationKeyId) return "未选择";
  const key = keyById.get(log.stationKeyId);
  return key ? `${key.name} · ${key.apiKeyMasked}` : log.stationKeyId;
}

export function formatStationName(log: RequestLog, keyById: Map<string, KeyPoolItem>) {
  if (log.stationKeyId) {
    const key = keyById.get(log.stationKeyId);
    if (key) return `${key.stationName} · ${key.stationType}`;
  }
  return log.stationId ?? "未选择";
}

export function formatGroupName(log: RequestLog, keyById: Map<string, KeyPoolItem>) {
  const key = log.stationKeyId ? keyById.get(log.stationKeyId) : undefined;
  if (key?.groupName) return key.groupName;
  return log.groupBindingId ?? "未分组";
}

export function reasoningEffortLabel(value: string | null) {
  if (!value) return "-";
  return reasoningLabels[value] ?? value;
}

export function billingModeLabel(value: string | null) {
  if (!value) return "-";
  return billingModeLabels[value] ?? "-";
}

export function tokenBreakdown(log: RequestLog) {
  const rows = [
    { label: "输入", value: formatCount(log.promptTokens) },
    { label: "输出", value: formatCount(log.completionTokens) },
  ];
  if (log.cacheReadTokens != null && log.cacheReadTokens > 0) {
    rows.push({ label: "缓存读", value: formatCount(log.cacheReadTokens) });
  }
  if (log.cacheCreationTokens != null && log.cacheCreationTokens > 0) {
    rows.push({ label: "缓存写", value: formatCount(log.cacheCreationTokens) });
  }
  return rows;
}

export function latencyBreakdown(log: RequestLog) {
  return [
    { label: "首字", value: formatDuration(log.firstTokenMs) },
    { label: "总耗时", value: formatDuration(log.durationMs) },
  ];
}

export function formatCompactTokenCount(value: number | null) {
  if (value == null) return "-";
  if (Math.abs(value) < 10_000) return value.toLocaleString("en-US");
  return new Intl.NumberFormat("en-US", {
    notation: "compact",
    maximumFractionDigits: 1,
  }).format(value);
}

export function formatRequestTokenCount(log: RequestLog, value: number | null) {
  if (value == null && log.status === "failed") return "0";
  return formatCompactTokenCount(value);
}

export function paginateRequestLogs(logs: RequestLog[], page: number, pageSize: number) {
  const safePageSize = Math.max(1, pageSize);
  const totalPages = Math.max(1, Math.ceil(logs.length / safePageSize));
  const safePage = Math.min(Math.max(1, page), totalPages);
  const startOffset = (safePage - 1) * safePageSize;
  const pageLogs = logs.slice(startOffset, startOffset + safePageSize);

  return {
    logs: pageLogs,
    page: safePage,
    pageSize: safePageSize,
    totalPages,
    startIndex: pageLogs.length === 0 ? 0 : startOffset + 1,
    endIndex: startOffset + pageLogs.length,
    totalCount: logs.length,
  };
}

export function formatTokenTotal(log: RequestLog) {
  if (log.totalTokens == null) {
    return log.costStatus === "unknown_usage" ? "用量未知" : "暂无";
  }
  return `${log.totalTokens.toLocaleString("zh-CN")} t`;
}

export function formatRequestCost(log: RequestLog) {
  if (log.estimatedTotalCost == null) return pricingStatusLabel(log.costStatus);
  return `$${log.estimatedTotalCost.toFixed(6)}`;
}

export function pricingStatusLabel(value: string | null | undefined) {
  if (value === "priced") return "已计价";
  if (value === "base_price_only") return "基准价估算";
  if (value === "missing_rate") return "缺倍率";
  if (value === "missing_model_price") return "缺模型基准价";
  if (value === "unsupported_billing_mode") return "不支持计费";
  if (value === "legacy_estimate") return "旧估算";
  if (value === "usage_only" || value === "unpriced") return "未定价";
  if (value === "unknown_usage") return "用量未知";
  return "未知";
}

export function pricingStatusTone(value: string | null | undefined) {
  if (value === "priced") return "healthy";
  if (value === "base_price_only" || value === "legacy_estimate") return "warning";
  if (
    value === "missing_rate" ||
    value === "missing_model_price" ||
    value === "unsupported_billing_mode" ||
    value === "unpriced"
  ) return "error";
  return "info";
}

export function normalizationLabel(value: string | null | undefined) {
  if (!value) return "未知";
  if (value === "complete") return "完整";
  if (value === "group_rate_only") return "仅倍率";
  if (value === "expired") return "已过期";
  return pricingStatusLabel(value) === "未知" ? value : pricingStatusLabel(value);
}

export function statusFallback(value: string | null | undefined) {
  return value ?? "未知";
}

function formatCount(value: number | null) {
  return value == null ? "-" : value.toLocaleString("zh-CN");
}

function formatDuration(value: number | null) {
  if (value == null) return "-";
  return value >= 1000 ? `${(value / 1000).toFixed(2)}s` : `${value}ms`;
}
