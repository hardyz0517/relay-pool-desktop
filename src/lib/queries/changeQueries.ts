import { listChangeEvents } from "@/lib/api/changeEvents";
import { listStations } from "@/lib/api/stations";
import type { ChangeEvent } from "@/lib/types/changeEvents";
import type { Station } from "@/lib/types/stations";

export type ChangeCenterWorkspace = {
  changeEvents: ChangeEvent[];
  stations: Station[];
};

export async function loadChangeCenterWorkspace(): Promise<ChangeCenterWorkspace> {
  const [changeEvents, stations] = await Promise.all([listChangeEvents(), listStations()]);

  return {
    changeEvents,
    stations,
  };
}
