import type { GroupRateRecord, StationGroupBinding, StationGroupOption } from "@/lib/types/groupFacts";

export type StationGroupCurrentFact = {
  identityKey: string;
  groupBindingId: string | null;
  stationId: string;
  stationKeyId: string | null;
  bindingKind: string;
  groupKeyHash: string | null;
  groupIdHash: string | null;
  groupName: string;
  bindingStatus: string;
  available: boolean;
  rateMultiplier: number | null;
  rateSource: string | null;
  rateEvidenceId: string | null;
  rateCheckedAt: string | null;
  sourceBinding: StationGroupBinding | null;
  sourceRate: GroupRateRecord | null;
};

export function buildCurrentStationGroupFacts(input: {
  bindings: StationGroupBinding[];
  rates: GroupRateRecord[];
}): StationGroupCurrentFact[] {
  const latestRates = latestGroupRatesByBindingOrHash(input.rates);
  const bindingIndex = buildBindingIndex(input.bindings);
  const consumedRateIds = new Set<string>();
  const facts = input.bindings.map((binding) => {
    const identityKey = identityKeyForBinding(binding);
    const latestRate =
      latestRates.get(`binding:${binding.id}`) ??
      latestRates.get(`group-key:${binding.groupKeyHash}`) ??
      null;
    if (latestRate) {
      consumedRateIds.add(latestRate.id);
    }
    return factFromBinding(binding, latestRate, identityKey);
  });

  for (const rate of input.rates) {
    if (consumedRateIds.has(rate.id) || rateIsCoveredByBinding(rate, bindingIndex)) {
      continue;
    }
    const identityKey = identityKeyForRate(rate);
    if (facts.some((fact) => fact.identityKey === identityKey)) {
      continue;
    }
    facts.push(factFromRate(rate, identityKey));
  }

  return facts;
}

function buildBindingIndex(bindings: StationGroupBinding[]) {
  return {
    ids: new Set(bindings.map((binding) => binding.id).filter(Boolean)),
    groupKeyHashes: new Set(bindings.map((binding) => binding.groupKeyHash).filter(Boolean)),
    groupNames: new Set(bindings.map((binding) => normalizedName(binding.groupName)).filter(Boolean)),
  };
}

function rateIsCoveredByBinding(
  rate: GroupRateRecord,
  bindingIndex: ReturnType<typeof buildBindingIndex>,
) {
  if (rate.groupBindingId && bindingIndex.ids.has(rate.groupBindingId)) {
    return true;
  }
  if (rate.groupKeyHash && bindingIndex.groupKeyHashes.has(rate.groupKeyHash)) {
    return true;
  }
  return !rate.groupBindingId && !rate.groupKeyHash && bindingIndex.groupNames.has(normalizedName(rate.groupName));
}

export function latestGroupRatesByBindingOrHash(
  rates: GroupRateRecord[],
): Map<string, GroupRateRecord> {
  const latest = new Map<string, GroupRateRecord>();
  for (const rate of rates) {
    const normalizedGroupName = normalizedName(rate.groupName);
    const keys = [
      rate.groupBindingId ? `binding:${rate.groupBindingId}` : null,
      rate.groupKeyHash ? `group-key:${rate.groupKeyHash}` : null,
      normalizedGroupName ? `group-name:${normalizedGroupName}` : null,
    ].filter((key): key is string => Boolean(key));
    for (const key of keys) {
      const existing = latest.get(key);
      if (!existing || Date.parse(rate.checkedAt) >= Date.parse(existing.checkedAt)) {
        latest.set(key, rate);
      }
    }
  }
  return latest;
}

export function buildStationGroupOptionsFromCurrentFacts(
  facts: StationGroupCurrentFact[],
): StationGroupOption[] {
  return facts
    .filter((fact) => fact.available)
    .map((fact) => ({
      value: fact.groupBindingId ? `binding:${fact.groupBindingId}` : fact.identityKey,
      groupBindingId: fact.groupBindingId,
      groupIdHash: fact.groupIdHash,
      groupName: fact.groupName,
      rateMultiplier: fact.rateMultiplier,
      rateSource: fact.rateSource,
      selectableForRemoteKey: Boolean(fact.groupIdHash),
    }));
}

export function isDisplayableStationGroupCurrentFact(fact: StationGroupCurrentFact) {
  return (
    fact.bindingKind === "station_group" &&
    fact.available &&
    fact.bindingStatus !== "manual_legacy" &&
    fact.sourceBinding?.rateSource !== "legacy_key_group"
  );
}

function factFromBinding(
  binding: StationGroupBinding,
  latestRate: GroupRateRecord | null,
  identityKey: string,
): StationGroupCurrentFact {
  return {
    identityKey,
    groupBindingId: binding.id,
    stationId: binding.stationId,
    stationKeyId: binding.stationKeyId,
    bindingKind: binding.bindingKind,
    groupKeyHash: binding.groupKeyHash,
    groupIdHash: binding.groupIdHash,
    groupName: binding.groupName,
    bindingStatus: binding.bindingStatus,
    available: binding.bindingStatus !== "missing" && binding.bindingStatus !== "disabled",
    rateMultiplier: firstNumber(
      binding.userRateMultiplier,
      binding.effectiveRateMultiplier,
      latestRate?.userRateMultiplier,
      latestRate?.effectiveRateMultiplier,
      binding.defaultRateMultiplier,
      latestRate?.defaultRateMultiplier,
    ),
    rateSource: binding.rateSource ?? latestRate?.source ?? null,
    rateEvidenceId: latestRate?.id ?? null,
    rateCheckedAt: latestRate?.checkedAt ?? binding.lastCheckedAt,
    sourceBinding: binding,
    sourceRate: latestRate,
  };
}

function factFromRate(rate: GroupRateRecord, identityKey: string): StationGroupCurrentFact {
  return {
    identityKey,
    groupBindingId: rate.groupBindingId,
    stationId: rate.stationId,
    stationKeyId: rate.stationKeyId,
    bindingKind: rate.bindingKind,
    groupKeyHash: rate.groupKeyHash,
    groupIdHash: null,
    groupName: rate.groupName,
    bindingStatus: "rate_only",
    available: true,
    rateMultiplier: firstNumber(
      rate.userRateMultiplier,
      rate.effectiveRateMultiplier,
      rate.defaultRateMultiplier,
    ),
    rateSource: rate.source,
    rateEvidenceId: rate.id,
    rateCheckedAt: rate.checkedAt,
    sourceBinding: null,
    sourceRate: rate,
  };
}

function identityKeyForBinding(binding: StationGroupBinding) {
  if (binding.id) return `binding:${binding.id}`;
  if (binding.groupKeyHash) return `group-key:${binding.groupKeyHash}`;
  if (binding.groupIdHash) return `group-id:${binding.groupIdHash}`;
  return `group-name:${normalizedName(binding.groupName)}`;
}

function identityKeyForRate(rate: GroupRateRecord) {
  if (rate.groupBindingId) return `binding:${rate.groupBindingId}`;
  if (rate.groupKeyHash) return `group-key:${rate.groupKeyHash}`;
  return `group-name:${normalizedName(rate.groupName)}`;
}

function firstNumber(...values: Array<number | null | undefined>) {
  for (const value of values) {
    if (typeof value === "number" && Number.isFinite(value)) {
      return value;
    }
  }
  return null;
}

function normalizedName(value: string) {
  return value.trim().toLowerCase();
}
