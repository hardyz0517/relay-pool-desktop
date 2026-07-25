import { getActiveBackendClient } from "@/lib/bridge/activeBackendClient";
import type {
  RouteSimulationInput,
  UpdateStationKeyCapabilitiesInput,
  UpsertModelAliasInput,
} from "@/lib/types/routing";

export function getStationKeyCapabilities(stationKeyId: string) {
  return getActiveBackendClient().routing.getStationKeyCapabilities(stationKeyId);
}

export function updateStationKeyCapabilities(input: UpdateStationKeyCapabilitiesInput) {
  return getActiveBackendClient().routing.updateStationKeyCapabilities(input);
}

export function listModelAliases() {
  return getActiveBackendClient().routing.listModelAliases();
}

export function upsertModelAlias(input: UpsertModelAliasInput) {
  return getActiveBackendClient().routing.upsertModelAlias(input);
}

export function deleteModelAlias(id: string) {
  return getActiveBackendClient().routing.deleteModelAlias(id);
}

export function listStationKeyHealth() {
  return getActiveBackendClient().routing.listStationKeyHealth();
}

export function getStationKeyHealth(stationKeyId: string) {
  return getActiveBackendClient().routing.getStationKeyHealth(stationKeyId);
}

export function simulateRoute(input: RouteSimulationInput) {
  return getActiveBackendClient().routing.simulateRoute(input);
}
