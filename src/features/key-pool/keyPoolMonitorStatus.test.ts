import { describe, expect, it } from "vitest";
import type { ChannelMonitor, ChannelStatusOutcome, ChannelStatusRow } from "@/lib/types/channelMonitors";
import { keyPoolMonitorStatus } from "./keyPoolMonitorStatus";

function monitor(enabled = true): ChannelMonitor {
  return {
    id: "monitor-1",
    enabled,
    targetType: "station_key",
    stationKeyId: "key-1",
  } as ChannelMonitor;
}

function statusRow(outcome: ChannelStatusOutcome, running = false): ChannelStatusRow {
  return {
    monitor: { id: "monitor-1" },
    target: { stationKeyId: "key-1" },
    latest: outcome === "missing" ? null : { outcome },
    running: running ? { executionId: "execution-1" } : null,
    selectedWindow: { latestOutcome: outcome },
  } as ChannelStatusRow;
}

describe("keyPoolMonitorStatus", () => {
  it("hides status unless the station-key monitor is enabled", () => {
    expect(keyPoolMonitorStatus(null, [])).toBeNull();
    expect(keyPoolMonitorStatus(monitor(false), [statusRow("available")])).toBeNull();
  });

  it("maps channel status facts into key-pool status badges", () => {
    expect(keyPoolMonitorStatus(monitor(), [])).toEqual({ label: "未检测", tone: "info" });
    expect(keyPoolMonitorStatus(monitor(), [statusRow("available")])).toEqual({ label: "正常", tone: "healthy" });
    expect(keyPoolMonitorStatus(monitor(), [statusRow("degraded")])).toEqual({ label: "降级", tone: "warning" });
    expect(keyPoolMonitorStatus(monitor(), [statusRow("unavailable")])).toEqual({ label: "错误", tone: "error" });
    expect(keyPoolMonitorStatus(monitor(), [statusRow("missing", true)])).toEqual({ label: "检测中", tone: "info" });
  });

  it("uses the latest check instead of the selected window bucket summary", () => {
    const row = statusRow("degraded");
    row.latest = { outcome: "available" } as ChannelStatusRow["latest"];

    expect(keyPoolMonitorStatus(monitor(), [row])).toEqual({ label: "正常", tone: "healthy" });
  });
});
