import { describe, expect, it } from "vitest";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";

import type { ChannelStatusRow } from "@/lib/types/channelMonitors";
import { ChannelStatusCardGrid } from "./components/ChannelStatusCardGrid";
import {
  availabilityHue,
  buildRowView,
  buildTrend,
  createChannelStatusWorkspaceInput,
  statusLabel,
} from "./channelStatusViewModel";

describe("channel status V2 view model", () => {
  it("distinguishes balance pause from a user-controlled disable", () => {
    const row = fixtureRow();
    row.monitor.balancePaused = true;

    expect(buildRowView(row, "recent").currentLabel).toBe("余额暂停");

    row.monitor.enabled = false;
    expect(buildRowView(row, "recent").currentLabel).toBe("停用");
  });

  it.each([
    [null, null],
    [-1, 0],
    [0, 0],
    [50, 60],
    [75, 90],
    [100, 120],
    [101, 120],
  ] as const)("maps availability %s to hue %s", (value, hue) => {
    expect(availabilityHue(value)).toBe(hue);
  });

  it("uses normal and error as the user-facing health terms", () => {
    expect(statusLabel("available")).toBe("正常");
    expect(statusLabel("unavailable")).toBe("错误");
  });

  it("creates a bounded stable workspace input from UI state", () => {
    expect(createChannelStatusWorkspaceInput({
      window: "last24h",
      filters: {
        search: "  key-a  ",
        enabled: "enabled",
        outcome: "degraded",
      },
      sort: { field: "availability", direction: "asc" },
    })).toEqual({
      window: "last24h",
      filter: {
        search: "key-a",
        enabled: true,
        outcome: "degraded",
        stationId: null,
        protocolKind: null,
        clientProfileId: null,
      },
      sort: { field: "availability", direction: "asc" },
      cursor: null,
      limit: 500,
    });
  });

  it("maps newest-first recent points to an oldest-first display timeline", () => {
    const row = fixtureRow();
    row.recent[0].httpStatus = 503;
    row.recent[0].terminalReason = "server overloaded";
    const trend = buildTrend(row, "recent");

    expect(trend).toHaveLength(2);
    expect(trend.map((cell) => cell.tone)).toEqual(["available", "unavailable"]);
    expect(trend[0].availabilityLabel).toBe("正常");
    expect(trend[1].availabilityLabel).toMatch(/^错误/);
    expect(trend[1].availabilityLabel).toMatch(/\(503\)$/);
  });

  it("falls back to terminal reason status codes for legacy unavailable points", () => {
    const row = fixtureRow();
    row.recent[0].httpStatus = null;
    row.recent[0].terminalReason = "HTTP 429";

    expect(buildTrend(row, "recent")[1].availabilityLabel).toMatch(/\(429\)$/);
  });

  it("shows named error codes and reasons for unavailable points", () => {
    const row = fixtureRow();
    row.recent[0].httpStatus = null;
    row.recent[0].terminalReason = "error_code=insufficient_quota";
    expect(buildTrend(row, "recent")[1].availabilityLabel).toMatch(/\(insufficient_quota\)$/);

    row.recent[0].terminalReason = "request timed out after 10000ms";
    expect(buildTrend(row, "recent")[1].availabilityLabel).toContain("(request timed out after 10000ms)");

    row.recent[0].terminalReason = null;
    row.recent[0].failureKind = "network";
    expect(buildTrend(row, "recent")[1].availabilityLabel).toMatch(/\(network\)$/);
  });

  it("maps backend buckets directly and preserves missing buckets", () => {
    const row = fixtureRow();
    const trend = buildTrend(row, "last24h");

    expect(trend).toHaveLength(2);
    expect(trend.map((cell) => cell.tone)).toEqual(["available", "missing"]);
  });

  it("does not collapse retry/fallback evidence in the row summary", () => {
    const row = fixtureRow();
    row.selectedWindow.latestOutcome = "available";
    const view = buildRowView(row, "last24h");

    expect(view.currentLabel).toBe("降级");
    expect(view.groupName).toBe("plus");
    expect(view.visualPlatform).toBe("openai");
    expect(view.availabilityPercent).toBe(92.5);
    expect(view.latencyLabel).toBe("321 ms");
    expect(view.endpointPingLabel).toBe("48 ms");
  });

  it("renders model latency and endpoint ping as the two card metrics", () => {
    const view = buildRowView(fixtureRow(), "recent");
    const markup = renderToStaticMarkup(createElement(ChannelStatusCardGrid, {
      rows: [view],
      loading: false,
    }));

    expect(markup).toContain("模型延迟");
    expect(markup).toContain("321 ms");
    expect(markup).toContain("端点 Ping");
    expect(markup).toContain("48 ms");
    expect(markup).toContain("可用性");
    expect(markup).not.toContain("正常率");
    expect(markup).not.toContain("最近探测");
    expect(markup).toContain("md:grid-cols-2 xl:grid-cols-3 2xl:grid-cols-4");
    expect(markup).toContain("flex w-full min-w-0 items-end gap-[2px]");
    expect(markup).toContain("h-5");
    expect(markup).not.toContain("<button");
  });

  it.each([
    ["gpt", "openai"],
    ["claude", "anthropic"],
    ["gemini", "gemini"],
    ["grok", "grok"],
    ["image_generation", "image"],
  ] as const)("maps the tested key group category %s to the %s mark", (category, platform) => {
    const row = fixtureRow();
    row.target.effectiveGroupCategory = category;

    expect(buildRowView(row, "recent").visualPlatform).toBe(platform);
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
      pauseOnZeroBalance: true,
      balancePaused: false,
      protocolKind: "open_ai_chat",
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
      stationKeyName: "密钥 A",
      groupName: "plus",
      effectiveGroupCategory: "gpt",
      endpointPing: {
        status: "success",
        latencyMs: 48,
        checkedAtMs: 1_700_000_000_000,
      },
    },
    latest: {
      targetResultId: "target-1",
      executionId: "execution-1",
      outcome: "degraded",
      failureKind: "fallback_used",
      terminalReason: "primary timed out",
      httpStatus: 200,
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
        targetResultId: "target-recent-2",
        executionId: "execution-recent-2",
        outcome: "unavailable",
        failureKind: "auth",
        terminalReason: "401",
        httpStatus: 401,
        latencyMs: null,
        checkedAtMs: 1_700_000_060_000,
        usedFallback: false,
        semanticConfidence: "validated",
        attemptCount: 1,
        effectiveModel: null,
      },
      {
        targetResultId: "target-recent-1",
        executionId: "execution-recent-1",
        outcome: "available",
        failureKind: null,
        terminalReason: null,
        httpStatus: 200,
        latencyMs: 120,
        checkedAtMs: 1_700_000_000_000,
        usedFallback: false,
        semanticConfidence: "validated",
        attemptCount: 1,
        effectiveModel: "gpt-4.1-mini",
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
