import type { LocalRoutingCandidateRow, RouteDecisionSummary } from "../../lib/types/localRouting";

export type CooldownDisplay = {
  active: boolean;
  label: string;
  remainingSeconds: number | null;
};

export type LatestDecisionDisplay = {
  title: string;
  badge: "历史记录" | "已选中" | "已回退" | "失败" | "不可用" | null;
  tone: "neutral" | "healthy" | "warning" | "error";
  decidedAt: string | null;
};

const previewRejectReasonLabels: Record<string, string> = {
  asset_unavailable: "候选不可用",
  routing_group_mismatch: "分组不匹配",
  capability_mismatch: "接口能力不匹配",
  model_mismatch: "模型不匹配",
  health_blocked: "健康状态阻止路由",
  balance_depleted: "余额不足",
  no_multiplier_evidence: "缺少倍率证据",
  multiplier_evidence_invalid: "倍率证据无效",
  multiplier_evidence_negative: "倍率不能为负数",
  multiplier_evidence_expired: "倍率证据已过期",
  multiplier_evidence_unbound_group: "倍率未绑定分组",
  multiplier_evidence_low_confidence: "费率可信度不足",
  multiplier_over_ceiling: "超过倍率上限",
  routing_multiplier_limit_not_configured: "倍率上限未设置",
};

const balanceStatusLabels: Record<string, string> = {
  normal: "正常",
  low: "偏低",
  depleted: "已耗尽",
  insufficient: "不足",
  blocked: "已阻断",
};

export function buildCooldownDisplay(
  healthState: LocalRoutingCandidateRow["healthState"],
  cooldownUntilMs: number | null,
  nowMs: number,
): CooldownDisplay {
  if (healthState !== "cooldown") {
    return { active: false, label: "无", remainingSeconds: null };
  }
  if (cooldownUntilMs == null || !Number.isFinite(cooldownUntilMs)) {
    return { active: true, label: "冷却中", remainingSeconds: null };
  }

  const remainingSeconds = Math.ceil((cooldownUntilMs - nowMs) / 1000);
  if (remainingSeconds <= 0) {
    return { active: true, label: "即将结束", remainingSeconds: 0 };
  }

  return {
    active: true,
    label: formatDuration(remainingSeconds),
    remainingSeconds,
  };
}

export function buildLatestDecisionDisplay(
  proxyRunning: boolean,
  latestDecision: RouteDecisionSummary | null,
): LatestDecisionDisplay {
  if (!latestDecision) {
    return { title: "尚无路由记录", badge: null, tone: "neutral", decidedAt: null };
  }

  if (!proxyRunning) {
    return {
      title: latestDecision.selectedStationName ?? "未选中密钥",
      badge: "历史记录",
      tone: "neutral",
      decidedAt: latestDecision.decidedAt,
    };
  }

  const badgeByStatus: Record<RouteDecisionSummary["status"], LatestDecisionDisplay["badge"]> = {
    selected: "已选中",
    fallback: "已回退",
    failed: "失败",
    unavailable: "不可用",
  };
  const toneByStatus: Record<RouteDecisionSummary["status"], LatestDecisionDisplay["tone"]> = {
    selected: "healthy",
    fallback: "warning",
    failed: "error",
    unavailable: "warning",
  };

  return {
    title: latestDecision.selectedStationName ?? "未选中密钥",
    badge: badgeByStatus[latestDecision.status],
    tone: toneByStatus[latestDecision.status],
    decidedAt: latestDecision.decidedAt,
  };
}

export function formatPreviewRejectReason(code: string) {
  return previewRejectReasonLabels[code] ?? "当前请求条件不满足";
}

export function formatMultiplierSource(source: string | null, confidence: number | null) {
  if (!source) return "暂无可信来源";
  const sourceLabel = source === "sub2api_groups_rates" ? "Sub2API 分组费率" : source;
  return confidence == null
    ? sourceLabel
    : `${sourceLabel} · 可信度 ${(confidence * 100).toFixed(0)}%`;
}

export function formatBalanceStatus(value: string | null) {
  return balanceStatusLabels[value ?? ""] ?? "未知";
}

export function formatRoutingDecisionTime(value: string | null) {
  if (!value) return "暂无决策时间";
  const date = parseTimestampLikeDate(value);
  if (Number.isNaN(date.getTime())) return "决策时间异常";
  return date.toLocaleString("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  });
}

function formatDuration(totalSeconds: number) {
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  if (hours > 0) {
    return `${hours}:${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
  }
  return `${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
}

function parseTimestampLikeDate(value: string) {
  const numeric = Number(value);
  return Number.isFinite(numeric) && numeric > 1000000000000 ? new Date(numeric) : new Date(value);
}
