import type { RuntimeContractInfo } from "./contract";
import type { AppSettings, CcswitchImportResult, UpdateSettingsInput } from "@/lib/types/settings";
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
  CreateStationKeyInput,
  KeyPoolItem,
  RemoteKeyCapability,
  RemoteKeyScanResult,
  RemoteStationKey,
  SaveStationKeyWithDefaultsInput,
  SaveStationKeyWithDefaultsResult,
  StationCredentials,
  StationKey,
  StationKeyConnectivityTestEvent,
  StationKeyConnectivityTestResult,
  UpdateStationKeyInput,
  UpdateStationSessionInput,
} from "@/lib/types/stationKeys";

export type BackendMode = "desktop" | "demo";

export type SettingsDomainClient = {
  getSettings(): Promise<AppSettings>;
  getLocalAccessKey(): Promise<string>;
  updateLocalAccessKey(value: string): Promise<AppSettings>;
  importRelayPoolToCCSwitch(): Promise<CcswitchImportResult>;
  updateSettings(input: UpdateSettingsInput): Promise<AppSettings>;
  chooseDataDir(): Promise<AppSettings>;
  resetDataDir(): Promise<AppSettings>;
};

export type StationsDomainClient = {
  listStations(): Promise<Station[]>;
  createStation(input: StationInput): Promise<Station>;
  updateStation(input: StationUpdateInput): Promise<Station>;
  deleteStation(id: string): Promise<void>;
  openStationWebsite(url: string): Promise<void>;
  reorderStations(stationIds: string[]): Promise<Station[]>;
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
  testStationKeyConnectivity(
    stationKeyId: string,
    model: string,
    options?: { onEvent?: (event: StationKeyConnectivityTestEvent) => void },
  ): Promise<StationKeyConnectivityTestResult>;
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

export type BackendClient = {
  readonly mode: BackendMode;
  readonly settings: SettingsDomainClient;
  readonly stations: StationsDomainClient;
  readonly stationKeys: StationKeysDomainClient;
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
