import { listCurrentAlertingIncidents } from "@/lib/api/alerting";
import { listStations } from "@/lib/api/stations";
import type { AlertingIncident } from "@/lib/types/alerting";
import type { Station } from "@/lib/types/stations";

export type ChangeCenterWorkspace = {
  incidents: AlertingIncident[];
  stations: Station[];
};

export async function loadChangeCenterWorkspace(): Promise<ChangeCenterWorkspace> {
  const [incidentPage, stations] = await Promise.all([
    listCurrentAlertingIncidents({ limit: 100 }),
    listStations(),
  ]);

  return {
    incidents: incidentPage.items,
    stations,
  };
}
