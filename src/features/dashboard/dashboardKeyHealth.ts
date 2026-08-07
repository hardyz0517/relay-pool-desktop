import type { ChannelMonitor, ChannelStatusRow } from "@/lib/types/channelMonitors";
import type { KeyPoolItem, StationKeyStatus } from "@/lib/types/stationKeys";
import { findStationKeyMonitor } from "@/lib/channelMonitorViewModel";

const outcomeTone = {
  available: "healthy",
  degraded: "warning",
  unavailable: "error",
  skipped: "disabled",
  missing: "info",
} as const;

export function summarizeDashboardKeyHealth(
  keys: Array<Pick<KeyPoolItem, "id" | "enabled">>,
  monitors: ChannelMonitor[],
  statusRows: ChannelStatusRow[],
): Record<StationKeyStatus, number> {
  const summary: Record<StationKeyStatus, number> = {
    unchecked: 0,
    healthy: 0,
    warning: 0,
    error: 0,
    disabled: 0,
  };

  for (const key of keys) {
    if (!key.enabled) {
      summary.disabled += 1;
      continue;
    }
    const monitor = findStationKeyMonitor(monitors, key.id);
    const monitorStatus = dashboardMonitorStatus(monitor, statusRows);
    const status: StationKeyStatus =
      monitorStatus?.tone === "healthy"
        ? "healthy"
        : monitorStatus?.tone === "warning"
          ? "warning"
          : monitorStatus?.tone === "error"
            ? "error"
            : "unchecked";
    summary[status] += 1;
  }
  return summary;
}

function dashboardMonitorStatus(
  monitor: ChannelMonitor | null,
  rows: ChannelStatusRow[],
) {
  if (!monitor?.enabled || monitor.targetType !== "station_key" || !monitor.stationKeyId) {
    return null;
  }
  const row = rows.find(
    (candidate) =>
      candidate.monitor.id === monitor.id &&
      candidate.target.stationKeyId === monitor.stationKeyId,
  );
  const outcome = row?.latest?.outcome ?? "missing";
  return { tone: row?.running ? "info" : outcomeTone[outcome] };
}
