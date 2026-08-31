// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, describe, expect, it } from "vitest";
import type { RoutingWorkspaceCandidate } from "@/lib/types/routing";
import type { RoutingCandidateView } from "@/lib/types/routingWorkspace";
import { LocalRoutingStatusCandidateRow, ScoreBreakdown } from "./LocalRoutingStatusCandidateRow";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

afterEach(() => {
  document.body.innerHTML = "";
});

describe("ScoreBreakdown", () => {
  it("renders expanded formulas as structured math", () => {
    const host = renderBreakdown(scoreDetails());

    expandFactor(host, "可靠性");

    expect(host.querySelectorAll("math").length).toBeGreaterThan(0);
    expect(host.querySelector("mfrac")).not.toBeNull();
    expect(host.querySelector("mtable")).not.toBeNull();
    expect(host.querySelector(".font-mono")).toBeNull();
  });

  it("keeps recent and historical reliability windows visible for insufficient zero-sample scores", () => {
    const host = renderBreakdown(scoreDetails());

    expandFactor(host, "可靠性");

    expect(host.textContent).toContain("最近 24 小时");
    expect(host.textContent).toContain("历史数据（24 小时以前，30 天窗口）");
    expect(host.textContent).toContain("0/5，样本不足");
    expect(host.textContent).toContain("0/15，样本不足");
    expect(host.textContent).not.toContain("未达到");
    expect(host.textContent).not.toContain("采用乐观值");
  });

  it("uses latency sample counts for responsiveness window details", () => {
    const details = scoreDetails();
    const windows = details.responsiveness.windowDetails;
    if (!windows) throw new Error("expected responsiveness window details");
    windows.recentObservationCount = 12;
    windows.recentLatencySampleCount = 2;
    windows.recentRealLatencySampleCount = 2;
    windows.historicalObservationCount = 31;
    windows.historicalLatencySampleCount = 7;
    windows.historicalRealLatencySampleCount = 7;

    const host = renderBreakdown(details);
    expandFactor(host, "响应速度");

    expect(host.textContent).toContain("2/5，样本不足");
    expect(host.textContent).toContain("7/15，样本不足");
    expect(host.textContent).not.toContain("12/5");
    expect(host.textContent).not.toContain("31/15");
  });

  it("labels windows with enough samples as sample-sufficient", () => {
    const details = scoreDetails();
    const windows = details.reliability.windowDetails;
    if (!windows) throw new Error("expected reliability window details");
    windows.recentRealSampleCount = 5;
    windows.recentMonitoringSampleCount = 5;
    windows.recentReliabilityMinimumMet = true;
    windows.historicalRealSampleCount = 15;
    windows.historicalMonitoringSampleCount = 15;
    windows.historicalReliabilityMinimumMet = true;

    const host = renderBreakdown(details);
    expandFactor(host, "可靠性");

    expect(host.textContent).toContain("5/5，样本充足");
    expect(host.textContent).toContain("15/15，样本充足");
    expect(host.textContent).not.toContain("未达到");
    expect(host.textContent).not.toContain("采用乐观值");
  });

  it("shows real and monitoring reliability samples separately", () => {
    const details = scoreDetails();
    const windows = details.reliability.windowDetails;
    if (!windows) throw new Error("expected reliability window details");
    windows.recentObservationCount = 7;
    windows.recentRealSampleCount = 3;
    windows.recentMonitoringSampleCount = 4;
    windows.historicalObservationCount = 11;
    windows.historicalRealSampleCount = 6;
    windows.historicalMonitoringSampleCount = 5;
    windows.monitoringSourceStatus = "comparable";

    const host = renderBreakdown(details);
    expandFactor(host, "可靠性");

    expect(host.textContent).toContain("实际流量样本3");
    expect(host.textContent).toContain("监控样本4");
    expect(host.textContent).toContain("纳入评分样本合计7");
    expect(host.textContent).toContain("实际流量样本6");
    expect(host.textContent).toContain("监控样本5");
    expect(host.textContent).toContain("纳入评分样本合计11");
  });

  it("does not exclude monitoring observations because their model differs", () => {
    const details = scoreDetails();
    const windows = details.reliability.windowDetails;
    if (!windows) throw new Error("expected reliability window details");
    windows.monitoringSourceStatus = "incomparable";
    windows.recentMonitoringSampleCount = 1;

    const host = renderBreakdown(details);
    expandFactor(host, "可靠性");

    expect(host.textContent).toContain("监控样本按整把密钥统计，已按监控来源权重参与评分");
  });
});

describe("LocalRoutingStatusCandidateRow concurrency", () => {
  it("renders backend participation reasons for paused, recovery, and unavailable candidates", () => {
    const unavailableCircuit = circuitDiagnostics("closed", null);
    unavailableCircuit.circuit.persistenceStatus = "unavailable";
    const paused = renderToStaticMarkup(
      <LocalRoutingStatusCandidateRow
        candidate={candidate({ participationStatus: "excluded", participationReason: "administratively_disabled" })}
        order={1}
        nowMs={0}
      />,
    );
    const recovery = renderToStaticMarkup(
      <LocalRoutingStatusCandidateRow
        candidate={candidate({ participationStatus: "conditionally_eligible", participationReason: "circuit_recovery_score_gate_passed" })}
        order={1}
        nowMs={0}
      />,
    );
    const unavailable = renderToStaticMarkup(
      <LocalRoutingStatusCandidateRow
        candidate={candidate({
          participationStatus: "unavailable",
          participationReason: "circuit_persistence_unavailable",
          diagnostics: unavailableCircuit,
        })}
        order={1}
        nowMs={0}
      />,
    );

    expect(paused).toContain("已暂停路由");
    expect(recovery).toContain("可恢复探测");
    expect(unavailable).toContain("熔断状态不可用");
    expect(unavailable).toContain("不可用");
  });

  it("shows the circuit countdown, half-open state, and closed placeholder", () => {
    const openMarkup = renderToStaticMarkup(
      <LocalRoutingStatusCandidateRow
        candidate={candidate({ diagnostics: circuitDiagnostics("open", 301_000) })}
        order={1}
        nowMs={0}
      />,
    );
    const halfOpenMarkup = renderToStaticMarkup(
      <LocalRoutingStatusCandidateRow
        candidate={candidate({ diagnostics: circuitDiagnostics("half_open", null) })}
        order={1}
        nowMs={0}
      />,
    );
    const closedMarkup = renderToStaticMarkup(
      <LocalRoutingStatusCandidateRow
        candidate={candidate({ diagnostics: circuitDiagnostics("closed", null) })}
        order={1}
        nowMs={0}
      />,
    );

    expect(openMarkup).toContain("05:01");
    expect(halfOpenMarkup).toContain("半开");
    expect(new DOMParser().parseFromString(closedMarkup, "text/html").body.textContent).toContain("-");
  });

  it("highlights active concurrency with a square green badge", () => {
    const markup = renderToStaticMarkup(
      <LocalRoutingStatusCandidateRow
        candidate={candidate({ currentConcurrency: 2, scoreStatus: "unavailable" })}
        order={1}
        nowMs={0}
      />,
    );
    const document = new DOMParser().parseFromString(markup, "text/html");
    const badge = [...document.querySelectorAll("span")].find(
      (element) => element.textContent === "2" && element.className.includes("bg-success-surface"),
    );

    expect(badge).toBeDefined();
    expect(badge?.className).toContain("rounded-[4px]");
    expect(badge?.className).not.toContain("rounded-full");
  });

  it("keeps zero concurrency in the disabled tone", () => {
    const markup = renderToStaticMarkup(
      <LocalRoutingStatusCandidateRow
        candidate={candidate({ currentConcurrency: 0, scoreStatus: "unavailable" })}
        order={1}
        nowMs={0}
      />,
    );
    const document = new DOMParser().parseFromString(markup, "text/html");
    const badge = [...document.querySelectorAll("span")].find(
      (element) => element.textContent === "0" && element.className.includes("bg-muted"),
    );

    expect(badge).toBeDefined();
    expect(badge?.className).toContain("rounded-[4px]");
  });
});

function renderBreakdown(details: NonNullable<RoutingWorkspaceCandidate["scoreDetails"]>) {
  const host = document.createElement("div");
  document.body.append(host);
  const root = createRoot(host);
  act(() => root.render(<ScoreBreakdown details={details} />));
  return host;
}

function expandFactor(host: HTMLElement, label: string) {
  const button = [...host.querySelectorAll("button")].find((element) =>
    element.textContent === "查看详情"
    && element.parentElement?.textContent?.includes(label)
  );
  if (!(button instanceof HTMLButtonElement)) {
    throw new Error(`missing detail button for ${label}`);
  }
  act(() => button.click());
}

function candidate(overrides: Partial<RoutingCandidateView> = {}): RoutingCandidateView {
  return {
    stationKeyId: "key-1",
    stationId: "station-1",
    stationName: "Station",
    keyName: "密钥",
    endpoint: "chat_completions",
    priority: 1,
    enabled: true,
    schedulable: true,
    participationStatus: "eligible",
    participationReason: "ready",
    score: null,
    scoreDetails: null,
    currentConcurrency: null,
    lastSuccessAt: null,
    lastFailureAt: null,
    routingGroupScope: "all_groups",
    routingGroupMatch: true,
    scoreStatus: "scored",
    plannerExclusionCodes: [],
    assessmentSnapshotId: null,
    assessmentDurableRevision: null,
    assessmentRequestContextFingerprint: null,
    facts: [],
    ...overrides,
  };
}

function circuitDiagnostics(
  state: "closed" | "open" | "half_open",
  cooldownUntilMs: number | null,
): NonNullable<RoutingCandidateView["diagnostics"]> {
  return {
    circuit: {
      state,
      stateRevision: null,
      lifecycleRevision: null,
      policyRevision: null,
      persistenceStatus: "available",
      stateRowPresent: false,
      consecutiveFailures: null,
      reopenLevel: 0,
      cooldownUntilMs,
      cooldownRemainingMs: cooldownUntilMs,
      halfOpenLeaseInFlight: false,
      halfOpenLeaseExpiresAtMs: null,
      recoverySuccesses: null,
      scoreGateStatus: "not_applicable",
      scoreGateReason: "test",
      bestClosedEffectiveScore: null,
    },
    effectiveScore: null,
    baseScore: null,
    quality: null,
    attempts: {
      rawRealAttemptCount: 0,
      deduplicatedRealRequestCount: 0,
    },
  };
}

function scoreDetails(): NonNullable<RoutingWorkspaceCandidate["scoreDetails"]> {
  const windowDetails = {
    recentObservationCount: 0,
    recentRealSampleCount: 0,
    recentMonitoringSampleCount: 0,
    recentEffectiveMassBasisPoints: 0,
    recentSuccessMassBasisPoints: 0,
    recentFailureMassBasisPoints: 0,
    recentMinimumSamples: 5,
    recentReliabilityMinimumMet: false,
    recentScore: 9_500,
    recentWeightBasisPoints: 0,
    recentResponsivenessWeightBasisPoints: 0,
    recentLatencySampleCount: 0,
    recentLatencyEffectiveMassBasisPoints: 0,
    recentWeightedLatencyMs: 2_500,
    recentLatencyMinimumMet: false,
    recentRealLatencySampleCount: 0,
    recentMonitoringLatencySampleCount: 0,
    recentRealWeightedLatencyMs: 2_500,
    recentMonitoringWeightedLatencyMs: 2_500,
    recentRealLatencyMinimumMet: false,
    recentMonitoringLatencyMinimumMet: false,
    responsivenessRealSourceWeightBasisPoints: 7_000,
    responsivenessMonitoringSourceWeightBasisPoints: 3_000,
    recentLatencyCoverageBasisPoints: 0,
    recentResponsivenessBasisPoints: 9_791,
    historicalObservationCount: 0,
    historicalRealSampleCount: 0,
    historicalMonitoringSampleCount: 0,
    historicalEffectiveMassBasisPoints: 0,
    historicalSuccessMassBasisPoints: 0,
    historicalFailureMassBasisPoints: 0,
    historicalMinimumSamples: 15,
    historicalReliabilityMinimumMet: false,
    historicalScore: 9_500,
    historicalWeightBasisPoints: 10_000,
    historicalResponsivenessWeightBasisPoints: 10_000,
    historicalLatencySampleCount: 0,
    historicalLatencyEffectiveMassBasisPoints: 0,
    historicalWeightedLatencyMs: 2_500,
    historicalLatencyMinimumMet: false,
    historicalRealLatencySampleCount: 0,
    historicalMonitoringLatencySampleCount: 0,
    historicalRealWeightedLatencyMs: 2_500,
    historicalMonitoringWeightedLatencyMs: 2_500,
    historicalRealLatencyMinimumMet: false,
    historicalMonitoringLatencyMinimumMet: false,
    historicalLatencyCoverageBasisPoints: 0,
    historicalResponsivenessBasisPoints: 9_791,
    historicalAgeWindowDays: 30,
    historicalHalfLifeDays: 7,
    monitoringSourceStatus: "no_evidence" as const,
  };
  const factor = {
    score: 9_500,
    weight: 2_500,
    contribution: 2_375,
    inputs: [],
  };

  return {
    total: 9_500,
    reliability: { ...factor, windowDetails: { ...windowDetails } },
    responsiveness: { ...factor, windowDetails: { ...windowDetails } },
    cost: { ...factor, windowDetails: null },
    preference: { ...factor, windowDetails: null },
  };
}
