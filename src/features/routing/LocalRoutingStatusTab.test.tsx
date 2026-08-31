// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, describe, expect, it } from "vitest";
import type { RequestDecisionTrace } from "@/lib/types/routing";
import { DecisionTraceDetails } from "./LocalRoutingStatusTab";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

afterEach(() => {
  document.body.innerHTML = "";
});

describe("DecisionTraceDetails", () => {
  it("shows detailed action labels and trace terminal facts", () => {
    const trace = baseTrace({
      status: "runtime_trace",
      detailAvailability: "detailed",
      reason: "capacity_exhausted",
      timeline: [{
        ordinal: 1,
        kind: "attempt_protocol",
        status: "available",
        title: "密钥失败重试",
        summary: "当前密钥失败，等待后重试",
        detailCode: "capacity_retry",
        detailAvailability: "detailed",
        explanationKey: null,
        action: "retry_current_key",
        attemptOrdinal: 2,
        remainingAttempts: 2,
        remainingPrecommitBudgetMs: 500,
        policyRevision: 4,
        routePolicy: null,
        routeReason: null,
        stationKeyId: null,
        stationId: null,
        attemptCount: null,
        fallbackCount: null,
        durationMs: null,
        costStatus: null,
        estimatedTotalCost: null,
        costCurrency: null,
      }],
    });
    const host = render(trace);
    expect(host.textContent).toContain("详细执行证据");
    expect(host.textContent).toContain("当前会话详细轨迹");
    expect(host.textContent).toContain("终态原因：capacity_exhausted");
    expect(host.textContent).toContain("动作：重试当前密钥");
    expect(host.textContent).toContain("剩余尝试：2");
    expect(host.textContent).toContain("请求提交前预算：500 ms");
  });

  it("marks pre-cutover action values as historical compatibility evidence", () => {
    const trace = baseTrace({
      timeline: [{
        ordinal: 1,
        kind: "fallback",
        status: "available",
        title: "legacy retry",
        summary: "legacy trace evidence",
        detailCode: "legacy_retry",
        detailAvailability: "summary_only",
        explanationKey: null,
        action: "wait_then_replan",
        attemptOrdinal: 1,
        remainingAttempts: 1,
        remainingPrecommitBudgetMs: null,
        policyRevision: 2,
        routePolicy: null,
        routeReason: null,
        stationKeyId: null,
        stationId: null,
        attemptCount: null,
        fallbackCount: null,
        durationMs: null,
        costStatus: null,
        estimatedTotalCost: null,
        costCurrency: null,
      }],
    });
    const host = render(trace);
    expect(host.textContent).toContain("动作：历史动作：等待后重新规划");
  });

  it("shows the legacy terminal summary without fabricating a timeline", () => {
    const trace = baseTrace({
      status: "legacy_summary",
      detailAvailability: "summary_only",
      reason: "upstream_failed",
      legacySummary: {
        routePolicy: "stable_first",
        routeReason: "上游不可用",
        stationKeyId: "key-safe",
        stationId: "station-safe",
        fallbackCount: 2,
      },
      timeline: [],
    });
    const host = render(trace);
    expect(host.textContent).toContain("仅有终态摘要");
    expect(host.textContent).toContain("兼容终态摘要");
    expect(host.textContent).toContain("终态摘要");
    expect(host.textContent).toContain("路由策略：stable_first");
    expect(host.textContent).toContain("路由原因：上游不可用");
    expect(host.textContent).toContain("故障转移次数：2");
    expect(host.textContent).toContain("未返回可展示的步骤");
    expect(host.querySelector("ol")).toBeNull();
  });

  it("explicitly marks unavailable detail and preserves only safe terminal evidence", () => {
    const trace = baseTrace({
      status: "trace_unavailable",
      detailAvailability: "unavailable",
      reason: "trace_unavailable",
      legacySummary: null,
      timeline: [],
    });
    const host = render(trace);
    expect(host.textContent).toContain("详细步骤不可用（仅保留终态摘要）");
    expect(host.textContent).toContain("决策明细不可用");
    expect(host.textContent).toContain("详细步骤不可用；仅保留终态摘要");
    expect(host.querySelector("ol")).toBeNull();
  });
});

function render(trace: RequestDecisionTrace): HTMLElement {
  const host = document.createElement("div");
  document.body.append(host);
  const root = createRoot(host);
  act(() => root.render(<DecisionTraceDetails trace={trace} />));
  return host;
}

function baseTrace(overrides: Partial<RequestDecisionTrace>): RequestDecisionTrace {
  return {
    traceVersion: "request_decision_trace_v1",
    requestLogId: "request-1",
    status: "runtime_trace",
    detailAvailability: "detailed",
    reason: "selected",
    explanationKey: "selected_candidate",
    policyRevision: 4,
    legacySummary: null,
    timeline: [],
    planningRounds: [],
    ...overrides,
  };
}
