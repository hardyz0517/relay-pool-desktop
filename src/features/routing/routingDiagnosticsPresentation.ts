import type { RoutingWorkspaceCandidate } from "@/lib/types/routing";

export type RoutingCandidateDiagnosticsDisplay = {
  score: string;
  qualityMetadata: string;
  sourceReliability: string;
  recentWindow: string;
  historicalWindow: string;
  latencySummary: string;
  sampleCounts: string;
  idleRealRoute: string;
  circuitState: string;
  circuitDetail: string;
  halfOpenLease: string;
  scoreGate: string;
};

export function buildRoutingCandidateDiagnosticsDisplay(
  candidate: RoutingWorkspaceCandidate,
): RoutingCandidateDiagnosticsDisplay | null {
  const diagnostics = candidate.diagnostics;
  if (!diagnostics) return null;

  const quality = diagnostics.quality;
  const circuit = diagnostics.circuit;
  return {
    score: `有效分 ${formatScore(diagnostics.effectiveScore)} · 基础分 ${formatScore(diagnostics.baseScore)}`,
    qualityMetadata: quality
      ? `质量投影 r${quality.qualityRevision} · 策略 r${quality.qualityPolicyRevision} · ${formatQualityBasis(quality.qualityBasis)} · ${quality.algorithmVersion || "算法版本未知"}`
      : "质量摘要暂不可用",
    sourceReliability: quality
      ? `真实流量 ${formatPercent(quality.realReliabilityBasisPoints)} × ${formatEligibleWeight(quality.realSourceEligible, quality.effectiveRealWeightBasisPoints)} · 监控 ${formatPercent(quality.monitoringReliabilityBasisPoints)} × ${formatEligibleWeight(quality.monitoringSourceEligible, quality.effectiveMonitoringWeightBasisPoints)}（${formatMonitoringSourceStatus(quality.monitoringSourceStatus)}）`
      : "来源可靠性暂不可用",
    recentWindow: quality
      ? `近期：${formatSourceWindow("真实", quality.realSource.eligible, quality.realSource.recent, quality.recentMinimumSamples)} · ${formatSourceWindow("监控", quality.monitoringSource.eligible, quality.monitoringSource.recent, quality.recentMinimumSamples)}`
      : "近期样本暂不可用",
    historicalWindow: quality
      ? `历史：${formatSourceWindow("真实", quality.realSource.eligible, quality.realSource.historical, quality.historicalMinimumSamples)} · ${formatSourceWindow("监控", quality.monitoringSource.eligible, quality.monitoringSource.historical, quality.historicalMinimumSamples)}`
      : "历史样本暂不可用",
    latencySummary: quality
      ? `响应延迟 ${formatLatency(quality.latency.blendedWeightedLatencyMs)} · 来源权重 实际 ${formatEligibleWeight(quality.realSourceEligible, quality.effectiveRealWeightBasisPoints)} / 监控 ${formatEligibleWeight(quality.monitoringSourceEligible, quality.effectiveMonitoringWeightBasisPoints)} · 近期 ${formatLatencyWindow(quality.latency.recentSampleCount, quality.recentMinimumSamples, quality.latency.recentEffectiveWeight, quality.latency.recentWeightedLatencyMs, quality.latency.recentMinimumMet)} · 历史 ${formatLatencyWindow(quality.latency.historicalSampleCount, quality.historicalMinimumSamples, quality.latency.historicalEffectiveWeight, quality.latency.historicalWeightedLatencyMs, quality.latency.historicalMinimumMet)}`
      : "响应延迟暂不可用",
    sampleCounts: quality
      ? `质量样本 ${quality.canonicalSampleCount} · 真实尝试 raw ${diagnostics.attempts.rawRealAttemptCount} / 去重请求 ${diagnostics.attempts.deduplicatedRealRequestCount}`
      : `真实尝试 raw ${diagnostics.attempts.rawRealAttemptCount} / 去重请求 ${diagnostics.attempts.deduplicatedRealRequestCount}`,
    idleRealRoute: quality
      ? formatIdleRealRoute(quality.idleRealRouteSample, quality.lastRealRouteSampleAtMs)
      : "真实流量闲置状态未知",
    circuitState: formatCircuitState(circuit.state, circuit.cooldownRemainingMs),
    circuitDetail: `回退层级 ${circuit.reopenLevel} · 连续失败 ${circuit.consecutiveFailures ?? "-"} · 状态 revision ${circuit.stateRevision ?? "未持久化"}`,
    halfOpenLease: circuit.halfOpenLeaseInFlight
      ? `Half-Open lease 已占用${formatLeaseExpiry(circuit.halfOpenLeaseExpiresAtMs)}`
      : "Half-Open lease 空闲",
    scoreGate: formatScoreGate(
      circuit.scoreGateStatus,
      circuit.bestClosedEffectiveScore,
    ),
  };
}

function formatMonitoringSourceStatus(status: string) {
  const labels: Record<string, string> = {
    comparable: "已按整把密钥计入",
    no_evidence: "暂无监控证据",
    incomparable: "已按整把密钥计入",
    weight_zero: "来源权重为 0",
    disabled: "来源未启用",
  };
  return labels[status] ?? "来源状态未知";
}

function formatSourceWindow(
  label: string,
  eligible: boolean,
  window: {
    sampleCount: number;
    effectiveWeight: number;
    reliabilityBasisPoints: number;
    minimumMet: boolean;
  },
  minimumSamples: number,
) {
  if (!eligible) return `${label}不参与`;
  const basis = window.minimumMet ? "实测" : "乐观值";
  return `${label} ${window.sampleCount}/${minimumSamples} · mass ${formatMass(window.effectiveWeight)} · ${formatPercent(window.reliabilityBasisPoints)}（${basis}）`;
}

function formatLatencyWindow(
  sampleCount: number,
  minimumSamples: number,
  effectiveWeight: number,
  weightedLatencyMs: number,
  minimumMet: boolean,
) {
  return `${sampleCount}/${minimumSamples} · mass ${formatMass(effectiveWeight)} · ${formatLatency(weightedLatencyMs)}（${minimumMet ? "实测" : "乐观值"}）`;
}

function formatScore(score: number | null) {
  return score == null ? "-" : (score / 100).toFixed(2);
}

function formatPercent(basisPoints: number) {
  return `${(basisPoints / 100).toFixed(2)}%`;
}

function formatEligibleWeight(eligible: boolean, basisPoints: number) {
  return eligible ? formatPercent(basisPoints) : "不参与";
}

function formatMass(basisPoints: number) {
  return (basisPoints / 1_000_000).toLocaleString("zh-CN", {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  });
}

function formatLatency(milliseconds: number) {
  if (milliseconds >= 1_000) {
    return `${(milliseconds / 1_000).toLocaleString("zh-CN", {
      minimumFractionDigits: 0,
      maximumFractionDigits: 2,
    })} 秒`;
  }
  return `${milliseconds} 毫秒`;
}

function formatQualityBasis(basis: string) {
  const labels: Record<string, string> = {
    Observed: "实测",
    OptimisticInsufficientSamples: "样本不足乐观值",
    QualityUnavailable: "质量不可用",
    legacy: "兼容摘要",
  };
  return labels[basis] ?? (basis || "basis 未知");
}

function formatIdleRealRoute(value: string, lastSampleAtMs: number | null) {
  if (value === "true") return "真实流量已闲置 24 小时以上";
  if (value === "false") {
    return lastSampleAtMs == null
      ? "真实流量 24 小时内有样本"
      : `真实流量 24 小时内有样本 · ${formatTimestamp(lastSampleAtMs)}`;
  }
  return "真实流量闲置状态未知";
}

function formatCircuitState(state: string, cooldownRemainingMs: number | null) {
  if (state === "open") {
    return cooldownRemainingMs != null && cooldownRemainingMs > 0
      ? `Open · 冷却剩余 ${formatDurationMs(cooldownRemainingMs)}`
      : "Open · 冷却已结束";
  }
  if (state === "half_open") return "Half-Open";
  return "Closed";
}

function formatScoreGate(status: string, bestClosedScore: number | null) {
  const baseline = bestClosedScore == null ? "无同层 Closed 基线" : `同层 Closed 最佳 ${formatScore(bestClosedScore)}`;
  const labels: Record<string, string> = {
    not_applicable: "评分门不适用",
    waiting_cooldown: "评分门等待冷却结束",
    passed: "评分门通过",
    denied: "评分门未通过",
    unavailable: "评分门暂不可评估",
  };
  return `${labels[status] ?? "评分门状态未知"} · ${baseline}`;
}

function formatLeaseExpiry(expiresAtMs: number | null) {
  return expiresAtMs == null ? "" : ` · 到期 ${formatTimestamp(expiresAtMs)}`;
}

function formatTimestamp(value: number) {
  return new Date(value).toLocaleString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  });
}

function formatDurationMs(value: number) {
  const seconds = Math.max(0, Math.ceil(value / 1_000));
  if (seconds < 60) return `${seconds} 秒`;
  const minutes = Math.ceil(seconds / 60);
  if (minutes < 60) return `${minutes} 分钟`;
  return `${Math.ceil(minutes / 60)} 小时`;
}
