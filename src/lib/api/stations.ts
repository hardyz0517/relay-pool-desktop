import { getActiveBackendClient } from "@/lib/bridge/activeBackendClient";
import type { StationInput, StationUpdateInput, UpsertStationCapacityDomainInput } from "@/lib/types/stations";

export function listStations() {
  return getActiveBackendClient().stations.listStations();
}

export function createStation(input: StationInput) {
  return getActiveBackendClient().stations.createStation(input);
}

export function updateStation(input: StationUpdateInput) {
  return getActiveBackendClient().stations.updateStation(input);
}

export function deleteStation(id: string) {
  return getActiveBackendClient().stations.deleteStation(id);
}

export function openStationWebsite(url: string) {
  return getActiveBackendClient().stations.openStationWebsite(url);
}

export function reorderStations(stationIds: string[]) {
  return getActiveBackendClient().stations.reorderStations(stationIds);
}

export function getStationCapacityDomain(stationId: string) {
  return getActiveBackendClient().stations.getStationCapacityDomain(stationId);
}

export function upsertStationCapacityDomain(input: UpsertStationCapacityDomainInput) {
  return getActiveBackendClient().stations.upsertStationCapacityDomain(input);
}

export function clearStationCapacityDomain(stationId: string, expectedRevision: number) {
  return getActiveBackendClient().stations.clearStationCapacityDomain(stationId, expectedRevision);
}

export function pingStationEndpoint(stationId: string) {
  return getActiveBackendClient().stations.pingStationEndpoint(stationId);
}
