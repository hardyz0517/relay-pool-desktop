// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, describe, expect, it } from "vitest";

import type { RoutingProtectionStatus, RoutingWorkspaceSnapshot } from "@/lib/types/routing";
import { RoutingStatusDiagnosticsPanel } from "./RoutingStatusDiagnosticsPanel";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

afterEach(() => {
  document.body.innerHTML = "";
});

function snapshotFixture(): RoutingWorkspaceSnapshot {
  return {
    readModelVersion: "routing_workspace_v3",
    generatedAtMs: 1,
    policyConfig: {} as RoutingWorkspaceSnapshot["policyConfig"],
    previewPolicyVersion: "v3",
    maxRateMultiplier: null,
    routingGroupFilter: "all_groups",
    capacityMode: "snapshot_only",
    page: { cursor: null, limit: 50, total: 1, returned: 1, nextCursor: null } as unknown as RoutingWorkspaceSnapshot["page"],
    candidates: [
      {
        stationKeyId: "key-1",
        stationId: "station-1",
        stationName: "Primary",
        keyName: "key-a",
        endpointRevision: 1,
        priority: 1,
        schedulable: true,
        healthState: "ready",
        participationStatus: "excluded",
        participationReason: "circuit_half_open_lease_occupied",
        score: 0.9,
        scoreStatus: "scored",
        plannerExclusionCodes: [],
        assessmentSnapshotId: null,
        assessmentDurableRevision: null,
        assessmentRequestContextFingerprint: null,
        scoreDetails: null,
        diagnostics: {
          effectiveScore: 9_300,
          baseScore: 9_100,
          quality: {
            qualityRevision: 42,
            qualityPolicyRevision: 3,
            algorithmVersion: "routing_quality_v3",
            qualityBasis: "Observed",
            qualityUnavailable: false,
            canonicalSampleCount: 8,
            realReliabilityBasisPoints: 9_600,
            monitoringReliabilityBasisPoints: 9_000,
            effectiveRealWeightBasisPoints: 7_000,
            effectiveMonitoringWeightBasisPoints: 3_000,
            realSourceEligible: true,
            monitoringSourceEligible: true,
            monitoringSourceStatus: "comparable",
            recentSampleCount: 4,
            recentEffectiveMassBasisPoints: 2_500_000,
            recentMinimumSamples: 5,
            historicalSampleCount: 16,
            historicalEffectiveMassBasisPoints: 5_000_000,
            historicalMinimumSamples: 15,
            realSource: {
              eligible: true,
              effectiveWeightBasisPoints: 7_000,
              recent: { sampleCount: 4, effectiveWeight: 2_500_000, successWeight: 2_400_000, failureWeight: 100_000, reliabilityBasisPoints: 9_500, minimumMet: false },
              historical: { sampleCount: 16, effectiveWeight: 5_000_000, successWeight: 4_800_000, failureWeight: 200_000, reliabilityBasisPoints: 9_600, minimumMet: true },
              blendedReliabilityBasisPoints: 9_560,
            },
            monitoringSource: {
              eligible: true,
              effectiveWeightBasisPoints: 3_000,
              recent: { sampleCount: 5, effectiveWeight: 3_000_000, successWeight: 2_700_000, failureWeight: 300_000, reliabilityBasisPoints: 9_000, minimumMet: true },
              historical: { sampleCount: 15, effectiveWeight: 4_000_000, successWeight: 3_600_000, failureWeight: 400_000, reliabilityBasisPoints: 9_000, minimumMet: true },
              blendedReliabilityBasisPoints: 9_000,
            },
            latency: {
              recentSampleCount: 4,
              recentEffectiveWeight: 2_500_000,
              recentWeightedLatencyMs: 2_500,
              recentMinimumMet: false,
              historicalSampleCount: 16,
              historicalEffectiveWeight: 5_000_000,
              historicalWeightedLatencyMs: 1_800,
              historicalMinimumMet: true,
              blendedWeightedLatencyMs: 2_100,
            },
            idleRealRouteSample: "true",
            lastRealRouteSampleAtMs: null,
          },
          attempts: {
            rawRealAttemptCount: 12,
            deduplicatedRealRequestCount: 8,
          },
          circuit: {
            state: "half_open",
            stateRevision: 7,
            lifecycleRevision: 3,
            policyRevision: 1,
            persistenceStatus: "available",
            stateRowPresent: true,
            consecutiveFailures: null,
            reopenLevel: 2,
            cooldownUntilMs: null,
            cooldownRemainingMs: null,
            halfOpenLeaseInFlight: true,
            halfOpenLeaseExpiresAtMs: null,
            recoverySuccesses: 1,
            scoreGateStatus: "passed",
            scoreGateReason: "half_open_lease_in_flight",
            bestClosedEffectiveScore: 8_800,
          },
        },
        group: null,
        multiplier: { status: "known", multiplier: 1, selectedSource: "test", ceilingRejected: false, reason: "" },
        capabilitySummary: { chatCompletions: true, responses: true, embeddings: false, stream: true, tools: false, vision: false, reasoning: false },
        capabilityVerdicts: { protocol: "ok", model: "ok", stream: "ok", tools: "ok", vision: "ok", reasoning: "ok", rejectionSubjects: [] },
        priceBasis: "test",
        pricing: { basis: "priced", comparisonValue: 1, reason: null, currency: "USD", unit: "request", estimatedInputPrice: null, estimatedOutputPrice: null, statusLabel: "已定价", sourceChain: [], observedAt: null, confidence: 1 },
        balanceStatus: "normal",
        balanceValue: null,
        balanceCurrency: null,
        capacity: { mode: "snapshot_only", status: "available", maxConcurrency: 2, inFlight: 0, acquired: false },
        sourceRefs: { stationKeyId: "key-1", stationId: "station-1", endpointRevision: 1, snapshotId: "snapshot", factVersionVector: "facts", projectorVersion: "projector" },
        hardRejectionCodes: [],
      },
    ],
    aggregates: {
      totalCandidates: 1,
      schedulableCandidates: 1,
      eligibleCandidates: 0,
      conditionallyEligibleCandidates: 0,
      excludedCandidates: 1,
      unavailableCandidates: 0,
      closedCircuits: 0,
      openCircuits: 0,
      halfOpenCircuits: 1,
      persistenceUnavailableCircuits: 0,
    },
    circuitReadModelStatus: "available",
    circuitReadModelCode: null,
    circuitRevision: {
      processGateRevision: 0,
      persistenceHealthRevision: 0,
      stateFingerprint: "test",
    },
    readModelStatus: "available",
    plannerEvaluation: "available",
    plannerEvaluationCode: null,
    availabilityStatus: "available",
    runtimeGenerationId: "rg-active-42",
    policyRevision: 7,
    qualityRevision: 42,
    healthRevision: 39,
    qualityProjectionBacklog: 3,
    qualityProjectionLagSeconds: 65,
    qualityStale: true,
  };
}

function protectionFixture(): RoutingProtectionStatus {
  return {
    statusVersion: "routing_protection_status_v1",
    generatedAtMs: 1,
    readModelStatus: "available",
    timeouts: null,
    entries: [
      {
        scope: "station_key:key-1",
        scopeKind: "station_key",
        state: "open",
        explanationKey: "routing.protection.open",
        persistenceKind: "durable",
        cooldownUntilMs: null,
        cooldownRemainingMs: null,
        recentFailureCode: "429",
        diagnosticReason: null,
        updatedAtMs: 1,
        detailAvailable: true,
      },
      {
        scope: "local-capacity",
        scopeKind: "local_capacity",
        state: "open",
        explanationKey: "routing.protection.open",
        persistenceKind: "runtime_capacity",
        cooldownUntilMs: null,
        cooldownRemainingMs: null,
        recentFailureCode: "capacity_exhausted",
        diagnosticReason: "capacity_exhausted",
        updatedAtMs: 1,
        detailAvailable: true,
      },
    ],
  };
}

describe("routing Key circuit diagnostics", () => {
  it("is hidden outside developer mode", () => {
    const host = document.createElement("div");
    document.body.append(host);
    const root = createRoot(host);
    act(() => {
      root.render(
        <RoutingStatusDiagnosticsPanel
          snapshot={null}
          runtimeOverlay={null}
          decisions={null}
          protectionStatus={null}
          loading={false}
          developerModeEnabled={false}
        />,
      );
    });
    expect(host.textContent).toBe("");
    act(() => root.unmount());
  });

  it("renders V3 scoring and circuit facts without capacity-domain identities", () => {
    const host = document.createElement("div");
    document.body.append(host);
    const root = createRoot(host);
    act(() => {
      root.render(
        <RoutingStatusDiagnosticsPanel
          snapshot={snapshotFixture()}
          runtimeOverlay={null}
          decisions={null}
          protectionStatus={protectionFixture()}
          loading={false}
          developerModeEnabled
        />,
      );
    });

    expect(host.textContent).toContain("密钥熔断诊断");
    expect(host.textContent).toContain("可参与候选");
    expect(host.textContent).toContain("0/1");
    expect(host.textContent).toContain("半开探测进行中");
    expect(host.textContent).toContain("运行代际：rg-active-42");
    expect(host.textContent).toContain("revision：策略 r7 · 质量 r42 · 熔断 gate r0 / durable r0");
    expect(host.textContent).toContain("质量投影：陈旧 · 积压 3");
    expect(host.textContent).toContain("投影延迟：1 分 5 秒");
    expect(host.textContent).toContain("有效分 93.00 · 基础分 91.00");
    expect(host.textContent).toContain("质量投影 r42 · 策略 r3 · 实测 · routing_quality_v3");
    expect(host.textContent).toContain("真实流量 96.00% × 70.00% · 监控 90.00% × 30.00%（已按整把密钥计入）");
    expect(host.textContent).toContain("存在可用密钥");
    expect(host.textContent).toContain("容量可用 · 本地在途 0/2");
    expect(host.textContent).toContain("近期：真实 4/5 · mass 2.50 · 95.00%（乐观值）");
    expect(host.textContent).toContain("响应延迟 2.1 秒");
    expect(host.textContent).toContain("历史 16/15 · mass 5.00");
    expect(host.textContent).toContain("质量样本 8 · 真实尝试 raw 12 / 去重请求 8");
    expect(host.textContent).toContain("真实流量已闲置 24 小时以上");
    expect(host.textContent).toContain("Half-Open lease 已占用");
    expect(host.textContent).toContain("回退层级 2");
    expect(host.textContent).toContain("评分门通过 · 同层 Closed 最佳 88.00");
    expect(host.textContent).not.toContain("故障域");
    expect(host.textContent).not.toContain("secret-domain-id");
    expect(host.textContent).not.toContain("容量域");
    expect(host.textContent).not.toContain("lease-id");
    act(() => root.unmount());
  });

  it("shows circuit read-model unavailability instead of a healthy fallback", () => {
    const unavailable = snapshotFixture();
    unavailable.circuitReadModelStatus = "unavailable";
    unavailable.aggregates.unavailableCandidates = 1;
    unavailable.aggregates.persistenceUnavailableCircuits = 1;
    unavailable.candidates[0].participationStatus = "unavailable";
    unavailable.candidates[0].participationReason = "circuit_persistence_unavailable";

    const host = document.createElement("div");
    document.body.append(host);
    const root = createRoot(host);
    act(() => {
      root.render(
        <RoutingStatusDiagnosticsPanel
          snapshot={unavailable}
          runtimeOverlay={null}
          decisions={null}
          protectionStatus={null}
          loading={false}
          developerModeEnabled
        />,
      );
    });

    expect(host.textContent).toContain("熔断读模型暂不可用");
    expect(host.textContent).toContain("熔断状态不可用");
    act(() => root.unmount());
  });

  it("preserves loading and empty diagnostics states", () => {
    const host = document.createElement("div");
    document.body.append(host);
    const root = createRoot(host);
    act(() => {
      root.render(
        <RoutingStatusDiagnosticsPanel
          snapshot={null}
          runtimeOverlay={null}
          decisions={null}
          protectionStatus={null}
          loading
          developerModeEnabled
        />,
      );
    });
    expect(host.textContent).toContain("正在读取路由状态");

    act(() => {
      root.render(
        <RoutingStatusDiagnosticsPanel
          snapshot={null}
          runtimeOverlay={null}
          decisions={null}
          protectionStatus={null}
          loading={false}
          developerModeEnabled
        />,
      );
    });
    expect(host.textContent).toContain("暂无路由诊断数据");
    act(() => root.unmount());
  });

  it("renders query failures as errors instead of empty data", () => {
    const host = document.createElement("div");
    document.body.append(host);
    const root = createRoot(host);
    act(() => {
      root.render(
        <RoutingStatusDiagnosticsPanel
          snapshot={null}
          runtimeOverlay={null}
          decisions={null}
          protectionStatus={null}
          loading={false}
          error="数据库暂不可用"
          developerModeEnabled
        />,
      );
    });
    expect(host.textContent).toContain("无法读取路由诊断");
    expect(host.textContent).toContain("数据库暂不可用");
    expect(host.textContent).not.toContain("暂无路由诊断数据");

    act(() => {
      root.render(
        <RoutingStatusDiagnosticsPanel
          snapshot={snapshotFixture()}
          runtimeOverlay={null}
          decisions={null}
          protectionStatus={null}
          loading={false}
          error="刷新超时"
          developerModeEnabled
        />,
      );
    });
    expect(host.textContent).toContain("当前显示的是上一次成功读取的数据");
    expect(host.textContent).toContain("刷新超时");
    act(() => root.unmount());
  });
});
