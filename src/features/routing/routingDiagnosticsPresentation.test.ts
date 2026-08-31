import { describe, expect, it } from "vitest";

import type { RoutingWorkspaceCandidate } from "@/lib/types/routing";
import { buildRoutingCandidateDiagnosticsDisplay } from "./routingDiagnosticsPresentation";

describe("routing diagnostics presentation", () => {
  it("stays explicit when the V3 diagnostics read model is absent", () => {
    const candidate = {} as RoutingWorkspaceCandidate;
    expect(buildRoutingCandidateDiagnosticsDisplay(candidate)).toBeNull();
  });

  it("renders unavailable sources and unknown idle state without inventing facts", () => {
    const candidate = {
      diagnostics: {
        effectiveScore: null,
        baseScore: null,
        quality: {
          qualityRevision: 9,
          qualityPolicyRevision: 2,
          algorithmVersion: "routing_quality_v3",
          qualityBasis: "QualityUnavailable",
          qualityUnavailable: true,
          canonicalSampleCount: 0,
          realReliabilityBasisPoints: 9_500,
          monitoringReliabilityBasisPoints: 9_500,
          effectiveRealWeightBasisPoints: 0,
          effectiveMonitoringWeightBasisPoints: 0,
          realSourceEligible: false,
          monitoringSourceEligible: false,
          monitoringSourceStatus: "incomparable",
          recentSampleCount: 0,
          recentEffectiveMassBasisPoints: 0,
          recentMinimumSamples: 5,
          historicalSampleCount: 0,
          historicalEffectiveMassBasisPoints: 0,
          historicalMinimumSamples: 15,
          realSource: {
            eligible: false,
            effectiveWeightBasisPoints: 0,
            recent: { sampleCount: 0, effectiveWeight: 0, successWeight: 0, failureWeight: 0, reliabilityBasisPoints: 9_500, minimumMet: false },
            historical: { sampleCount: 0, effectiveWeight: 0, successWeight: 0, failureWeight: 0, reliabilityBasisPoints: 9_500, minimumMet: false },
            blendedReliabilityBasisPoints: 9_500,
          },
          monitoringSource: {
            eligible: false,
            effectiveWeightBasisPoints: 0,
            recent: { sampleCount: 0, effectiveWeight: 0, successWeight: 0, failureWeight: 0, reliabilityBasisPoints: 9_500, minimumMet: false },
            historical: { sampleCount: 0, effectiveWeight: 0, successWeight: 0, failureWeight: 0, reliabilityBasisPoints: 9_500, minimumMet: false },
            blendedReliabilityBasisPoints: 9_500,
          },
          latency: {
            recentSampleCount: 0,
            recentEffectiveWeight: 0,
            recentWeightedLatencyMs: 2_500,
            recentMinimumMet: false,
            historicalSampleCount: 0,
            historicalEffectiveWeight: 0,
            historicalWeightedLatencyMs: 2_500,
            historicalMinimumMet: false,
            blendedWeightedLatencyMs: 2_500,
          },
          idleRealRouteSample: "unknown",
          lastRealRouteSampleAtMs: null,
        },
        attempts: {
          rawRealAttemptCount: 0,
          deduplicatedRealRequestCount: 0,
        },
        circuit: {
          state: "open",
          stateRevision: 4,
          lifecycleRevision: 2,
          consecutiveFailures: 3,
          reopenLevel: 1,
          cooldownUntilMs: 61_000,
          cooldownRemainingMs: 61_000,
          halfOpenLeaseInFlight: false,
          halfOpenLeaseExpiresAtMs: null,
          recoverySuccesses: null,
          scoreGateStatus: "waiting_cooldown",
          scoreGateReason: "cooldown_active",
          bestClosedEffectiveScore: null,
        },
      },
    } as RoutingWorkspaceCandidate;

    const display = buildRoutingCandidateDiagnosticsDisplay(candidate);
    expect(display).toMatchObject({
      score: "有效分 - · 基础分 -",
      qualityMetadata: "质量投影 r9 · 策略 r2 · 质量不可用 · routing_quality_v3",
      sourceReliability: "真实流量 95.00% × 不参与 · 监控 95.00% × 不参与（已按整把密钥计入）",
      recentWindow: "近期：真实不参与 · 监控不参与",
      historicalWindow: "历史：真实不参与 · 监控不参与",
      latencySummary: "响应延迟 2.5 秒 · 来源权重 实际 不参与 / 监控 不参与 · 近期 0/5 · mass 0.00 · 2.5 秒（乐观值） · 历史 0/15 · mass 0.00 · 2.5 秒（乐观值）",
      idleRealRoute: "真实流量闲置状态未知",
      circuitState: "Open · 冷却剩余 2 分钟",
      scoreGate: "评分门等待冷却结束 · 无同层 Closed 基线",
    });
  });
});
