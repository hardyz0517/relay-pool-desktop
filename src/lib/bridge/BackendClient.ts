import type { RuntimeContractInfo } from "./contract";
import type { AppSettings, CcswitchImportResult, UpdateSettingsInput } from "@/lib/types/settings";
import type {
  EndpointPingResult,
  Station,
  StationEndpointHealth,
  StationInput,
  StationUpdateInput,
} from "@/lib/types/stations";

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

export type BackendClient = {
  readonly mode: BackendMode;
  readonly settings: SettingsDomainClient;
  readonly stations: StationsDomainClient;
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
