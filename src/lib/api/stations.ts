import { invoke } from "@tauri-apps/api/core";
import type { Station, StationInput, StationUpdateInput } from "@/lib/types/stations";

export function listStations() {
  return invoke<Station[]>("list_stations");
}

export function createStation(input: StationInput) {
  return invoke<Station>("create_station", { input });
}

export function updateStation(input: StationUpdateInput) {
  return invoke<Station>("update_station", { input });
}

export function deleteStation(id: string) {
  return invoke<void>("delete_station", { id });
}

export function reorderStations(stationIds: string[]) {
  return invoke<Station[]>("reorder_stations", { stationIds });
}
