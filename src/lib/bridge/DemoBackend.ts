import { IPC_BINDING_HASH, IPC_CONTRACT_VERSION } from "./contract";
import { DemoBackendUnsupportedError, type BackendClient } from "./BackendClient";
import type { RuntimeContractInfo } from "./contract";
import type {
  AlertPolicy,
  AlertPolicyInput,
  AlertingActivityInput,
  AlertingActivityPage,
  AlertingCurrentInput,
  AlertingHistoryInput,
  AlertingDomainClient,
  AlertingIncidentInput,
  AlertingIncidentPage,
  AlertingSettings,
  AlertingSettingsInput,
} from "@/lib/types/alerting";
import { DEFAULT_ALERTING_SETTINGS, defaultAlertPolicy } from "@/lib/types/alerting";

const DEMO_SEED = "relay-pool-demo-v1";
const DEMO_CLOCK_ISO = "2026-07-22T00:00:00.000Z";

type DemoStore = {
  readonly seed: string;
  readonly clockIso: string;
};

export class DemoBackend implements BackendClient {
  readonly mode = "demo" as const;
  private alertingSettings: AlertingSettings = { ...DEFAULT_ALERTING_SETTINGS };
  private alertingPolicies: AlertPolicy[] = [
    defaultAlertPolicy("collector_failed"),
    defaultAlertPolicy("station_down"),
    defaultAlertPolicy("balance_low"),
  ];
  readonly settings: BackendClient["settings"] = {
    getSettings: () => this.rejectUnsupported("settings"),
    getLocalAccessKey: () => this.rejectUnsupported("settings.local_access_key"),
    updateLocalAccessKey: (_value: string) => this.rejectUnsupported("settings.local_access_key"),
    importRelayPoolToCCSwitch: () => this.rejectUnsupported("settings.ccswitch_import"),
    updateSettings: () => this.rejectUnsupported("settings"),
    chooseDataDir: () => this.rejectUnsupported("settings.data_dir"),
    resetDataDir: () => this.rejectUnsupported("settings.data_dir"),
    listCommonLoginOptions: () => this.rejectUnsupported("settings.common_login_options"),
    upsertCommonLoginEmail: () => this.rejectUnsupported("settings.common_login_options"),
    deleteCommonLoginEmail: (_id: string) => this.rejectUnsupported("settings.common_login_options"),
    upsertCommonLoginPassword: () => this.rejectUnsupported("settings.common_login_options"),
    deleteCommonLoginPassword: (_id: string) => this.rejectUnsupported("settings.common_login_options"),
    getCommonLoginPassword: (_id: string) =>
      this.rejectUnsupported("settings.common_login_options"),
  };
  readonly alerting: AlertingDomainClient = {
    loadWorkspace: async () => ({
      settings: { ...this.alertingSettings },
      policies: this.alertingPolicies.map((policy) => ({ ...policy })),
    }),
    getSettings: async () => ({ ...this.alertingSettings }),
    updateSettings: async (input: AlertingSettingsInput) => {
      this.alertingSettings = {
        ...this.alertingSettings,
        ...input,
        revision: this.alertingSettings.revision + 1,
        updatedAtMs: Date.now(),
      };
      return { ...this.alertingSettings };
    },
    listPolicies: async () => this.alertingPolicies.map((policy) => ({ ...policy })),
    upsertPolicy: async (input: AlertPolicyInput) => {
      const id = input.id ?? `demo-policy-${Date.now()}`;
      const current = this.alertingPolicies.find((policy) => policy.id === id);
      const policy = {
        ...input,
        id,
        revision: (current?.revision ?? 0) + 1,
        createdAtMs: current?.createdAtMs ?? Date.now(),
        updatedAtMs: Date.now(),
      } as AlertPolicy;
      this.alertingPolicies = current
        ? this.alertingPolicies.map((item) => item.id === id ? policy : item)
        : [...this.alertingPolicies, policy];
      return { ...policy };
    },
    deletePolicy: async (id: string) => {
      this.alertingPolicies = this.alertingPolicies.filter((policy) => policy.id !== id);
    },
    listCurrentIncidents: async (_input: AlertingCurrentInput = {}): Promise<AlertingIncidentPage> => ({
      items: [],
      nextCursor: null,
      activeCount: 0,
      unseenCount: 0,
    }),
    listActivity: async (_input: AlertingActivityInput = {}): Promise<AlertingActivityPage> => ({
      items: [],
      nextCursor: null,
      activeCount: 0,
      unseenCount: 0,
    }),
    getIncident: async (_input: AlertingIncidentInput) => {
      throw new DemoBackendUnsupportedError("alerting.incident");
    },
    listOccurrences: async (_input: AlertingHistoryInput) => ({ items: [], nextCursor: null }),
    listDeliveries: async (_input: AlertingHistoryInput) => ({ items: [], nextCursor: null }),
    markSeen: async () => undefined,
    markAllSeen: async () => 0,
    resolveAllActive: async () => 0,
    clearActivity: async () => 0,
    snooze: async () => undefined,
    sendTestNotification: async () => undefined,
    getDesktopNotificationPermission: async () => "unavailable",
    requestDesktopNotificationPermission: async () => "unavailable",
  };
  readonly stations: BackendClient["stations"] = {
    listStations: () => this.rejectUnsupported("stations"),
    createStation: () => this.rejectUnsupported("stations"),
    updateStation: () => this.rejectUnsupported("stations"),
    getStationCapacityDomain: () => this.rejectUnsupported("stations"),
    upsertStationCapacityDomain: () => this.rejectUnsupported("stations"),
    clearStationCapacityDomain: () => this.rejectUnsupported("stations"),
    deleteStation: (_id: string) => this.rejectUnsupported("stations"),
    openStationWebsite: (_url: string) => this.rejectUnsupported("stations.external_url"),
    reorderStations: () => this.rejectUnsupported("stations"),
    listStationEndpointHealth: () => this.rejectUnsupported("stations.endpoint_health"),
    pingStationEndpoint: (_stationId: string) => this.rejectUnsupported("stations.endpoint_ping"),
  };
  readonly collectorRuns: BackendClient["collectorRuns"] = {
    listCollectorRuns: (_stationId: string) => this.rejectUnsupported("collector_runs"),
  };
  readonly collectors: BackendClient["collectors"] = {
    detectSub2apiStation: (_stationId: string) => this.rejectUnsupported("collectors"),
    collectSub2apiStation: (_stationId: string) => this.rejectUnsupported("collectors"),
    detectStationInfo: (_stationId: string) => this.rejectUnsupported("collectors"),
    collectStationInfo: (_stationId: string) => this.rejectUnsupported("collectors"),
    collectStationTask: (_stationId: string) => this.rejectUnsupported("collectors"),
    testStationLogin: (_stationId: string) => this.rejectUnsupported("collectors"),
    testStationLoginInput: () => this.rejectUnsupported("collectors"),
    listCollectorSnapshots: (_stationId: string) => this.rejectUnsupported("collectors"),
    getLatestCollectorSnapshot: (_stationId: string) => this.rejectUnsupported("collectors"),
    listLatestCollectorSnapshots: (_stationIds: string[]) => this.rejectUnsupported("collectors"),
    startCaptureSession: (_stationId: string) => this.rejectUnsupported("collectors"),
    getCaptureSessionStatus: (_stationId: string) => this.rejectUnsupported("collectors"),
    finishCaptureSession: (_stationId: string) => this.rejectUnsupported("collectors"),
    finishWebAuthorizationSession: (_stationId: string) => this.rejectUnsupported("collectors"),
    clearCaptureSession: (_stationId: string) => this.rejectUnsupported("collectors"),
    closeCaptureSession: (_stationId: string) => this.rejectUnsupported("collectors"),
  };
  readonly providerDrafts: BackendClient["providerDrafts"] = {
    createOrResume: () => this.rejectUnsupported("provider_drafts"),
    get: (_draftId: string) => this.rejectUnsupported("provider_drafts"),
    patch: () => this.rejectUnsupported("provider_drafts"),
    discard: (_draftId: string) => this.rejectUnsupported("provider_drafts"),
    collectPreview: () => this.rejectUnsupported("provider_drafts"),
    scanRemoteKeys: (_draftId: string) => this.rejectUnsupported("provider_drafts"),
    startAuthorization: (_draftId: string) => this.rejectUnsupported("provider_drafts"),
    commit: () => this.rejectUnsupported("provider_drafts"),
  };
  readonly updater: BackendClient["updater"] = {
    currentAppVersion: async () => "0.0.0",
    checkForAppUpdate: async () => ({ kind: "unsupported", currentVersion: "0.0.0" }),
    downloadPendingUpdate: () => this.rejectUnsupported("updater"),
    installPendingUpdateAndRelaunch: () => this.rejectUnsupported("updater"),
    closePendingUpdate: async () => undefined,
  };
  readonly proxy: BackendClient["proxy"] = {
    getProxyStatus: () => this.rejectUnsupported("proxy"),
    startLocalProxy: () => this.rejectUnsupported("proxy"),
    stopLocalProxy: () => this.rejectUnsupported("proxy"),
    restartLocalProxy: () => this.rejectUnsupported("proxy"),
    prepareLocalProxyForUpdate: () => this.rejectUnsupported("proxy"),
    listRequestLogs: () => this.rejectUnsupported("proxy"),
    clearRequestLogs: () => this.rejectUnsupported("proxy"),
  };
  readonly dashboard: BackendClient["dashboard"] = {
    loadLiveRequestMetrics: () => this.rejectUnsupported("dashboard.request_metrics"),
    loadCumulativeRequestMetrics: () => this.rejectUnsupported("dashboard.request_metrics"),
  };
  readonly runtime: BackendClient["runtime"] = {
    getRuntimeStatus: () => this.rejectUnsupported("runtime_status"),
  };
  readonly runtimeDiagnostics: BackendClient["runtimeDiagnostics"] = {
    readRuntimeDiagnostics: () => this.rejectUnsupported("runtime_diagnostics"),
    exportRuntimeSupportBundle: () => this.rejectUnsupported("runtime_diagnostics"),
    openRuntimeLogDirectory: () => this.rejectUnsupported("runtime_diagnostics"),
    openRuntimeLogFile: () => this.rejectUnsupported("runtime_diagnostics"),
  };
  readonly dataRecovery: BackendClient["dataRecovery"] = {
    getDataStoreStartupState: () => this.rejectUnsupported("data_recovery"),
    refreshDataStoreCandidates: () => this.rejectUnsupported("data_recovery"),
    locateDataStoreCandidate: () => this.rejectUnsupported("data_recovery"),
    activateDataStoreCandidate: (_candidateId: string) => this.rejectUnsupported("data_recovery"),
    createNewDataStore: (_confirmed: boolean) => this.rejectUnsupported("data_recovery"),
    restartApp: () => this.rejectUnsupported("data_recovery.restart"),
    openDataStoreBackupDir: () => this.rejectUnsupported("data_recovery"),
    exportDataStoreDiagnostic: () => this.rejectUnsupported("data_recovery"),
  };
  readonly dataMigration: BackendClient["dataMigration"] = {
    getPortableMigrationCapability: () => this.rejectUnsupported("data_migration"),
    choosePortableExportPath: () => this.rejectUnsupported("data_migration"),
    startPortableExport: () => this.rejectUnsupported("data_migration"),
    getPortableExportResult: () => this.rejectUnsupported("data_migration"),
    choosePortableImportFile: () => this.rejectUnsupported("data_migration"),
    startPortableImportInspection: () => this.rejectUnsupported("data_migration"),
    getPortableImportInspection: () => this.rejectUnsupported("data_migration"),
    startPortableImportPrepare: () => this.rejectUnsupported("data_migration"),
    getPortableImportPrepareResult: () => this.rejectUnsupported("data_migration"),
    getPortableMigrationOperation: () => this.rejectUnsupported("data_migration"),
    getPortableImportRecoveryState: () => this.rejectUnsupported("data_migration"),
  };
  readonly economics: BackendClient["economics"] = {
    listPricingRules: () => this.rejectUnsupported("economics"),
    upsertPricingRule: () => this.rejectUnsupported("economics"),
    deletePricingRule: (_id: string) => this.rejectUnsupported("economics"),
    resolveStationKeyPricingContext: (_stationKeyId: string, _requestedModel: string) =>
      this.rejectUnsupported("economics"),
    listModelBasePrices: () => this.rejectUnsupported("economics"),
    upsertModelBasePrice: () => this.rejectUnsupported("economics"),
    resetModelBasePricesToBuiltins: () => this.rejectUnsupported("economics"),
    listBalanceSnapshots: () => this.rejectUnsupported("economics"),
    listCurrentStationBalanceSnapshots: () => this.rejectUnsupported("economics"),
    listBalanceSnapshotsForStation: (_stationId: string) => this.rejectUnsupported("economics"),
    upsertBalanceSnapshot: () => this.rejectUnsupported("economics"),
  };
  readonly groupFacts: BackendClient["groupFacts"] = {
    listStationGroupBindings: (_stationId: string) => this.rejectUnsupported("group_facts"),
    listStationGroupOptions: (_stationId: string) => this.rejectUnsupported("group_facts"),
    listGroupRateRecords: (_stationId: string) => this.rejectUnsupported("group_facts"),
    upsertStationGroupBinding: () => this.rejectUnsupported("group_facts"),
  };
  readonly pricing: BackendClient["pricing"] = {
    loadPricingComparisonWorkspace: () => this.rejectUnsupported("pricing"),
    loadPricingGroupMonitorStatus: () => this.rejectUnsupported("pricing.monitor_status"),
  };
  readonly routing: BackendClient["routing"] = {
    getStationKeyCapabilities: (_stationKeyId: string) => this.rejectUnsupported("routing"),
    updateStationKeyCapabilities: () => this.rejectUnsupported("routing"),
    listModelAliases: () => this.rejectUnsupported("routing"),
    upsertModelAlias: () => this.rejectUnsupported("routing"),
    deleteModelAlias: (_id: string) => this.rejectUnsupported("routing"),
    listStationKeyHealth: () => this.rejectUnsupported("routing"),
    loadRoutingPolicy: () => this.rejectUnsupported("routing.policy"),
    updateRoutingPolicy: () => this.rejectUnsupported("routing.policy"),
    loadRoutingWorkspaceSnapshot: () => this.rejectUnsupported("routing.workspace_snapshot"),
    loadRoutingRuntimeOverlay: () => this.rejectUnsupported("routing.runtime_overlay"),
    listRecentRouteDecisions: () => this.rejectUnsupported("routing.route_decisions"),
    getStationKeyOperationalDetail: (_stationKeyId: string) =>
      this.rejectUnsupported("routing.operational_detail"),
    getRequestDecisionTrace: (_requestLogId: string) =>
      this.rejectUnsupported("routing.decision_trace"),
    getStationKeyHealth: (_stationKeyId: string) => this.rejectUnsupported("routing"),
    simulateRoute: () => this.rejectUnsupported("routing"),
  };
  readonly channels: BackendClient["channels"] = {
    listChannelMonitors: () => this.rejectUnsupported("channels"),
    createChannelMonitor: () => this.rejectUnsupported("channels"),
    updateChannelMonitor: () => this.rejectUnsupported("channels"),
    deleteChannelMonitor: (_id: string) => this.rejectUnsupported("channels"),
    runChannelMonitorNow: (_monitorId: string) => this.rejectUnsupported("channels"),
    cancelChannelMonitorExecution: (_executionId: string) => this.rejectUnsupported("channels"),
    listChannelMonitorExecutions: () => this.rejectUnsupported("channels"),
    getChannelMonitorExecution: (_executionId: string) => this.rejectUnsupported("channels"),
    listChannelMonitorAttempts: () => this.rejectUnsupported("channels"),
    listMonitoringCapabilities: () => this.rejectUnsupported("channels"),
    listChannelMonitorTemplates: () => this.rejectUnsupported("channels"),
    createChannelMonitorTemplate: () => this.rejectUnsupported("channels"),
    updateChannelMonitorTemplate: () => this.rejectUnsupported("channels"),
    duplicateChannelMonitorTemplate: (_id: string) => this.rejectUnsupported("channels"),
    deleteChannelMonitorTemplate: (_id: string) => this.rejectUnsupported("channels"),
    loadChannelMonitoringWorkspace: () => this.rejectUnsupported("channels"),
    loadChannelStatusWorkspace: () => this.rejectUnsupported("channels"),
  };
  readonly stationKeys: BackendClient["stationKeys"] = {
    listStationKeys: (_stationId: string) => this.rejectUnsupported("station_keys"),
    getRemoteKeyCapability: (_stationId: string) => this.rejectUnsupported("station_keys.remote_key"),
    listRemoteStationKeys: (_stationId: string) => this.rejectUnsupported("station_keys.remote_key"),
    scanRemoteStationKeys: (_stationId: string) => this.rejectUnsupported("station_keys.remote_key_scan"),
    createRemoteStationKey: () => this.rejectUnsupported("station_keys.remote_key"),
    createLocalStationKeyFromRemote: (_remoteKeyId: string, _stationId: string) =>
      this.rejectUnsupported("station_keys.remote_key"),
    deleteRemoteStationKey: (_remoteKeyId: string, _stationId: string) =>
      this.rejectUnsupported("station_keys.remote_key"),
    bindRemoteStationKey: (_remoteKeyId: string, _stationKeyId: string) =>
      this.rejectUnsupported("station_keys.remote_key_binding"),
    unbindRemoteStationKey: (_remoteKeyId: string, _stationId: string) =>
      this.rejectUnsupported("station_keys.remote_key_binding"),
    createStationKey: () => this.rejectUnsupported("station_keys"),
    updateStationKey: () => this.rejectUnsupported("station_keys"),
    saveStationKeyWithDefaults: () => this.rejectUnsupported("station_keys.defaults"),
    updateStationKeyGroupBinding: (_stationKeyId: string, _groupBindingId: string) =>
      this.rejectUnsupported("station_keys.group_binding"),
    deleteStationKey: (_id: string) => this.rejectUnsupported("station_keys"),
    reorderStationKeys: (_stationId: string, _keyIds: string[]) => this.rejectUnsupported("station_keys.reorder"),
    listKeyPoolItems: () => this.rejectUnsupported("station_keys.key_pool"),
    reorderKeyPool: (_keyIds: string[]) => this.rejectUnsupported("station_keys.key_pool"),
    getStationCredentials: (_stationId: string) => this.rejectUnsupported("station_keys.credentials"),
    updateStationCredentials: () => this.rejectUnsupported("station_keys.credentials"),
    clearStationCredentials: (_stationId: string) => this.rejectUnsupported("station_keys.credentials"),
    updateStationSession: () => this.rejectUnsupported("station_keys.session"),
  };
  private store: DemoStore = createInitialStore();

  async handshake(): Promise<RuntimeContractInfo> {
    return {
      appVersion: `demo-${this.store.seed}`,
      ipcContractVersion: IPC_CONTRACT_VERSION,
      bindingHash: IPC_BINDING_HASH,
      capabilities: ["runtime_contract"],
    };
  }

  reset(): void {
    this.store = createInitialStore();
  }

  unsupported(capability: string): never {
    throw new DemoBackendUnsupportedError(capability);
  }

  private rejectUnsupported<T>(capability: string): Promise<T> {
    return Promise.reject(new DemoBackendUnsupportedError(capability));
  }
}

export function createDemoBackendClient(): BackendClient {
  return new DemoBackend();
}

function createInitialStore(): DemoStore {
  return {
    seed: DEMO_SEED,
    clockIso: DEMO_CLOCK_ISO,
  };
}
