import { IPC_BINDING_HASH, IPC_CONTRACT_VERSION } from "./contract";
import { DemoBackendUnsupportedError, type BackendClient } from "./BackendClient";
import type { RuntimeContractInfo } from "./contract";

const DEMO_SEED = "relay-pool-demo-v1";
const DEMO_CLOCK_ISO = "2026-07-22T00:00:00.000Z";

type DemoStore = {
  readonly seed: string;
  readonly clockIso: string;
};

export class DemoBackend implements BackendClient {
  readonly mode = "demo" as const;
  readonly settings: BackendClient["settings"] = {
    getSettings: () => this.rejectUnsupported("settings"),
    getLocalAccessKey: () => this.rejectUnsupported("settings.local_access_key"),
    updateLocalAccessKey: (_value: string) => this.rejectUnsupported("settings.local_access_key"),
    importRelayPoolToCCSwitch: () => this.rejectUnsupported("settings.ccswitch_import"),
    updateSettings: () => this.rejectUnsupported("settings"),
    chooseDataDir: () => this.rejectUnsupported("settings.data_dir"),
    resetDataDir: () => this.rejectUnsupported("settings.data_dir"),
  };
  readonly stations: BackendClient["stations"] = {
    listStations: () => this.rejectUnsupported("stations"),
    createStation: () => this.rejectUnsupported("stations"),
    updateStation: () => this.rejectUnsupported("stations"),
    deleteStation: (_id: string) => this.rejectUnsupported("stations"),
    openStationWebsite: (_url: string) => this.rejectUnsupported("stations.external_url"),
    reorderStations: () => this.rejectUnsupported("stations"),
    listStationEndpointHealth: () => this.rejectUnsupported("stations.endpoint_health"),
    pingStationEndpoint: (_stationId: string) => this.rejectUnsupported("stations.endpoint_ping"),
  };
  readonly stationKeys: BackendClient["stationKeys"] = {
    listStationKeys: (_stationId: string) => this.rejectUnsupported("station_keys"),
    getRemoteKeyCapability: (_stationId: string) => this.rejectUnsupported("station_keys.remote_key"),
    listRemoteStationKeys: (_stationId: string) => this.rejectUnsupported("station_keys.remote_key"),
    scanRemoteStationKeys: (_stationId: string) => this.rejectUnsupported("station_keys.remote_key_scan"),
    createRemoteStationKey: () => this.rejectUnsupported("station_keys.remote_key"),
    createLocalStationKeyFromRemote: (_remoteKeyId: string, _stationId: string) =>
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
    testStationKeyConnectivity: (_stationKeyId: string, _model: string) =>
      this.rejectUnsupported("station_keys.connectivity"),
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
