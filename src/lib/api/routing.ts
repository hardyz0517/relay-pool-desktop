import { invoke } from "@tauri-apps/api/core";
import type {
  ModelAlias,
  StationKeyCapabilities,
  StationKeyHealth,
  UpdateStationKeyCapabilitiesInput,
  UpsertModelAliasInput,
} from "@/lib/types/routing";

export function getStationKeyCapabilities(stationKeyId: string) {
  return invoke<StationKeyCapabilities>("get_station_key_capabilities", { stationKeyId });
}

export function updateStationKeyCapabilities(input: UpdateStationKeyCapabilitiesInput) {
  return invoke<StationKeyCapabilities>("update_station_key_capabilities", { input });
}

export function listModelAliases() {
  return invoke<ModelAlias[]>("list_model_aliases");
}

export function upsertModelAlias(input: UpsertModelAliasInput) {
  return invoke<ModelAlias>("upsert_model_alias", { input });
}

export function deleteModelAlias(id: string) {
  return invoke<void>("delete_model_alias", { id });
}

export function listStationKeyHealth() {
  return invoke<StationKeyHealth[]>("list_station_key_health");
}

export function getStationKeyHealth(stationKeyId: string) {
  return invoke<StationKeyHealth>("get_station_key_health", { stationKeyId });
}
