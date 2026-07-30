import { describe, expect, it } from "vitest";

import type { ChannelStatusRow } from "@/lib/types/channelMonitors";
import {
  availabilityTone,
  buildRowView,
  buildTrend,
  createChannelStatusWorkspaceInput,
} from "./channelStatusViewModel";

describe("channel status V2 view model", () => {
  it.each([
    [null, "muted"],
    [49.9, "danger"],
    [50, "warning"],
    [74.9, "warning"],
    [75, "success"],
  ] as const)("maps availability %s to %s", (value, tone) => {
    expect(availabilityTone(value)).toBe(tone);
  });

  it("creates a bounded stable workspace input from UI state", () => {
    expect(createChannelStatusWorkspaceInput({
      window: "last24h",
      filters: {
        search: "  key-a  ",
        enabled: "enabled",
        outcome: "degraded",
        protocolKind: "openai_chat",
        clientProfileId: "codex_cli_compat",
      },
      sort: { field: "availability", direction: "asc" },
    })).toEqual({
      window: "last24h",
      filter: {
        search: "key-a",
        enabled: true,
        outcome: "degraded",
        stationId: null,
        protocolKind: "openai_chat",
        clientProfileId: "codex_cli_compat",
      },
      sort: { field: "availability", direction: "asc" },
      cursor: null,
      limit: 500,
    });
  });

  it("maps recent points directly without padding or generated outcomes", () => {
    const row = fixtureRow();
    const trend = buildTrend(row, "recent");

    expect(trend).toHaveLength(2);
    expect(trend.map((cell) => cell.tone)).toEqual(["available", "unavailable"]);
  });

  it("maps backend buckets directly and preserves missing buckets", () => {
    const row = fixtureRow();
    const trend = buildTrend(row, "last24h");

    expect(trend).toHaveLength(2);
    expect(trend.map((cell) => cell.tone)).toEqual(["available", "missing"]);
  });

  it("does not collapse retry/fallback evidence in the row summary", () => {
    const row = fixtureRow();
    const view = buildRowView(row, "last24h");

    expect(view.currentLabel).toBe("降级");
    expect(view.availabilityPercent).toBe(92.5);
    expect(view.attemptsLabel).toBe("2 次");
    expect(view.fallbackLabel).toContain("fallback");
  });
});

function fixtureRow(): ChannelStatusRow {
  return {
    rowKey: "monitor-1:key-1",
    monitor: {
      id: "monitor-1",
      name: "OpenAI monitor",
      targetType: "station_key",
      enabled: true,
      protocolKind: "openai_chat",
      clientProfileId: "standard_api",
      clientProfileVersion: 1,
      primaryModel: "gpt-4.1-mini",
      fallbackModels: ["gpt-4o-mini"],
      intervalSeconds: 300,
      jitterSeconds: 30,
      nextDueAtMs: null,
    },
    target: {
      stationId: "station-1",
      stationName: "Station A",
      stationKeyId: "key-1",
      stationKeyName: "Key A",
    },
    latest: {
      targetResultId: "target-1",
      executionId: "execution-1",
      outcome: "degraded",
      failureKind: "fallback_used",
      terminalReason: "primary timed out",
      latencyMs: 321,
      finishedAtMs: 1_700_000_000_000,
      semanticConfidence: "validated",
      usedFallback: true,
      attemptCount: 2,
      effectiveModel: "gpt-4o-mini",
    },
    running: null,
    recent: [
      {
        targetResultId: "target-recent-1",
        executionId: "execution-recent-1",
        outcome: "available",
        failureKind: null,
        terminalReason: null,
        latencyMs: 120,
        checkedAtMs: 1_700_000_000_000,
        usedFallback: false,
        semanticConfidence: "validated",
        attemptCount: 1,
        effectiveModel: "gpt-4.1-mini",
      },
      {
        targetResultId: "target-recent-2",
        executionId: "execution-recent-2",
        outcome: "unavailable",
        failureKind: "auth",
        terminalReason: "401",
        latencyMs: null,
        checkedAtMs: 1_700_000_060_000,
        usedFallback: false,
        semanticConfidence: "validated",
        attemptCount: 1,
        effectiveModel: null,
      },
    ],
    hourlyBuckets: [
      {
        kind: "hour",
        startMs: 1,
        endMs: 2,
        state: "available",
        counts: { total: 1, available: 1, degraded: 0, unavailable: 0, skipped: 0 },
        strictAvailabilityBps: 10_000,
        effectiveAvailabilityBps: 10_000,
        p50LatencyMs: 120,
        p95LatencyMs: 120,
        failureCounts: {},
        dirty: false,
        corrupt: false,
      },
      {
        kind: "hour",
        startMs: 2,
        endMs: 3,
        state: "missing",
        counts: { total: 0, available: 0, degraded: 0, unavailable: 0, skipped: 0 },
        strictAvailabilityBps: null,
        effectiveAvailabilityBps: null,
        p50LatencyMs: null,
        p95LatencyMs: null,
        failureCounts: {},
        dirty: false,
        corrupt: false,
      },
    ],
    dailyBuckets: [],
    selectedWindow: {
      window: "last24h",
      bucketKind: "hour",
      startMs: 1,
      endMs: 3,
      counts: { total: 10, available: 8, degraded: 1, unavailable: 1, skipped: 0 },
      strictAvailabilityBps: 8_000,
      effectiveAvailabilityBps: 9_250,
      latestOutcome: "degraded",
      latestCheckedAtMs: 1_700_000_000_000,
      dirty: false,
      corrupt: false,
    },
  };
}
