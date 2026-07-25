import { getVersion } from "@tauri-apps/api/app";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type DownloadEvent, type Update } from "@tauri-apps/plugin-updater";
import { validateRuntimeContract } from "@/app/bootstrap/runtimeContract";
import { normalizeGroupCategory } from "@/lib/groupCategories";
import {
  bindRemoteStationKey as bindRemoteStationKeyBinding,
  deleteModelAlias as deleteModelAliasBinding,
  deletePricingRule as deletePricingRuleBinding,
  clearChangeEvents as clearChangeEventsBinding,
  clearRequestLogs as clearRequestLogsBinding,
  clearCaptureSession as clearCaptureSessionBinding,
  chooseDataDir as chooseDataDirBinding,
  closeCaptureSession as closeCaptureSessionBinding,
  clearStationCredentials as clearStationCredentialsBinding,
  activateDataStoreCandidate as activateDataStoreCandidateBinding,
  createChannelMonitor as createChannelMonitorBinding,
  createChannelMonitorTemplate as createChannelMonitorTemplateBinding,
  createLocalStationKeyFromRemote as createLocalStationKeyFromRemoteBinding,
  createNewDataStore as createNewDataStoreBinding,
  createRemoteStationKey as createRemoteStationKeyBinding,
  createStation as createStationBinding,
  createStationKey as createStationKeyBinding,
  collectStationInfo as collectStationInfoBinding,
  collectStationTask as collectStationTaskBinding,
  collectSub2apiStation as collectSub2apiStationBinding,
  deleteStation as deleteStationBinding,
  deleteStationKey as deleteStationKeyBinding,
  deleteChannelMonitor as deleteChannelMonitorBinding,
  deleteChannelMonitorTemplate as deleteChannelMonitorTemplateBinding,
  detectStationInfo as detectStationInfoBinding,
  detectSub2apiStation as detectSub2apiStationBinding,
  duplicateChannelMonitorTemplate as duplicateChannelMonitorTemplateBinding,
  exportDataStoreDiagnostic as exportDataStoreDiagnosticBinding,
  finishCaptureSession as finishCaptureSessionBinding,
  finishWebAuthorizationSession as finishWebAuthorizationSessionBinding,
  getCaptureSessionStatus as getCaptureSessionStatusBinding,
  getLatestCollectorSnapshot as getLatestCollectorSnapshotBinding,
  getRemoteKeyCapability as getRemoteKeyCapabilityBinding,
  getDataStoreStartupState as getDataStoreStartupStateBinding,
  getLocalAccessKey as getLocalAccessKeyBinding,
  getRuntimeContractInfo,
  getSettings as getSettingsBinding,
  getStationCredentials as getStationCredentialsBinding,
  getStationKeyCapabilities as getStationKeyCapabilitiesBinding,
  getStationKeyHealth as getStationKeyHealthBinding,
  importRelayPoolToCcswitch as importRelayPoolToCcswitchBinding,
  inspectLatestUpdateManifest as inspectLatestUpdateManifestBinding,
  listBalanceSnapshots as listBalanceSnapshotsBinding,
  listBalanceSnapshotsForStation as listBalanceSnapshotsForStationBinding,
  listChannelMonitorRuns as listChannelMonitorRunsBinding,
  listChannelMonitorSummaries as listChannelMonitorSummariesBinding,
  listChannelMonitorTemplates as listChannelMonitorTemplatesBinding,
  listChannelMonitors as listChannelMonitorsBinding,
  listChannelStatusSummaries as listChannelStatusSummariesBinding,
  listCollectorSnapshots as listCollectorSnapshotsBinding,
  listCurrentStationBalanceSnapshots as listCurrentStationBalanceSnapshotsBinding,
  listGroupRateRecords as listGroupRateRecordsBinding,
  listKeyPoolItems as listKeyPoolItemsBinding,
  listChangeEvents as listChangeEventsBinding,
  listChangeEventsForStation as listChangeEventsForStationBinding,
  listCollectorRuns as listCollectorRunsBinding,
  loadLocalRoutingWorkspace as loadLocalRoutingWorkspaceBinding,
  loadChannelStatusWorkspace as loadChannelStatusWorkspaceBinding,
  loadPricingComparisonWorkspace as loadPricingComparisonWorkspaceBinding,
  listModelAliases as listModelAliasesBinding,
  listModelBasePrices as listModelBasePricesBinding,
  listPricingRules as listPricingRulesBinding,
  listRemoteStationKeys as listRemoteStationKeysBinding,
  listStationEndpointHealth as listStationEndpointHealthBinding,
  listStationGroupBindings as listStationGroupBindingsBinding,
  listStationGroupOptions as listStationGroupOptionsBinding,
  listStationKeyHealth as listStationKeyHealthBinding,
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
  resetModelBasePricesToBuiltins as resetModelBasePricesToBuiltinsBinding,
  resolveStationKeyPricingContext as resolveStationKeyPricingContextBinding,
  resetDataDir as resetDataDirBinding,
  resolveChangeEvent as resolveChangeEventBinding,
  runChannelMonitorNow as runChannelMonitorNowBinding,
  saveStationKeyWithDefaults as saveStationKeyWithDefaultsBinding,
  scanRemoteStationKeys as scanRemoteStationKeysBinding,
  simulateRoute as simulateRouteBinding,
  unbindRemoteStationKey as unbindRemoteStationKeyBinding,
  updateLocalAccessKey as updateLocalAccessKeyBinding,
  upsertBalanceSnapshot as upsertBalanceSnapshotBinding,
  upsertChangeEvent as upsertChangeEventBinding,
  upsertModelAlias as upsertModelAliasBinding,
  upsertModelBasePrice as upsertModelBasePriceBinding,
  upsertPricingRule as upsertPricingRuleBinding,
  upsertStationGroupBinding as upsertStationGroupBindingBinding,
  updateSettings as updateSettingsBinding,
  updateStation as updateStationBinding,
  updateChannelMonitor as updateChannelMonitorBinding,
  updateChannelMonitorTemplate as updateChannelMonitorTemplateBinding,
  updateStationCredentials as updateStationCredentialsBinding,
  updateStationKey as updateStationKeyBinding,
  updateStationKeyCapabilities as updateStationKeyCapabilitiesBinding,
  updateStationKeyGroupBinding as updateStationKeyGroupBindingBinding,
  updateStationSession as updateStationSessionBinding,
  updaterNetworkConfig as updaterNetworkConfigBinding,
  startCaptureSession as startCaptureSessionBinding,
  startLocalProxy as startLocalProxyBinding,
  testStationLogin as testStationLoginBinding,
  testStationLoginInput as testStationLoginInputBinding,
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
import { coordinateUpdateCheck } from "./updaterCheckCoordinator";

export class DesktopBackend implements BackendClient {
  readonly mode = "desktop" as const;
  private pendingUpdate: Update | null = null;
  private nativeUpdateCheckInFlight: Promise<Update | null> | null = null;
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
  readonly collectors = {
    detectSub2apiStation: (stationId: string) => detectSub2apiStationBinding({ stationId }),
    collectSub2apiStation: (stationId: string) => collectSub2apiStationBinding({ stationId }),
    detectStationInfo: (stationId: string) => detectStationInfoBinding({ stationId }),
    collectStationInfo: (stationId: string) => collectStationInfoBinding({ stationId }),
    collectStationTask: (stationId: string, taskType: Parameters<BackendClient["collectors"]["collectStationTask"]>[1]) =>
      collectStationTaskBinding({ stationId, taskType }),
    testStationLogin: (stationId: string) => testStationLoginBinding({ stationId }),
    testStationLoginInput: (input: Parameters<BackendClient["collectors"]["testStationLoginInput"]>[0]) =>
      testStationLoginInputBinding({
        stationType: input.stationType === "newapi" ? "newapi" : "sub2api",
        websiteUrl: input.websiteUrl,
        loginUsername: input.loginUsername,
        loginPassword: input.loginPassword,
      }),
    listCollectorSnapshots: (stationId: string) => listCollectorSnapshotsBinding({ stationId }),
    getLatestCollectorSnapshot: (stationId: string) => getLatestCollectorSnapshotBinding({ stationId }),
    startCaptureSession: (stationId: string) => startCaptureSessionBinding({ stationId }),
    getCaptureSessionStatus: (stationId: string) => getCaptureSessionStatusBinding({ stationId }),
    finishCaptureSession: (stationId: string) => finishCaptureSessionBinding({ stationId }),
    finishWebAuthorizationSession: (stationId: string) => finishWebAuthorizationSessionBinding({ stationId }),
    clearCaptureSession: (stationId: string) => clearCaptureSessionBinding({ stationId }),
    closeCaptureSession: (stationId: string) => closeCaptureSessionBinding({ stationId }),
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
  readonly updater = {
    currentAppVersion: () => getVersion(),
    checkForAppUpdate: async () => {
      const currentVersion = await this.updater.currentAppVersion();

      await this.closePendingUpdateBeforeCheck();
      const network = await updaterNetworkConfigBinding()
        .catch(() => ({ proxyUrl: null }));
      const result = await coordinateUpdateCheck({
        currentVersion,
        proxyUrl: network.proxyUrl,
        checkNative: async (proxyUrl) => {
          try {
            return await withTimeout(
              this.startNativeUpdateCheck(proxyUrl),
              12_000,
              "更新检查超时",
            );
          } catch (error) {
            this.abandonNativeUpdateCheck();
            throw error;
          }
        },
        inspectPublished: (version) => inspectLatestUpdateManifestBinding({ currentVersion: version }),
      });
      if (result.kind === "current") return result;

      this.pendingUpdate = result.update;

      return {
        kind: "available" as const,
        update: {
          currentVersion: result.update.currentVersion,
          version: result.update.version,
          notes: result.update.body ?? null,
        },
      };
    },
    downloadPendingUpdate: async (onProgress: Parameters<BackendClient["updater"]["downloadPendingUpdate"]>[0]) => {
      if (!this.pendingUpdate) throw new Error("没有可下载的应用更新");
      let downloadedBytes = 0;
      let totalBytes: number | null = null;
      await this.pendingUpdate.download((event: DownloadEvent) => {
        if (event.event === "Started") {
          totalBytes = event.data.contentLength ?? null;
          onProgress({ downloadedBytes, totalBytes });
        } else if (event.event === "Progress") {
          downloadedBytes += event.data.chunkLength;
          onProgress({ downloadedBytes, totalBytes });
        } else {
          onProgress({ downloadedBytes, totalBytes });
        }
      });
    },
    installPendingUpdateAndRelaunch: async () => {
      if (!this.pendingUpdate) throw new Error("没有已下载的应用更新");
      await this.pendingUpdate.install();
      await relaunch();
    },
    closePendingUpdate: async () => {
      const update = this.pendingUpdate;
      this.pendingUpdate = null;
      await update?.close();
    },
  };
  readonly economics = {
    listPricingRules: () => listPricingRulesBinding(),
    upsertPricingRule: (input: Parameters<BackendClient["economics"]["upsertPricingRule"]>[0]) =>
      upsertPricingRuleBinding(input),
    deletePricingRule: (id: string) => deletePricingRuleBinding({ id }),
    resolveStationKeyPricingContext: (
      stationKeyId: string,
      requestedModel: string,
      requestKind: Parameters<BackendClient["economics"]["resolveStationKeyPricingContext"]>[2] = "text",
    ) => resolveStationKeyPricingContextBinding({ stationKeyId, requestedModel, requestKind }),
    listModelBasePrices: () => listModelBasePricesBinding(),
    upsertModelBasePrice: (input: Parameters<BackendClient["economics"]["upsertModelBasePrice"]>[0]) =>
      upsertModelBasePriceBinding(input),
    resetModelBasePricesToBuiltins: () => resetModelBasePricesToBuiltinsBinding(),
    listBalanceSnapshots: () => listBalanceSnapshotsBinding(),
    listCurrentStationBalanceSnapshots: () => listCurrentStationBalanceSnapshotsBinding(),
    listBalanceSnapshotsForStation: (stationId: string) => listBalanceSnapshotsForStationBinding({ stationId }),
    upsertBalanceSnapshot: (input: Parameters<BackendClient["economics"]["upsertBalanceSnapshot"]>[0]) =>
      upsertBalanceSnapshotBinding(input),
  };
  readonly groupFacts = {
    listStationGroupBindings: (stationId: string) =>
      listStationGroupBindingsBinding({ stationId }).then((bindings) => bindings.map(normalizeGroupBinding)),
    listStationGroupOptions: (stationId: string) =>
      listStationGroupOptionsBinding({ stationId }).then((options) => options.map(normalizeGroupOption)),
    listGroupRateRecords: (stationId: string) =>
      listGroupRateRecordsBinding({ stationId }).then((records) => records.map(normalizeGroupRateRecord)),
    upsertStationGroupBinding: (input: Parameters<BackendClient["groupFacts"]["upsertStationGroupBinding"]>[0]) =>
      upsertStationGroupBindingBinding(input).then(normalizeGroupBinding),
  };
  readonly pricing = {
    loadPricingComparisonWorkspace: () =>
      loadPricingComparisonWorkspaceBinding() as ReturnType<BackendClient["pricing"]["loadPricingComparisonWorkspace"]>,
  };
  readonly routing = {
    getStationKeyCapabilities: (stationKeyId: string) => getStationKeyCapabilitiesBinding({ stationKeyId }),
    updateStationKeyCapabilities: (input: Parameters<BackendClient["routing"]["updateStationKeyCapabilities"]>[0]) =>
      updateStationKeyCapabilitiesBinding(input),
    listModelAliases: () => listModelAliasesBinding(),
    upsertModelAlias: (input: Parameters<BackendClient["routing"]["upsertModelAlias"]>[0]) =>
      upsertModelAliasBinding(input),
    deleteModelAlias: (id: string) => deleteModelAliasBinding({ id }),
    listStationKeyHealth: () => listStationKeyHealthBinding(),
    getStationKeyHealth: (stationKeyId: string) => getStationKeyHealthBinding({ stationKeyId }),
    simulateRoute: (input: Parameters<BackendClient["routing"]["simulateRoute"]>[0]) =>
      simulateRouteBinding({
        endpoint: input.endpoint,
        model: input.model,
        stream: input.stream,
        usesTools: input.usesTools,
        usesVision: input.usesVision,
        usesReasoning: input.usesReasoning,
        policy: input.policy,
        maxRateMultiplier: input.maxRateMultiplier ?? null,
        routingGroupFilter: input.routingGroupFilter ?? null,
        sessionHash: input.sessionHash ?? null,
        previousResponseId: input.previousResponseId ?? null,
      }),
  };
  readonly channels = {
    listChannelMonitors: () => listChannelMonitorsBinding(),
    listChannelMonitorSummaries: (
      options: Parameters<BackendClient["channels"]["listChannelMonitorSummaries"]>[0] = {},
    ) =>
      listChannelMonitorSummariesBinding({
        runSince: options.runSince ?? null,
        runLimit: options.runLimit ?? null,
      }),
    listChannelStatusSummaries: () => listChannelStatusSummariesBinding(),
    createChannelMonitor: (input: Parameters<BackendClient["channels"]["createChannelMonitor"]>[0]) =>
      createChannelMonitorBinding(input),
    updateChannelMonitor: (input: Parameters<BackendClient["channels"]["updateChannelMonitor"]>[0]) =>
      updateChannelMonitorBinding(input),
    deleteChannelMonitor: (id: string) => deleteChannelMonitorBinding({ id }),
    runChannelMonitorNow: (monitorId: string) => runChannelMonitorNowBinding({ monitorId }),
    listChannelMonitorRuns: (monitorId: string) => listChannelMonitorRunsBinding({ monitorId }),
    listChannelMonitorTemplates: () => listChannelMonitorTemplatesBinding(),
    createChannelMonitorTemplate: (
      input: Parameters<BackendClient["channels"]["createChannelMonitorTemplate"]>[0],
    ) => createChannelMonitorTemplateBinding(input),
    updateChannelMonitorTemplate: (
      input: Parameters<BackendClient["channels"]["updateChannelMonitorTemplate"]>[0],
    ) => updateChannelMonitorTemplateBinding(input),
    duplicateChannelMonitorTemplate: (id: string) => duplicateChannelMonitorTemplateBinding({ id }),
    deleteChannelMonitorTemplate: (id: string) => deleteChannelMonitorTemplateBinding({ id }),
    loadChannelMonitoringWorkspace: async () => {
      const [monitorSummaries, stations, keyPoolItems, templates] = await Promise.all([
        listChannelMonitorSummariesBinding({ runSince: null, runLimit: null }),
        listStationsBinding().then((stations) => stations.map(normalizeStation)),
        listKeyPoolItemsBinding(),
        listChannelMonitorTemplatesBinding(),
      ]);

      return { monitorSummaries, stations, keyPoolItems, templates };
    },
    loadChannelStatusWorkspace: () =>
      loadChannelStatusWorkspaceBinding() as ReturnType<BackendClient["channels"]["loadChannelStatusWorkspace"]>,
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

  private async closePendingUpdateBeforeCheck() {
    try {
      await withTimeout(this.updater.closePendingUpdate(), 3_000, "清理旧更新检查超时");
    } catch {
      // A stale resource should not block a fresh check; pendingUpdate is already cleared.
    }
  }

  private startNativeUpdateCheck(proxyUrl: string | null) {
    if (!this.nativeUpdateCheckInFlight) {
      const trackedUpdateCheck = check(
        proxyUrl ? { timeout: 10_000, proxy: proxyUrl } : { timeout: 10_000 },
      ).finally(() => {
        if (this.nativeUpdateCheckInFlight === trackedUpdateCheck) {
          this.nativeUpdateCheckInFlight = null;
        }
      });
      this.nativeUpdateCheckInFlight = trackedUpdateCheck;
    }
    return this.nativeUpdateCheckInFlight;
  }

  private abandonNativeUpdateCheck() {
    const abandonedUpdateCheck = this.nativeUpdateCheckInFlight;
    this.nativeUpdateCheckInFlight = null;
    if (!abandonedUpdateCheck) return;
    void abandonedUpdateCheck.then(
      (update) => update?.close(),
      () => undefined,
    );
  }

  async handshake(): Promise<RuntimeContractInfo> {
    const payload = await getRuntimeContractInfo();
    const validation = validateRuntimeContract(payload);
    if (!validation.ok) {
      throw new RuntimeContractMismatchError(validation.reason);
    }
    return validation.contract;
  }
}

function withTimeout<T>(promise: Promise<T>, timeoutMs: number, message: string) {
  return new Promise<T>((resolve, reject) => {
    const timer = window.setTimeout(() => reject(new Error(message)), timeoutMs);
    promise.then(
      (value) => {
        window.clearTimeout(timer);
        resolve(value);
      },
      (error) => {
        window.clearTimeout(timer);
        reject(error);
      },
    );
  });
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

function normalizeGroupBinding(
  binding: Awaited<ReturnType<typeof upsertStationGroupBindingBinding>>,
): Awaited<ReturnType<BackendClient["groupFacts"]["upsertStationGroupBinding"]>> {
  return {
    ...binding,
    inferredGroupCategory: normalizeGroupCategory(binding.inferredGroupCategory),
    groupCategoryOverride: normalizeGroupCategory(binding.groupCategoryOverride),
  };
}

function normalizeGroupOption(
  option: Awaited<ReturnType<typeof listStationGroupOptionsBinding>>[number],
): Awaited<ReturnType<BackendClient["groupFacts"]["listStationGroupOptions"]>>[number] {
  return {
    ...option,
    inferredGroupCategory: normalizeGroupCategory(option.inferredGroupCategory),
    groupCategoryOverride: normalizeGroupCategory(option.groupCategoryOverride),
    effectiveGroupCategory: normalizeGroupCategory(option.effectiveGroupCategory) ?? "unknown",
  };
}

function normalizeGroupRateRecord(
  record: Awaited<ReturnType<typeof listGroupRateRecordsBinding>>[number],
): Awaited<ReturnType<BackendClient["groupFacts"]["listGroupRateRecords"]>>[number] {
  return {
    ...record,
    inferredGroupCategory: normalizeGroupCategory(record.inferredGroupCategory),
  };
}
