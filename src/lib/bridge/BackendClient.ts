import type { RuntimeContractInfo } from "./contract";
import type {
  CaptureSessionStatus,
  CollectorRunResult,
  CollectorSnapshot,
  CollectorTaskType,
  StationLoginTestInput,
  StationLoginTestResult,
} from "@/lib/types/collector";
import type {
  ChannelMonitor,
  ChannelMonitorRequestTemplate,
  ChannelMonitorAttemptHistoryInput,
  ChannelMonitorAttemptPage,
  ChannelMonitorExecutionDetail,
  ChannelMonitorExecutionListInput,
  ChannelMonitorExecutionPage,
  ChannelStatusWorkspace,
  ChannelStatusWorkspaceInput,
  CancelChannelMonitorExecutionReceipt,
  CreateChannelMonitorInput,
  CreateChannelMonitorTemplateInput,
  MonitoringCapabilityCatalog,
  RunChannelMonitorReceipt,
  UpdateChannelMonitorInput,
  UpdateChannelMonitorTemplateInput,
} from "@/lib/types/channelMonitors";
import type {
  AppSettings,
  CcswitchImportResult,
  CommonLoginEmail,
  CommonLoginOptions,
  CommonLoginPassword,
  UpdateSettingsInput,
  UpsertCommonLoginEmailInput,
  UpsertCommonLoginPasswordInput,
} from "@/lib/types/settings";
import type {
  BalanceSnapshot,
  ModelBasePrice,
  PricingRule,
  RequestKind,
  ResolvedPricingContext,
  UpsertBalanceSnapshotInput,
  UpsertModelBasePriceInput,
  UpsertPricingRuleInput,
} from "@/lib/types/economics";
import type {
  GroupRateRecord,
  StationGroupBinding,
  StationGroupOption,
  UpsertStationGroupBindingInput,
} from "@/lib/types/groupFacts";
import type {
  EndpointPingResult,
  Station,
  StationEndpointHealth,
  StationInput,
  StationUpdateInput,
} from "@/lib/types/stations";
import type {
  CreateLocalStationKeyFromRemoteResult,
  CreateRemoteStationKeyInput,
  CreateRemoteStationKeyResult,
  DeleteRemoteStationKeyResult,
  CreateStationKeyInput,
  KeyPoolItem,
  RemoteKeyCapability,
  RemoteKeyScanResult,
  RemoteStationKey,
  SaveStationKeyWithDefaultsInput,
  SaveStationKeyWithDefaultsResult,
  StationCredentials,
  StationKey,
  UpdateStationKeyInput,
  UpdateStationSessionInput,
} from "@/lib/types/stationKeys";
import type { CollectorRun } from "@/lib/types/collectorRuns";
import type { ActivationResult, DataStoreCandidate, DataStoreStartupView } from "@/lib/types/dataRecovery";
import type {
  InspectPortableImportInput,
  PortableExportResult,
  PortableImportInspection,
  PortableImportPrepareResult,
  PortableImportRecoveryState,
  PortableMigrationCapability,
  PortableMigrationOperation,
  PortableMigrationOperationStarted,
  PortablePathToken,
  PreparePortableImportInput,
  StartPortableExportInput,
} from "@/lib/types/dataMigration";
import type {
  DashboardCumulativeRequestMetricsSnapshot,
  DashboardLiveRequestMetricsSnapshot,
  DashboardRequestMetricsInput,
} from "@/lib/types/dashboardMetrics";
import type { ProxyStatus, RequestLog } from "@/lib/types/proxy";
import type { RuntimeStatus } from "@/lib/types/runtimeStatus";
import type {
  ProviderDraft,
  ProviderDraftPatch,
  ProviderDraftPayload,
  ProviderDraftPreview,
} from "@/lib/types/providerDrafts";
import type {
  ModelAlias,
  RecentRouteDecisionsInput,
  RecentRouteDecisionsPage,
  RequestDecisionTrace,
  RouteSimulationInput,
  RouteSimulationResult,
  RoutingRuntimeOverlay,
  RoutingWorkspaceSnapshot,
  RoutingWorkspaceSnapshotInput,
  StationKeyOperationalDetail,
  StationKeyCapabilities,
  StationKeyHealth,
  UpdateStationKeyCapabilitiesInput,
  UpsertModelAliasInput,
} from "@/lib/types/routing";
import type {
  ApplyModelMappingDocumentInputDto,
  ModelMappingDocumentDto,
  ModelMappingSimulationResultDto,
  ModelMappingTraceDto,
  ModelMappingValidationResultDto,
  ModelMappingWorkspaceDto,
  RestoreModelMappingRevisionInputDto,
  SimulateModelMappingInputDto,
  ValidateModelMappingDocumentInputDto,
} from "@/lib/types/modelMapping";
import type { AppUpdateCheckResult, DownloadProgress } from "@/lib/types/updater";
import type { AlertingDomainClient } from "@/lib/types/alerting";
import type {
  PricingGroupMonitorStatusInput,
  PricingGroupMonitorStatusWorkspace,
} from "@/lib/types/pricingMonitoring";
import type { StationPublishedStatusWorkspace } from "@/lib/types/stationPublishedStatus";
import type {
  RuntimeDiagnosticsPageDto,
  RuntimeDiagnosticsQueryDto,
  RuntimeSupportBundleResultDto,
} from "./generated";

export type BackendMode = "desktop" | "demo";

export type SettingsDomainClient = {
  getSettings(): Promise<AppSettings>;
  getLocalAccessKey(): Promise<string>;
  updateLocalAccessKey(value: string): Promise<AppSettings>;
  importRelayPoolToCCSwitch(): Promise<CcswitchImportResult>;
  updateSettings(input: UpdateSettingsInput): Promise<AppSettings>;
  chooseDataDir(): Promise<AppSettings>;
  resetDataDir(): Promise<AppSettings>;
  listCommonLoginOptions(): Promise<CommonLoginOptions>;
  upsertCommonLoginEmail(input: UpsertCommonLoginEmailInput): Promise<CommonLoginEmail>;
  deleteCommonLoginEmail(id: string): Promise<void>;
  upsertCommonLoginPassword(input: UpsertCommonLoginPasswordInput): Promise<CommonLoginPassword>;
  deleteCommonLoginPassword(id: string): Promise<void>;
  getCommonLoginPassword(id: string): Promise<string>;
};

export type StationsDomainClient = {
  listStations(): Promise<Station[]>;
  createStation(input: StationInput): Promise<Station>;
  updateStation(input: StationUpdateInput): Promise<Station>;
  deleteStation(id: string): Promise<void>;
  openStationWebsite(url: string): Promise<void>;
  reorderStations(stationIds: string[]): Promise<Station[]>;
  getStationCapacityDomain(stationId: string): Promise<import("@/lib/types/stations").StationCapacityDomain | null>;
  upsertStationCapacityDomain(input: import("@/lib/types/stations").UpsertStationCapacityDomainInput): Promise<import("@/lib/types/stations").StationCapacityDomain>;
  clearStationCapacityDomain(stationId: string, expectedRevision: number): Promise<void>;
  listStationEndpointHealth(): Promise<StationEndpointHealth[]>;
  pingStationEndpoint(stationId: string): Promise<EndpointPingResult>;
};

export type StationKeysDomainClient = {
  listStationKeys(stationId: string): Promise<StationKey[]>;
  getRemoteKeyCapability(stationId: string): Promise<RemoteKeyCapability>;
  listRemoteStationKeys(stationId: string): Promise<RemoteStationKey[]>;
  scanRemoteStationKeys(stationId: string): Promise<RemoteKeyScanResult>;
  createRemoteStationKey(input: CreateRemoteStationKeyInput): Promise<CreateRemoteStationKeyResult>;
  createLocalStationKeyFromRemote(
    remoteKeyId: string,
    stationId: string,
  ): Promise<CreateLocalStationKeyFromRemoteResult>;
  deleteRemoteStationKey(
    remoteKeyId: string,
    stationId: string,
  ): Promise<DeleteRemoteStationKeyResult>;
  bindRemoteStationKey(remoteKeyId: string, stationKeyId: string): Promise<RemoteStationKey[]>;
  unbindRemoteStationKey(remoteKeyId: string, stationId: string): Promise<RemoteStationKey[]>;
  createStationKey(input: CreateStationKeyInput): Promise<StationKey>;
  updateStationKey(input: UpdateStationKeyInput): Promise<StationKey>;
  saveStationKeyWithDefaults(input: SaveStationKeyWithDefaultsInput): Promise<SaveStationKeyWithDefaultsResult>;
  updateStationKeyGroupBinding(stationKeyId: string, groupBindingId: string): Promise<StationKey>;
  deleteStationKey(id: string): Promise<void>;
  reorderStationKeys(stationId: string, keyIds: string[]): Promise<StationKey[]>;
  listKeyPoolItems(): Promise<KeyPoolItem[]>;
  reorderKeyPool(keyIds: string[]): Promise<KeyPoolItem[]>;
  getStationCredentials(stationId: string): Promise<StationCredentials>;
  updateStationCredentials(input: {
    stationId: string;
    loginUsername: string | null;
    loginPassword: string | null;
    rememberPassword: boolean;
  }): Promise<StationCredentials>;
  clearStationCredentials(stationId: string): Promise<StationCredentials>;
  updateStationSession(input: UpdateStationSessionInput): Promise<StationCredentials>;
};

export type CollectorRunsDomainClient = {
  listCollectorRuns(stationId: string): Promise<CollectorRun[]>;
};

export type ProxyDomainClient = {
  getProxyStatus(): Promise<ProxyStatus>;
  startLocalProxy(): Promise<ProxyStatus>;
  stopLocalProxy(): Promise<ProxyStatus>;
  restartLocalProxy(): Promise<ProxyStatus>;
  prepareLocalProxyForUpdate(): Promise<ProxyStatus>;
  listRequestLogs(): Promise<RequestLog[]>;
  clearRequestLogs(): Promise<void>;
};

export type DashboardDomainClient = {
  loadLiveRequestMetrics(input: DashboardRequestMetricsInput): Promise<DashboardLiveRequestMetricsSnapshot>;
  loadCumulativeRequestMetrics(): Promise<DashboardCumulativeRequestMetricsSnapshot>;
};

export type RuntimeDomainClient = {
  getRuntimeStatus(): Promise<RuntimeStatus>;
};

export type RuntimeDiagnosticsDomainClient = {
  readRuntimeDiagnostics(input?: RuntimeDiagnosticsQueryDto): Promise<RuntimeDiagnosticsPageDto>;
  exportRuntimeSupportBundle(): Promise<RuntimeSupportBundleResultDto | null>;
  openRuntimeLogDirectory(): Promise<void>;
  openRuntimeLogFile(): Promise<void>;
};

export type DataRecoveryDomainClient = {
  getDataStoreStartupState(): Promise<DataStoreStartupView>;
  refreshDataStoreCandidates(): Promise<DataStoreStartupView>;
  locateDataStoreCandidate(): Promise<DataStoreCandidate | null>;
  activateDataStoreCandidate(candidateId: string): Promise<ActivationResult>;
  createNewDataStore(confirmed: boolean): Promise<ActivationResult>;
  restartApp(): Promise<void>;
  openDataStoreBackupDir(): Promise<void>;
  exportDataStoreDiagnostic(): Promise<string | null>;
};

export type DataMigrationDomainClient = {
  getPortableMigrationCapability(): Promise<PortableMigrationCapability>;
  choosePortableExportPath(): Promise<PortablePathToken | null>;
  startPortableExport(input: StartPortableExportInput): Promise<PortableMigrationOperationStarted>;
  getPortableExportResult(resourceId: string): Promise<PortableExportResult>;
  choosePortableImportFile(): Promise<PortablePathToken | null>;
  startPortableImportInspection(input: InspectPortableImportInput): Promise<PortableMigrationOperationStarted>;
  getPortableImportInspection(resourceId: string): Promise<PortableImportInspection>;
  startPortableImportPrepare(input: PreparePortableImportInput): Promise<PortableMigrationOperationStarted>;
  getPortableImportPrepareResult(resourceId: string): Promise<PortableImportPrepareResult>;
  getPortableMigrationOperation(operationId: string): Promise<PortableMigrationOperation>;
  getPortableImportRecoveryState(): Promise<PortableImportRecoveryState>;
};

export type EconomicsDomainClient = {
  listPricingRules(): Promise<PricingRule[]>;
  upsertPricingRule(input: UpsertPricingRuleInput): Promise<PricingRule>;
  deletePricingRule(id: string): Promise<void>;
  resolveStationKeyPricingContext(
    stationKeyId: string,
    requestedModel: string,
    requestKind?: RequestKind,
  ): Promise<ResolvedPricingContext>;
  listModelBasePrices(): Promise<ModelBasePrice[]>;
  upsertModelBasePrice(input: UpsertModelBasePriceInput): Promise<ModelBasePrice>;
  resetModelBasePricesToBuiltins(): Promise<ModelBasePrice[]>;
  listBalanceSnapshots(): Promise<BalanceSnapshot[]>;
  listCurrentStationBalanceSnapshots(): Promise<BalanceSnapshot[]>;
  listBalanceSnapshotsForStation(stationId: string): Promise<BalanceSnapshot[]>;
  upsertBalanceSnapshot(input: UpsertBalanceSnapshotInput): Promise<BalanceSnapshot>;
};

export type GroupFactsDomainClient = {
  listStationGroupBindings(stationId: string): Promise<StationGroupBinding[]>;
  listStationGroupOptions(stationId: string): Promise<StationGroupOption[]>;
  listGroupRateRecords(stationId: string): Promise<GroupRateRecord[]>;
  upsertStationGroupBinding(input: UpsertStationGroupBindingInput): Promise<StationGroupBinding>;
};

export type PricingComparisonWorkspace = {
  stations: Station[];
  stationKeys: StationKey[];
  groupBindings: StationGroupBinding[];
  groupRates: GroupRateRecord[];
  pricingRules: PricingRule[];
  developerModeEnabled: boolean;
};

export type PricingDomainClient = {
  loadPricingComparisonWorkspace(): Promise<PricingComparisonWorkspace>;
  loadPricingGroupMonitorStatus(
    input: PricingGroupMonitorStatusInput,
  ): Promise<PricingGroupMonitorStatusWorkspace>;
};

export type RoutingDomainClient = {
  getStationKeyCapabilities(stationKeyId: string): Promise<StationKeyCapabilities>;
  updateStationKeyCapabilities(input: UpdateStationKeyCapabilitiesInput): Promise<StationKeyCapabilities>;
  listModelAliases(): Promise<ModelAlias[]>;
  upsertModelAlias(input: UpsertModelAliasInput): Promise<ModelAlias>;
  deleteModelAlias(id: string): Promise<void>;
  listStationKeyHealth(): Promise<StationKeyHealth[]>;
  loadRoutingPolicy(): Promise<import("./generated").RoutingPolicySnapshotDto>;
  updateRoutingPolicy(input: import("./generated").UpdateRoutingPolicyInputDto): Promise<import("./generated").RoutingPolicySnapshotDto>;
  applyRoutingPolicyDocument(input: import("./generated").ApplyRoutingPolicyDocumentInputDto): Promise<import("./generated").RoutingPolicySnapshotDto>;
  loadRoutingWorkspaceSnapshot(input?: RoutingWorkspaceSnapshotInput): Promise<RoutingWorkspaceSnapshot>;
  loadRoutingRuntimeOverlay(): Promise<RoutingRuntimeOverlay>;
  listRecentRouteDecisions(input?: RecentRouteDecisionsInput): Promise<RecentRouteDecisionsPage>;
  getStationKeyOperationalDetail(stationKeyId: string): Promise<StationKeyOperationalDetail>;
  getRequestDecisionTrace(requestLogId: string): Promise<RequestDecisionTrace>;
  getStationKeyHealth(stationKeyId: string): Promise<StationKeyHealth>;
  simulateRoute(input: RouteSimulationInput): Promise<RouteSimulationResult>;
  getModelMappingWorkspace(): Promise<ModelMappingWorkspaceDto>;
  getModelMappingDocument(): Promise<ModelMappingDocumentDto>;
  validateModelMappingDocument(input: ValidateModelMappingDocumentInputDto): Promise<ModelMappingValidationResultDto>;
  applyModelMappingDocument(input: ApplyModelMappingDocumentInputDto): Promise<ModelMappingWorkspaceDto>;
  restoreModelMappingRevision(input: RestoreModelMappingRevisionInputDto): Promise<ModelMappingWorkspaceDto>;
  simulateModelMapping(input: SimulateModelMappingInputDto): Promise<ModelMappingSimulationResultDto>;
  resolveRequestMappingTrace(requestLogId: string): Promise<ModelMappingTraceDto>;
};

export type ChannelMonitoringWorkspace = {
  monitors: ChannelMonitor[];
  statusWorkspace: ChannelStatusWorkspace;
  stations: Station[];
  keyPoolItems: KeyPoolItem[];
  templates: ChannelMonitorRequestTemplate[];
};

export type ChannelsDomainClient = {
  listChannelMonitors(): Promise<ChannelMonitor[]>;
  createChannelMonitor(input: CreateChannelMonitorInput): Promise<ChannelMonitor>;
  updateChannelMonitor(input: UpdateChannelMonitorInput): Promise<ChannelMonitor>;
  deleteChannelMonitor(id: string): Promise<void>;
  runChannelMonitorNow(monitorId: string, triggerRequestId?: string): Promise<RunChannelMonitorReceipt>;
  cancelChannelMonitorExecution(executionId: string): Promise<CancelChannelMonitorExecutionReceipt>;
  listChannelMonitorExecutions(input?: ChannelMonitorExecutionListInput): Promise<ChannelMonitorExecutionPage>;
  getChannelMonitorExecution(executionId: string): Promise<ChannelMonitorExecutionDetail>;
  listChannelMonitorAttempts(input: ChannelMonitorAttemptHistoryInput): Promise<ChannelMonitorAttemptPage>;
  listMonitoringCapabilities(): Promise<MonitoringCapabilityCatalog>;
  listChannelMonitorTemplates(): Promise<ChannelMonitorRequestTemplate[]>;
  createChannelMonitorTemplate(input: CreateChannelMonitorTemplateInput): Promise<ChannelMonitorRequestTemplate>;
  updateChannelMonitorTemplate(input: UpdateChannelMonitorTemplateInput): Promise<ChannelMonitorRequestTemplate>;
  duplicateChannelMonitorTemplate(id: string): Promise<ChannelMonitorRequestTemplate>;
  deleteChannelMonitorTemplate(id: string): Promise<void>;
  loadChannelMonitoringWorkspace(): Promise<ChannelMonitoringWorkspace>;
  loadChannelStatusWorkspace(input?: ChannelStatusWorkspaceInput): Promise<ChannelStatusWorkspace>;
};

export type CollectorsDomainClient = {
  detectSub2apiStation(stationId: string): Promise<CollectorRunResult>;
  collectSub2apiStation(stationId: string): Promise<CollectorRunResult>;
  detectStationInfo(stationId: string): Promise<CollectorRunResult>;
  collectStationInfo(stationId: string): Promise<CollectorRunResult>;
  collectStationTask(stationId: string, taskType: CollectorTaskType): Promise<CollectorRunResult>;
  scanStationRecharge(stationId: string): Promise<CollectorRunResult>;
  testStationLogin(stationId: string): Promise<CollectorRunResult>;
  testStationLoginInput(input: StationLoginTestInput): Promise<StationLoginTestResult>;
  listCollectorSnapshots(stationId: string): Promise<CollectorSnapshot[]>;
  getLatestCollectorSnapshot(stationId: string): Promise<CollectorSnapshot | null>;
  listLatestCollectorSnapshots(stationIds: string[]): Promise<CollectorSnapshot[]>;
  startCaptureSession(stationId: string): Promise<CaptureSessionStatus>;
  getCaptureSessionStatus(stationId: string): Promise<CaptureSessionStatus>;
  finishCaptureSession(stationId: string): Promise<CollectorRunResult>;
  finishWebAuthorizationSession(stationId: string): Promise<CollectorRunResult>;
  clearCaptureSession(stationId: string): Promise<CaptureSessionStatus>;
  closeCaptureSession(stationId: string): Promise<CaptureSessionStatus>;
};

export type StationPublishedStatusDomainClient = {
  getStationPublishedStatusWorkspace(stationId: string): Promise<StationPublishedStatusWorkspace>;
};

export type ProviderDraftsDomainClient = {
  createOrResume(input: { baseStationId: string | null; payload: ProviderDraftPayload }): Promise<ProviderDraft>;
  get(draftId: string): Promise<ProviderDraft>;
  patch(input: ProviderDraftPatch): Promise<ProviderDraft>;
  discard(draftId: string): Promise<void>;
  collectPreview(input: {
    draftId: string;
    taskType: "detect" | "balance" | "groups" | "models" | "full";
  }): Promise<ProviderDraftPreview>;
  scanRemoteKeys(draftId: string): Promise<RemoteKeyScanResult>;
  startAuthorization(draftId: string): Promise<CaptureSessionStatus>;
  commit(input: { draftId: string; expectedRevision: number; commitKey: string }): Promise<Station>;
};

export type UpdaterDomainClient = {
  currentAppVersion(): Promise<string>;
  checkForAppUpdate(): Promise<AppUpdateCheckResult>;
  downloadPendingUpdate(onProgress: (progress: DownloadProgress) => void): Promise<void>;
  installPendingUpdateAndRelaunch(): Promise<void>;
  closePendingUpdate(): Promise<void>;
};

export type BackendClient = {
  readonly mode: BackendMode;
  readonly settings: SettingsDomainClient;
  readonly stations: StationsDomainClient;
  readonly stationKeys: StationKeysDomainClient;
  readonly alerting: AlertingDomainClient;
  readonly collectorRuns: CollectorRunsDomainClient;
  readonly proxy: ProxyDomainClient;
  readonly dashboard: DashboardDomainClient;
  readonly dataRecovery: DataRecoveryDomainClient;
  readonly dataMigration: DataMigrationDomainClient;
  readonly economics: EconomicsDomainClient;
  readonly groupFacts: GroupFactsDomainClient;
  readonly pricing: PricingDomainClient;
  readonly routing: RoutingDomainClient;
  readonly channels: ChannelsDomainClient;
  readonly collectors: CollectorsDomainClient;
  readonly stationPublishedStatus?: StationPublishedStatusDomainClient;
  readonly providerDrafts?: ProviderDraftsDomainClient;
  readonly updater: UpdaterDomainClient;
  readonly runtime: RuntimeDomainClient;
  readonly runtimeDiagnostics?: RuntimeDiagnosticsDomainClient;
  handshake(): Promise<RuntimeContractInfo>;
};

export class DemoBackendUnsupportedError extends Error {
  readonly code = "unsupported" as const;
  readonly retryable = false;
  readonly capability: string;

  constructor(capability: string) {
    super(`Demo backend does not support '${capability}'.`);
    this.name = "DemoBackendUnsupportedError";
    this.capability = capability;
  }
}
