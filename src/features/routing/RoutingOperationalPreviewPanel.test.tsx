// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type {
  RecentRouteDecisionsPage,
  RequestDecisionTrace,
  RouteSimulationResult,
  RoutingRuntimeOverlay,
  RoutingWorkspaceCandidate,
  RoutingWorkspaceSnapshot,
  StationKeyOperationalDetail,
} from "@/lib/types/routing";
import type { VersionedRoutingDeepLink } from "@/lib/types/routingDeepLinks";
import { RoutingOperationalPreviewPanel } from "./RoutingOperationalPreviewPanel";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const queryMocks = vi.hoisted(() => ({
  getRequestDecisionTraceQuery: vi.fn(),
  getStationKeyOperationalDetailQuery: vi.fn(),
  simulateRouteQuery: vi.fn(),
}));

vi.mock("@/lib/queries/routingQueries", () => queryMocks);

function candidate(overrides: Partial<RoutingWorkspaceCandidate> = {}): RoutingWorkspaceCandidate {
  return {
    stationKeyId: "key-long-local-name-abcdefghijklmnopqrstuvwxyz-0123456789",
    stationId: "station-1",
    stationName: "Very long local relay station name that should not force the workspace wider than the window",
    keyName: "Very long local key name that should truncate in the candidate table",
    endpointRevision: 7,
    priority: 1,
    schedulable: true,
    healthState: "ready",
    group: {
      stableKey: "default",
      displayName: "Default group",
      available: true,
      reason: "group_resolved",
    },
    multiplier: {
      status: "missing",
      multiplier: null,
      selectedSource: null,
      ceilingRejected: false,
      reason: "multiplier_missing",
    },
    capabilitySummary: {
      chatCompletions: true,
      responses: true,
      embeddings: false,
      stream: true,
      tools: true,
      vision: false,
      reasoning: false,
    },
    capabilityVerdicts: {
      protocol: "allow",
      model: "allow",
      stream: "allow",
      tools: "allow",
      vision: "unknown",
      reasoning: "unknown",
      rejectionSubjects: [],
    },
    priceBasis: "unpriced",
    pricing: {
      basis: "unpriced",
      comparisonValue: null,
      reason: "pricing unavailable",
      currency: null,
      unit: null,
      estimatedInputPrice: null,
      estimatedOutputPrice: null,
      estimatedFixedPrice: null,
      statusLabel: "unavailable",
      sourceChain: [],
      observedAt: null,
      confidence: null,
    },
    balanceStatus: null,
    capacity: {
      mode: "snapshot_only",
      maxConcurrency: 4,
      inFlight: null,
      acquired: false,
    },
    sourceRefs: {
      stationKeyId: "key-long-local-name-abcdefghijklmnopqrstuvwxyz-0123456789",
      stationId: "station-1",
      endpointRevision: 7,
      snapshotId: "snapshot-a",
      factVersionVector: "station=1,key=2,settings=3",
      projectorVersion: "route_candidate_projection_v1",
    },
    hardRejectionCodes: [],
    ...overrides,
  };
}

function snapshot(overrides: Partial<RoutingWorkspaceSnapshot> = {}): RoutingWorkspaceSnapshot {
  const rows = overrides.candidates ?? [candidate()];
  return {
    readModelVersion: "routing_workspace.v1",
    generatedAtMs: 1,
    productionPolicy: "priority_fallback",
    previewPolicyVersion: "hierarchical_v1",
    maxRateMultiplier: 2.5,
    routingGroupFilter: "all_groups",
    capacityMode: "snapshot_only",
    page: {
      limit: 50,
      returned: rows.length,
      nextCursor: null,
    },
    candidates: rows,
    readModelStatus: "available",
    ...overrides,
  };
}

function runtimeOverlay(overrides: Partial<RoutingRuntimeOverlay> = {}): RoutingRuntimeOverlay {
  return {
    overlayVersion: "routing_runtime_overlay.v1",
    sampledAtMs: 2,
    revision: 11,
    candidates: [
      {
        stationKeyId: "key-long-local-name-abcdefghijklmnopqrstuvwxyz-0123456789",
        stationId: "station-1",
        endpointRevision: 7,
        inFlight: 2,
        healthState: "degraded",
        cooldownUntil: null,
      },
    ],
    ...overrides,
  };
}

function decisions(overrides: Partial<RecentRouteDecisionsPage> = {}): RecentRouteDecisionsPage {
  return {
    pageVersion: "route_decisions.v1",
    decisions: [
      {
        requestLogId: "request-log-1",
        requestId: "req-1",
        createdAt: "2026-07-31T00:00:00Z",
        startedAt: "2026-07-31T00:00:00Z",
        finishedAt: "2026-07-31T00:00:01Z",
        durationMs: 1000,
        endpoint: "chat_completions",
        model: "gpt-4o-mini",
        status: "success",
        lifecycleStatus: "completed",
        stationKeyId: "key-long-local-name-abcdefghijklmnopqrstuvwxyz-0123456789",
        stationId: "station-1",
        routePolicy: "priority_fallback",
        routeReason: "selected",
        fallbackCount: 1,
        costStatus: "unavailable",
        estimatedTotalCost: null,
        costCurrency: null,
      },
    ],
    nextCursor: null,
    readModelStatus: "available",
    ...overrides,
  };
}

function trace(overrides: Partial<RequestDecisionTrace> = {}): RequestDecisionTrace {
  return {
    traceVersion: "request_decision_trace.v1",
    requestLogId: "request-log-1",
    status: "trace_unavailable",
    reason: "typed_trace_fixture",
    legacySummary: null,
    timeline: [
      {
        ordinal: 1,
        kind: "planning_round",
        status: "available",
        title: "Planning round 1 with a very long backend supplied explanation title",
        summary: "Backend planner filtered and ordered candidates from the immutable read-model snapshot.",
        detailCode: "planner.detail.code.with.a.very.long.backend.value.that.must.wrap.instead.of_overflowing",
        routePolicy: "priority_fallback",
        routeReason: "selected",
        stationKeyId: "key-long-local-name-abcdefghijklmnopqrstuvwxyz-0123456789",
        stationId: "station-1",
        attemptCount: 1,
        fallbackCount: 0,
        durationMs: 12,
        costStatus: "unavailable",
        estimatedTotalCost: null,
        costCurrency: null,
      },
    ],
    planningRounds: [],
    ...overrides,
  };
}

function detail(overrides: Partial<StationKeyOperationalDetail> = {}): StationKeyOperationalDetail {
  return {
    detailVersion: "operational_detail.v1",
    stationKeyId: "key-long-local-name-abcdefghijklmnopqrstuvwxyz-0123456789",
    stationId: "station-1",
    endpointRevision: 7,
    facts: [
      {
        scope: "station_key",
        name: "capability",
        value: "backend projection says chat/responses/tools",
        source: "capability_projector",
        freshness: "fresh",
        reason: "source revision 7",
      },
    ],
    lazyHistoryAvailable: true,
    readModelStatus: "available",
    ...overrides,
  };
}

function simulation(overrides: Partial<RouteSimulationResult> = {}): RouteSimulationResult {
  return {
    previewPolicyVersion: "hierarchical_v1",
    capacityMode: "snapshot_only",
    selectedCapacityAcquired: false,
    selectedStationKeyId: "key-long-local-name-abcdefghijklmnopqrstuvwxyz-0123456789",
    selectedStationId: "station-1",
    mappedModel: "gpt-4o-mini",
    policy: "priority_fallback",
    maxRateMultiplier: 2.5,
    routingGroupFilter: "all_groups",
    schedulerErrorCode: null,
    candidates: [],
    message: "simulation came from backend planner",
    ...overrides,
  };
}

async function renderPanel({
  deepLink,
  panelDecisions = decisions(),
  panelLoading = false,
  panelRuntimeOverlay = runtimeOverlay(),
  panelSnapshot = snapshot(),
}: {
  deepLink?: VersionedRoutingDeepLink;
  panelDecisions?: RecentRouteDecisionsPage | null;
  panelLoading?: boolean;
  panelRuntimeOverlay?: RoutingRuntimeOverlay | null;
  panelSnapshot?: RoutingWorkspaceSnapshot | null;
} = {}) {
  const host = document.createElement("div");
  const root = createRoot(host);
  const onOpenRequestLog = vi.fn();

  await act(async () =>
    root.render(
      <RoutingOperationalPreviewPanel
        decisions={panelDecisions}
        deepLink={deepLink}
        loading={panelLoading}
        onOpenRequestLog={onOpenRequestLog}
        runtimeOverlay={panelRuntimeOverlay}
        snapshot={panelSnapshot}
      />,
    ),
  );

  return { host, root, onOpenRequestLog };
}

describe("RoutingOperationalPreviewPanel", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("opens request deep links through the backend decision trace and returns to request logs", async () => {
    queryMocks.getRequestDecisionTraceQuery.mockResolvedValueOnce(trace());
    queryMocks.getStationKeyOperationalDetailQuery.mockResolvedValue(detail());
    queryMocks.simulateRouteQuery.mockResolvedValue(simulation());

    const { host, root, onOpenRequestLog } = await renderPanel({
      deepLink: {
        kind: "request",
        requestLogId: "request-log-1",
        source: "request_log",
        sequence: 1,
      },
    });

    await act(async () => undefined);

    expect(queryMocks.getRequestDecisionTraceQuery).toHaveBeenCalledWith("request-log-1");
    expect(host.textContent).toContain("Planning round 1");
    expect(host.textContent).toContain("planner.detail.code.with.a.very.long.backend.value");
    expect(host.textContent).not.toContain("planningRounds");

    const requestLogButton = [...host.querySelectorAll<HTMLButtonElement>("button")].find((button) =>
      button.textContent?.includes("查看使用记录"),
    );
    expect(requestLogButton).toBeTruthy();

    await act(async () => requestLogButton!.dispatchEvent(new MouseEvent("click", { bubbles: true })));
    expect(onOpenRequestLog).toHaveBeenCalledWith("request-log-1");

    await act(async () => root.unmount());
  });

  it("runs simulate-model deep links against the shared backend simulator using snapshot policy", async () => {
    queryMocks.getRequestDecisionTraceQuery.mockResolvedValue(trace());
    queryMocks.getStationKeyOperationalDetailQuery.mockResolvedValue(detail());
    queryMocks.simulateRouteQuery.mockResolvedValueOnce(simulation());

    const { host, root } = await renderPanel({
      deepLink: {
        kind: "simulate-model",
        model: "gpt-4.1-mini",
        endpoint: "responses",
        source: "pricing",
        sequence: 2,
      },
    });

    await act(async () => undefined);

    expect(queryMocks.simulateRouteQuery).toHaveBeenCalledWith({
      endpoint: "responses",
      model: "gpt-4.1-mini",
      stream: true,
      usesTools: false,
      usesVision: false,
      usesReasoning: false,
      policy: "priority_fallback",
      maxRateMultiplier: 2.5,
      routingGroupFilter: "all_groups",
    });
    expect(host.textContent).toContain("snapshot_only");
    expect(host.textContent).toContain("simulation came from backend planner");
    expect(host.textContent).toContain("unpriced");
    expect(host.textContent).toContain("pricing unavailable");

    await act(async () => root.unmount());
  });

  it("keeps loading and unavailable states explicit without fake health or zero price", async () => {
    const loading = await renderPanel({
      panelDecisions: null,
      panelLoading: true,
      panelRuntimeOverlay: null,
      panelSnapshot: null,
    });

    expect(loading.host.textContent).toContain("routing read model");
    expect(loading.host.textContent).not.toContain("ready");
    expect(loading.host.textContent).not.toContain("exact");
    expect(loading.host.textContent).not.toContain("0.000000");
    await act(async () => loading.root.unmount());

    const unavailable = await renderPanel({
      panelDecisions: decisions({ decisions: [], readModelStatus: "unavailable" }),
      panelRuntimeOverlay: null,
      panelSnapshot: snapshot({
        candidates: [],
        page: { limit: 50, returned: 0, nextCursor: null },
        readModelStatus: "unavailable",
      }),
    });

    expect(unavailable.host.textContent).toContain("unavailable");
    expect(unavailable.host.textContent).toContain("暂无候选");
    expect(unavailable.host.textContent).not.toContain("ready");
    expect(unavailable.host.textContent).not.toContain("0/4");
    expect(unavailable.host.textContent).not.toContain("$0");
    await act(async () => unavailable.root.unmount());
  });

  it("renders typed backend errors without falling back to stale candidate facts", async () => {
    queryMocks.getRequestDecisionTraceQuery.mockResolvedValue(trace());
    queryMocks.getStationKeyOperationalDetailQuery.mockResolvedValue(detail());
    queryMocks.simulateRouteQuery.mockRejectedValueOnce(new Error("routing_configuration_required_for_fixture"));

    const { host, root } = await renderPanel();
    const simulateButton = [...host.querySelectorAll<HTMLButtonElement>("button")].find((button) =>
      button.textContent?.includes("模拟"),
    );
    expect(simulateButton).toBeTruthy();

    await act(async () => simulateButton!.dispatchEvent(new MouseEvent("click", { bubbles: true })));

    expect(host.textContent).toContain("routing_configuration_required_for_fixture");
    expect(host.textContent).toContain("unpriced");
    expect(host.textContent).not.toContain("0.000000");

    await act(async () => root.unmount());
  });
});
