import type { StatusTone } from "@/components/ui";
import type { ChannelMonitor, ChannelStatusOutcome, ChannelStatusRow } from "@/lib/types/channelMonitors";

export type KeyPoolMonitorStatus = {
  label: string;
  tone: StatusTone;
};

const outcomeStatus: Record<ChannelStatusOutcome, KeyPoolMonitorStatus> = {
  available: { label: "正常", tone: "healthy" },
  degraded: { label: "降级", tone: "warning" },
  unavailable: { label: "错误", tone: "error" },
  skipped: { label: "跳过", tone: "disabled" },
  missing: { label: "未检测", tone: "info" },
};

export function keyPoolMonitorStatus(
  monitor: ChannelMonitor | null,
  rows: ChannelStatusRow[],
): KeyPoolMonitorStatus | null {
  if (!monitor?.enabled || monitor.targetType !== "station_key" || !monitor.stationKeyId) {
    return null;
  }

  const row = rows.find(
    (candidate) =>
      candidate.monitor.id === monitor.id &&
      candidate.target.stationKeyId === monitor.stationKeyId,
  );
  if (!row) {
    return outcomeStatus.missing;
  }
  if (row.running) {
    return { label: "检测中", tone: "info" };
  }
  return outcomeStatus[row.latest?.outcome ?? "missing"];
}
