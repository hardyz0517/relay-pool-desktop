import { listChannelMonitorSummaries, listChannelMonitorTemplates } from "@/lib/api/channelMonitors";
import { listRequestLogs } from "@/lib/api/proxy";
import { listStationKeyHealth } from "@/lib/api/routing";
import { listKeyPoolItems } from "@/lib/api/stationKeys";
import { listStations } from "@/lib/api/stations";
import type { ChannelMonitorRequestTemplate, ChannelMonitorSummary } from "@/lib/types/channelMonitors";
import type { RequestLog } from "@/lib/types/proxy";
import type { StationKeyHealth } from "@/lib/types/routing";
import type { KeyPoolItem } from "@/lib/types/stationKeys";
import type { Station } from "@/lib/types/stations";

export type ChannelMonitoringWorkspace = {
  monitorSummaries: ChannelMonitorSummary[];
  stations: Station[];
  keyPoolItems: KeyPoolItem[];
  templates: ChannelMonitorRequestTemplate[];
};

export type ChannelStatusWorkspace = {
  keyPoolItems: KeyPoolItem[];
  requestLogs: RequestLog[];
  stationKeyHealth: StationKeyHealth[];
  monitorSummaries: ChannelMonitorSummary[];
};

export async function loadChannelMonitoringWorkspace(): Promise<ChannelMonitoringWorkspace> {
  const [monitorSummaries, stations, keyPoolItems, templates] = await Promise.all([
    listChannelMonitorSummaries(),
    listStations(),
    listKeyPoolItems(),
    listChannelMonitorTemplates(),
  ]);

  return {
    monitorSummaries,
    stations,
    keyPoolItems,
    templates,
  };
}

export async function loadChannelStatusWorkspace(): Promise<ChannelStatusWorkspace> {
  const [keyPoolItems, requestLogs, stationKeyHealth, monitorSummaries] = await Promise.all([
    listKeyPoolItems(),
    listRequestLogs(),
    listStationKeyHealth(),
    listChannelMonitorSummaries(),
  ]);

  return {
    keyPoolItems,
    requestLogs,
    stationKeyHealth,
    monitorSummaries,
  };
}
