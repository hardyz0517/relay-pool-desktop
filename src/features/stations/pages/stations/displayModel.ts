import { parseTimestampLikeDate } from "@/lib/time";
import type { StationAssetRow } from "../../stationAssetViewModels";

export function stationAvatarLabel(name: string) {
  const trimmed = name.trim();
  return trimmed ? Array.from(trimmed)[0] : "?";
}

export function formatStationDisplayUrl(baseUrl: string) {
  try {
    const url = new URL(baseUrl);
    return `${url.protocol}//${url.host}`;
  } catch {
    return baseUrl.replace(/\/+$/, "");
  }
}

export function formatStationBalanceParts(row: StationAssetRow) {
  const value = row.latestBalance?.value ?? row.station.balanceCny;
  if (value == null) {
    return { amount: "未采集", currency: "" };
  }
  return {
    amount: value.toFixed(2),
    currency: row.latestBalance?.currency ?? "CNY",
  };
}

export function formatRelativeTime(value: string | null) {
  if (!value) {
    return "未采集";
  }
  const date = parseTimestampLikeDate(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  const diffMs = Math.max(0, Date.now() - date.getTime());
  const minute = 60 * 1000;
  const hour = 60 * minute;
  const day = 24 * hour;
  if (diffMs < minute) {
    return "刚刚";
  }
  if (diffMs < hour) {
    return `${Math.floor(diffMs / minute)} 分钟前`;
  }
  if (diffMs < day) {
    return `${Math.floor(diffMs / hour)} 小时前`;
  }
  return `${Math.floor(diffMs / day)} 天前`;
}

export function stationIssueTagClassName(tone: "info" | "warning" | "error" | "disabled") {
  if (tone === "error") return "border-danger-border bg-danger-surface text-danger-foreground";
  if (tone === "warning") return "border-warning-border bg-warning-surface text-warning-foreground";
  if (tone === "disabled") return "border-border bg-muted text-muted-foreground";
  return "border-info-border bg-info-surface text-info-foreground";
}

export function formatNullableTime(value: string | null) {
  if (!value) {
    return "未记录";
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

export function formatMultiplier(value: number | null | undefined) {
  return typeof value === "number" && Number.isFinite(value) ? `${value.toFixed(2)}x` : "-";
}

export function collectorTaskTypeLabel(value: string) {
  if (value === "detect") return "探测";
  if (value === "balance") return "余额";
  if (value === "groups") return "分组";
  if (value === "models") return "模型";
  if (value === "full") return "完整";
  return value;
}

export function collectorRunStatusLabel(status: string) {
  if (status === "success") return "成功";
  if (status === "failed") return "失败";
  if (status === "manual_required") return "需要登录";
  if (status === "running") return "运行中";
  if (status === "partial") return "部分完成";
  return status;
}

export function groupBindingStatusLabel(status: string) {
  if (status === "available") return "可用";
  if (status === "missing") return "缺失";
  if (status === "manual") return "手动";
  return status;
}
