import { validateRuntimeContract } from "@/app/bootstrap/runtimeContract";
import {
  bindRemoteStationKey as bindRemoteStationKeyBinding,
  clearChangeEvents as clearChangeEventsBinding,
  clearRequestLogs as clearRequestLogsBinding,
  chooseDataDir as chooseDataDirBinding,
  clearStationCredentials as clearStationCredentialsBinding,
  activateDataStoreCandidate as activateDataStoreCandidateBinding,
  createLocalStationKeyFromRemote as createLocalStationKeyFromRemoteBinding,
  createNewDataStore as createNewDataStoreBinding,
  createRemoteStationKey as createRemoteStationKeyBinding,
  createStation as createStationBinding,
  createStationKey as createStationKeyBinding,
  deleteStation as deleteStationBinding,
  deleteStationKey as deleteStationKeyBinding,
  exportDataStoreDiagnostic as exportDataStoreDiagnosticBinding,
  getRemoteKeyCapability as getRemoteKeyCapabilityBinding,
  getDataStoreStartupState as getDataStoreStartupStateBinding,
  getLocalAccessKey as getLocalAccessKeyBinding,
  getRuntimeContractInfo,
  getSettings as getSettingsBinding,
  getStationCredentials as getStationCredentialsBinding,
  importRelayPoolToCcswitch as importRelayPoolToCcswitchBinding,
  listKeyPoolItems as listKeyPoolItemsBinding,
  listChangeEvents as listChangeEventsBinding,
  listChangeEventsForStation as listChangeEventsForStationBinding,
  listCollectorRuns as listCollectorRunsBinding,
  loadLocalRoutingWorkspace as loadLocalRoutingWorkspaceBinding,
  listRemoteStationKeys as listRemoteStationKeysBinding,
  listStationEndpointHealth as listStationEndpointHealthBinding,
  listStationKeys as listStationKeysBinding,
  listStations as listStationsBinding,
  locateDataStoreCandidate as locateDataStoreCandidateBinding,
  openExternalUrl as openExternalUrlBinding,
  openDataStoreBackupDir as openDataStoreBackupDirBinding,
  pingStationEndpoint as pingStationEndpointBinding,
  getProxyStatus as getProxyStatusBinding,
  markChangeEventRead as markChangeEventReadBinding,
  markChangeEventsRead as markChangeEventsReadBinding,
  dismissChangeEvent as dismissChangeEventBinding,
  listRequestLogs as listRequestLogsBinding,
  reorderKeyPool as reorderKeyPoolBinding,
  reorderStationKeys as reorderStationKeysBinding,
  reorderLocalRoutingKeys as reorderLocalRoutingKeysBinding,
  reorderStations as reorderStationsBinding,
  prepareLocalProxyForUpdate as prepareLocalProxyForUpdateBinding,
  refreshDataStoreCandidates as refreshDataStoreCandidatesBinding,
  resetDataDir as resetDataDirBinding,
  resolveChangeEvent as resolveChangeEventBinding,
  saveStationKeyWithDefaults as saveStationKeyWithDefaultsBinding,
  scanRemoteStationKeys as scanRemoteStationKeysBinding,
  unbindRemoteStationKey as unbindRemoteStationKeyBinding,
  updateLocalAccessKey as updateLocalAccessKeyBinding,
  upsertChangeEvent as upsertChangeEventBinding,
  updateSettings as updateSettingsBinding,
  updateStation as updateStationBinding,
  updateStationCredentials as updateStationCredentialsBinding,
  updateStationKey as updateStationKeyBinding,
  updateStationKeyGroupBinding as updateStationKeyGroupBindingBinding,
  updateStationSession as updateStationSessionBinding,
  startLocalProxy as startLocalProxyBinding,
  stopLocalProxy as stopLocalProxyBinding,
  restartLocalProxy as restartLocalProxyBinding,
} from "./generated";
import type { UpdateStationKeyInputDto } from "./generated";
import type { BackendClient } from "./BackendClient";
import type { RuntimeContractInfo } from "./contract";
import {
  normalizeSettings,
  normalizeEndpointPingResult,
  normalizeStation,
  normalizeStationEndpointHealth,
  normalizeDataStoreCandidate,
  normalizeDataStoreStartupView,
  toCreateStationDto,
  toUpdateSettingsDto,
  toUpdateStationDto,
} from "./domainMapping";
import { RuntimeContractMismatchError } from "./runtimeContractError";
import { invokeStationKeyConnectivityStream } from "./streamingAdapter";

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
  readonly changeEvents = {
    listChangeEvents: () => listChangeEventsBinding(),
    clearChangeEvents: () => clearChangeEventsBinding(),
    listChangeEventsForStation: (stationId: string) => listChangeEventsForStationBinding({ stationId }),
    upsertChangeEvent: (input: Parameters<BackendClient["changeEvents"]["upsertChangeEvent"]>[0]) =>
      upsertChangeEventBinding(input),
    markChangeEventRead: (id: string) => markChangeEventReadBinding({ id }),
    markChangeEventsRead: (ids: string[]) => markChangeEventsReadBinding({ ids }),
    dismissChangeEvent: (id: string) => dismissChangeEventBinding({ id }),
    resolveChangeEvent: (id: string) => resolveChangeEventBinding({ id }),
  };
  readonly collectorRuns = {
    listCollectorRuns: (stationId: string) => listCollectorRunsBinding({ stationId }),
  };
  readonly proxy = {
    getProxyStatus: () => getProxyStatusBinding(),
    startLocalProxy: () => startLocalProxyBinding(),
    stopLocalProxy: () => stopLocalProxyBinding(),
    restartLocalProxy: () => restartLocalProxyBinding(),
    prepareLocalProxyForUpdate: () => prepareLocalProxyForUpdateBinding(),
    listRequestLogs: () => listRequestLogsBinding(),
    clearRequestLogs: () => clearRequestLogsBinding(),
  };
  readonly localRouting = {
    loadLocalRoutingWorkspace: () => loadLocalRoutingWorkspaceBinding(),
    reorderLocalRoutingKeys: (input: Parameters<BackendClient["localRouting"]["reorderLocalRoutingKeys"]>[0]) =>
      reorderLocalRoutingKeysBinding(input),
  };
  readonly dataRecovery = {
    getDataStoreStartupState: () => getDataStoreStartupStateBinding().then(normalizeDataStoreStartupView),
    refreshDataStoreCandidates: () => refreshDataStoreCandidatesBinding().then(normalizeDataStoreStartupView),
    locateDataStoreCandidate: () => locateDataStoreCandidateBinding().then(normalizeDataStoreCandidate),
    activateDataStoreCandidate: (candidateId: string) => activateDataStoreCandidateBinding({ candidateId }),
    createNewDataStore: (confirmed: boolean) => createNewDataStoreBinding({ confirmed }),
    openDataStoreBackupDir: () => openDataStoreBackupDirBinding(),
    exportDataStoreDiagnostic: () => exportDataStoreDiagnosticBinding(),
  };
  readonly stationKeys = {
    listStationKeys: (stationId: string) => listStationKeysBinding({ stationId }),
    getRemoteKeyCapability: (stationId: string) => getRemoteKeyCapabilityBinding({ stationId }),
    listRemoteStationKeys: (stationId: string) => listRemoteStationKeysBinding({ stationId }),
    scanRemoteStationKeys: (stationId: string) => scanRemoteStationKeysBinding({ stationId }),
    createRemoteStationKey: (input: Parameters<BackendClient["stationKeys"]["createRemoteStationKey"]>[0]) =>
      createRemoteStationKeyBinding(input),
    createLocalStationKeyFromRemote: (remoteKeyId: string, stationId: string) =>
      createLocalStationKeyFromRemoteBinding({ remoteKeyId, stationId }),
    bindRemoteStationKey: (remoteKeyId: string, stationKeyId: string) =>
      bindRemoteStationKeyBinding({ remoteKeyId, stationKeyId }),
    unbindRemoteStationKey: (remoteKeyId: string, stationId: string) =>
      unbindRemoteStationKeyBinding({ remoteKeyId, stationId }),
    createStationKey: (input: Parameters<BackendClient["stationKeys"]["createStationKey"]>[0]) =>
      createStationKeyBinding(input),
    updateStationKey: (input: Parameters<BackendClient["stationKeys"]["updateStationKey"]>[0]) =>
      updateStationKeyBinding(normalizeUpdateStationKeyInput(input)),
    saveStationKeyWithDefaults: (input: Parameters<BackendClient["stationKeys"]["saveStationKeyWithDefaults"]>[0]) =>
      saveStationKeyWithDefaultsBinding(input),
    updateStationKeyGroupBinding: (stationKeyId: string, groupBindingId: string) =>
      updateStationKeyGroupBindingBinding({ stationKeyId, groupBindingId }),
    deleteStationKey: (id: string) => deleteStationKeyBinding({ id }),
    reorderStationKeys: (stationId: string, keyIds: string[]) =>
      reorderStationKeysBinding({ stationId, keyIds }),
    listKeyPoolItems: () => listKeyPoolItemsBinding(),
    reorderKeyPool: (keyIds: string[]) => reorderKeyPoolBinding({ keyIds }),
    testStationKeyConnectivity: (
      stationKeyId: string,
      model: string,
      options: Parameters<BackendClient["stationKeys"]["testStationKeyConnectivity"]>[2] = {},
    ) =>
      invokeStationKeyConnectivityStream(
        { stationKeyId, model },
        { onEvent: options.onEvent },
      ),
    getStationCredentials: (stationId: string) => getStationCredentialsBinding({ stationId }),
    updateStationCredentials: (input: Parameters<BackendClient["stationKeys"]["updateStationCredentials"]>[0]) =>
      updateStationCredentialsBinding(input),
    clearStationCredentials: (stationId: string) => clearStationCredentialsBinding({ stationId }),
    updateStationSession: (input: Parameters<BackendClient["stationKeys"]["updateStationSession"]>[0]) =>
      updateStationSessionBinding(input),
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

function normalizeUpdateStationKeyInput(
  input: Parameters<BackendClient["stationKeys"]["updateStationKey"]>[0],
): UpdateStationKeyInputDto {
  const normalized: UpdateStationKeyInputDto = {
    ...input,
    maxConcurrency: input.maxConcurrency ?? 3,
    loadFactor: input.loadFactor ?? null,
    schedulable: input.schedulable ?? true,
  };
  if ("manualRateMultiplier" in input) {
    normalized.manualRateMultiplier = input.manualRateMultiplier ?? null;
  } else {
    delete normalized.manualRateMultiplier;
  }
  return normalized;
}
