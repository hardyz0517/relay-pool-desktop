import { getActiveBackendClient } from "@/lib/bridge/activeBackendClient";
import type { StationInput, StationUpdateInput } from "@/lib/types/stations";

export function listStations() {
  return getActiveBackendClient().stations.listStations();
}

export function loadStationAssets() {
  return getActiveBackendClient().stations.loadStationAssets();
}

export function loadStationDetail(stationId: string) {
  return getActiveBackendClient().stations.loadStationDetail(stationId);
}

export function getStationDetailRevision(stationId: string) {
  return getActiveBackendClient().stations.getStationDetailRevision(stationId);
}

export function getStationAssetsRevision() {
  return getActiveBackendClient().stations.getStationAssetsRevision();
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

export function pingStationEndpoint(stationId: string) {
  return getActiveBackendClient().stations.pingStationEndpoint(stationId);
}
