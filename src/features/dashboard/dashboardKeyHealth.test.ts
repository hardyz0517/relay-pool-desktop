import { describe, expect, it } from "vitest";
import type { ChannelMonitor, ChannelStatusRow } from "@/lib/types/channelMonitors";
import { summarizeDashboardKeyHealth } from "./dashboardKeyHealth";

const monitor = (stationKeyId: string, enabled = true) => ({
  id: `monitor-${stationKeyId}`,
  name: `Monitor ${stationKeyId}`,
  targetType: "station_key" as const,
  enabled,
  stationId: "station-1",
  stationKeyId,
}) as unknown as ChannelMonitor;

const statusRow = (stationKeyId: string, outcome: "available" | "unavailable") => ({
  monitor: monitor(stationKeyId),
  target: { stationKeyId },
  latest: { outcome },
}) as unknown as ChannelStatusRow;

describe("dashboard key health summary", () => {
  it("uses canonical health snapshots instead of the key asset status", () => {
    const result = summarizeDashboardKeyHealth(
      [{ id: "disabled", enabled: false }, { id: "unchecked", enabled: true }, { id: "healthy", enabled: true }, { id: "error", enabled: true }],
      [monitor("healthy"), monitor("error")],
      [statusRow("healthy", "available"), statusRow("error", "unavailable")],
    );

    expect(result).toEqual({ unchecked: 1, healthy: 1, warning: 0, error: 1 });
  });
});
