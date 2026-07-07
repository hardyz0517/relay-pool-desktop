export type BindingKind = "station_group" | "key_binding" | string;

export type BindingStatus =
  | "available"
  | "bound"
  | "missing"
  | "disabled"
  | "manual_legacy"
  | string;

export type StationGroupBinding = {
  id: string;
  stationId: string;
  stationKeyId: string | null;
  bindingKind: BindingKind;
  parentGroupBindingId: string | null;
  groupKeyHash: string;
  groupIdHash: string | null;
  groupName: string;
  bindingStatus: BindingStatus;
  defaultRateMultiplier: number | null;
  userRateMultiplier: number | null;
  effectiveRateMultiplier: number | null;
  rateSource: string | null;
  confidence: number;
  lastSeenAt: string | null;
  lastCheckedAt: string | null;
  lastRateChangedAt: string | null;
  rawJsonRedacted: Record<string, unknown> | null;
  createdAt: string;
  updatedAt: string;
};

export type GroupRateRecord = {
  id: string;
  stationId: string;
  stationKeyId: string | null;
  groupBindingId: string | null;
  bindingKind: BindingKind;
  groupKeyHash: string;
  groupName: string;
  defaultRateMultiplier: number | null;
  userRateMultiplier: number | null;
  effectiveRateMultiplier: number | null;
  source: string;
  confidence: number;
  rawJsonRedacted: Record<string, unknown> | null;
  checkedAt: string;
  createdAt: string;
};

export type UpsertStationGroupBindingInput = {
  stationId: string;
  stationKeyId: string | null;
  bindingKind: "station_group" | "key_binding";
  parentGroupBindingId: string | null;
  groupKeyHash: string;
  groupIdHash: string | null;
  groupName: string;
  bindingStatus: "available" | "bound" | "missing" | "disabled" | "manual_legacy";
  defaultRateMultiplier: number | null;
  userRateMultiplier: number | null;
  effectiveRateMultiplier: number | null;
  rateSource: string | null;
  confidence: number;
  lastSeenAt: string | null;
  rawJsonRedacted: Record<string, unknown> | null;
};

export type StationGroupOption = {
  value: string;
  groupBindingId: string | null;
  groupIdHash: string | null;
  groupName: string;
  rateMultiplier: number | null;
  rateSource: string | null;
  selectableForRemoteKey: boolean;
};

export function isCollectedStationGroupBinding(binding: StationGroupBinding) {
  return (
    binding.bindingKind === "station_group" &&
    binding.bindingStatus !== "disabled" &&
    binding.bindingStatus !== "missing" &&
    binding.bindingStatus !== "manual_legacy" &&
    binding.rateSource !== "legacy_key_group"
  );
}
