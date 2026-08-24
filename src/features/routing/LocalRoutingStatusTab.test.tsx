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
        title: "容量重试",
        summary: "等待后重试同一目标",
        detailCode: "capacity_retry",
        detailAvailability: "detailed",
        explanationKey: null,
        action: "retry_same_target",
        attemptOrdinal: 2,
        remainingAttempts: 2,
        remainingWaitBudgetMs: 500,
        policyRevision: 4,
        failureDomain: "capacity-domain-a",
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
    expect(host.textContent).toContain("动作：同目标重试");
    expect(host.textContent).toContain("剩余尝试：2");
    expect(host.textContent).toContain("故障域：capacity-domain-a");
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
