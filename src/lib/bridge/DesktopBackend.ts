import { validateRuntimeContract } from "@/app/bootstrap/runtimeContract";
import {
  chooseDataDir as chooseDataDirBinding,
  createStation as createStationBinding,
  deleteStation as deleteStationBinding,
  getLocalAccessKey as getLocalAccessKeyBinding,
  getRuntimeContractInfo,
  getSettings as getSettingsBinding,
  importRelayPoolToCcswitch as importRelayPoolToCcswitchBinding,
  listStationEndpointHealth as listStationEndpointHealthBinding,
  listStations as listStationsBinding,
  openExternalUrl as openExternalUrlBinding,
  pingStationEndpoint as pingStationEndpointBinding,
  reorderStations as reorderStationsBinding,
  resetDataDir as resetDataDirBinding,
  updateLocalAccessKey as updateLocalAccessKeyBinding,
  updateSettings as updateSettingsBinding,
  updateStation as updateStationBinding,
} from "./generated";
import type { BackendClient } from "./BackendClient";
import type { RuntimeContractInfo } from "./contract";
import {
  normalizeSettings,
  normalizeEndpointPingResult,
  normalizeStation,
  normalizeStationEndpointHealth,
  toCreateStationDto,
  toUpdateSettingsDto,
  toUpdateStationDto,
} from "./domainMapping";
import { RuntimeContractMismatchError } from "./runtimeContractError";

export class DesktopBackend implements BackendClient {
  readonly mode = "desktop" as const;
  readonly settings = {
    getSettings: () => getSettingsBinding().then(normalizeSettings),
    getLocalAccessKey: () => getLocalAccessKeyBinding(),
    updateLocalAccessKey: (value: string) =>
      updateLocalAccessKeyBinding({ value }).then(normalizeSettings),
    importRelayPoolToCCSwitch: () => importRelayPoolToCcswitchBinding(),
    updateSettings: (input: Parameters<BackendClient["settings"]["updateSettings"]>[0]) =>
      updateSettingsBinding(toUpdateSettingsDto(input)).then(normalizeSettings),
    chooseDataDir: () => chooseDataDirBinding().then(normalizeSettings),
    resetDataDir: () => resetDataDirBinding().then(normalizeSettings),
  };
  readonly stations = {
    listStations: () => listStationsBinding().then((stations) => stations.map(normalizeStation)),
    createStation: (input: Parameters<BackendClient["stations"]["createStation"]>[0]) =>
      createStationBinding(toCreateStationDto(input)).then(normalizeStation),
    updateStation: (input: Parameters<BackendClient["stations"]["updateStation"]>[0]) =>
      updateStationBinding(toUpdateStationDto(input)).then(normalizeStation),
    deleteStation: (id: string) => deleteStationBinding({ id }),
    openStationWebsite: (url: string) => openExternalUrlBinding({ url }),
    reorderStations: (stationIds: string[]) =>
      reorderStationsBinding({ stationIds }).then((stations) => stations.map(normalizeStation)),
    listStationEndpointHealth: () =>
      listStationEndpointHealthBinding().then((health) => health.map(normalizeStationEndpointHealth)),
    pingStationEndpoint: (stationId: string) =>
      pingStationEndpointBinding({ stationId }).then(normalizeEndpointPingResult),
  };

  async handshake(): Promise<RuntimeContractInfo> {
    const payload = await getRuntimeContractInfo();
    const validation = validateRuntimeContract(payload);
    if (!validation.ok) {
      throw new RuntimeContractMismatchError(validation.reason);
    }
    return validation.contract;
  }
}

export function createDesktopBackendClient(): BackendClient {
  return new DesktopBackend();
}
