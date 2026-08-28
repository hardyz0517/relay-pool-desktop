import { describe, expect, it } from "vitest";
import { buildOfficialStatusView, createOfficialStatusInput, defaultOfficialStatusFilters } from "./officialStatusViewModel";

describe("official status view model", () => {
  it("creates a bounded first-page input and clears empty filters", () => {
    expect(createOfficialStatusInput(defaultOfficialStatusFilters)).toEqual({
      filter: { search: null, stationId: null, outcome: null, sourceState: null },
      cursor: null,
      limit: 100,
    });
  });

  it("projects rows without recomputing backend availability", () => {
    const view = buildOfficialStatusView({
      readAtMs: 1700000000000,
      summary: { monitorTotal: 1, supportedStationCount: 1 },
      rows: [{
        rowKey: "monitor-1", stationId: "station-1", stationName: "站点", stationType: "sub2api", stationEnabled: true, stationPriority: 1,
        endpointRevision: 2, sourceKind: "published_status", sourceState: "available", completeness: "complete", stale: false,
        lastAttemptAtMs: 1700000000000, lastSuccessAtMs: 1700000000000, lastCompleteAtMs: 1700000000000,
        upstreamMonitorId: "upstream-1", identityKind: "upstream_id", name: "监控", provider: "openai", groupName: null,
        primaryModel: "gpt-test", extraModels: [], currentOutcome: "available", currentLatencyMs: 120, currentPingLatencyMs: 30,
        recentAvailabilityPercent: 87.5, upstreamCheckedAtMs: 1700000000000, recentSamples: [],
      }],
      page: { limit: 100, returned: 1, nextCursor: "v1:100" },
    });
    expect(view.rows[0].availabilityLabel).toBe("87.50%");
    expect(view.rows[0].sourceStateLabel).toBe("正常");
    expect(view.nextCursor).toBe("v1:100");
  });
});
