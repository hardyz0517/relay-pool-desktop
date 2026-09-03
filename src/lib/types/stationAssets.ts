import type {
  ReadModelEnvelope,
  ReadModelRevisionDto,
  StationAssetReadRowDto,
  StationAssetsReadModelDto,
  StationDetailIncidentDto,
  StationDetailReadModelDto,
} from "@/lib/bridge/generated";
import type { CollectorSnapshot } from "./collector";
import type { CollectorRun } from "./collectorRuns";
import type { BalanceSnapshot } from "./economics";
import type { GroupRateRecord, StationGroupBinding } from "./groupFacts";
import type { KeyPoolItem, StationCredentials } from "./stationKeys";
import type { Station } from "./stations";
import type { AlertSeverity, AlertingIncident } from "./alerting";

/**
 * Backend-owned Station asset read model. The envelope/revision fields are
 * intentionally kept here even before the Tauri command is exposed so the
 * frontend cannot regress to an unversioned array contract when the binding
 * lands.
 */
export type StationAssetReadRow = Omit<StationAssetReadRowDto, "station" | "keys"> & {
  station: Station;
  keys: KeyPoolItem[];
};

export type StationAssetsReadModel = Omit<StationAssetsReadModelDto, "rows"> & {
  rows: StationAssetReadRow[];
};

export type StationAssetsReadModelEnvelope = Omit<
  ReadModelEnvelope<StationAssetsReadModelDto>,
  "data"
> & {
  data: StationAssetsReadModel;
};

export type StationAssetsRevision = ReadModelRevisionDto;

export type StationDetailIncident = Omit<StationDetailIncidentDto, "severity" | "lifecycleState" | "stationId"> & {
  severity: AlertSeverity;
  lifecycleState: AlertingIncident["lifecycleState"];
  stationId: string | null;
};

export type StationDetailReadModel = Omit<
  StationDetailReadModelDto,
  | "asset"
  | "credentials"
  | "groupBindings"
  | "groupRates"
  | "collectorRuns"
  | "latestSnapshot"
  | "balances"
  | "incidents"
> & {
  asset: StationAssetReadRow;
  credentials: StationCredentials;
  groupBindings: StationGroupBinding[];
  groupRates: GroupRateRecord[];
  collectorRuns: CollectorRun[];
  latestSnapshot: CollectorSnapshot | null;
  balances: BalanceSnapshot[];
  incidents: StationDetailIncident[];
};

export type StationDetailReadModelEnvelope = Omit<
  ReadModelEnvelope<StationDetailReadModelDto>,
  "data"
> & {
  data: StationDetailReadModel;
};
