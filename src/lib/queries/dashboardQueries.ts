import { listChangeEvents } from "@/lib/api/changeEvents";
import { listBalanceSnapshots } from "@/lib/api/economics";
import { getProxyStatus, listRequestLogs } from "@/lib/api/proxy";
import { getSettings } from "@/lib/api/settings";
import { listKeyPoolItems } from "@/lib/api/stationKeys";
import { listStations } from "@/lib/api/stations";
import type { ChangeEvent } from "@/lib/types/changeEvents";
import type { BalanceSnapshot } from "@/lib/types/economics";
import type { ProxyStatus, RequestLog } from "@/lib/types/proxy";
import type { AppSettings } from "@/lib/types/settings";
import type { KeyPoolItem } from "@/lib/types/stationKeys";
import type { Station } from "@/lib/types/stations";

export type DashboardWorkspace = {
  proxyStatus: ProxyStatus;
  requestLogs: RequestLog[];
  keyPoolItems: KeyPoolItem[];
  stations: Station[];
  balanceSnapshots: BalanceSnapshot[];
  settings: AppSettings;
  changeEvents: ChangeEvent[];
};

export async function loadDashboardWorkspace(): Promise<DashboardWorkspace> {
  const [proxyStatus, requestLogs, keyPoolItems, stations, balanceSnapshots, settings, changeEvents] = await Promise.all([
    getProxyStatus(),
    listRequestLogs(),
    listKeyPoolItems(),
    listStations(),
    listBalanceSnapshots(),
    getSettings(),
    listChangeEvents(),
  ]);

  return {
    proxyStatus,
    requestLogs,
    keyPoolItems,
    stations,
    balanceSnapshots,
    settings,
    changeEvents,
  };
}
