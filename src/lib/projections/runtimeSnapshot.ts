import { buildCurrentStationBalanceFacts } from "@/lib/projections/balanceFacts";
import { buildCurrentStationGroupFacts, type StationGroupCurrentFact } from "@/lib/projections/groupFacts";
import { buildPricingGroupCandidates, type PricingGroupCandidate } from "@/lib/projections/pricingFacts";
import type { BalanceSnapshot, PricingRule } from "@/lib/types/economics";
import type { GroupRateRecord, StationGroupBinding } from "@/lib/types/groupFacts";
import type { StationKeyCapabilities, StationKeyHealth } from "@/lib/types/routing";
import type { KeyPoolItem, StationKey } from "@/lib/types/stationKeys";
import type { Station } from "@/lib/types/stations";

export type RuntimeRouteSecretRef = {
  kind: "station_key_secret";
  stationKeyId: string;
  present: boolean;
  masked: string | null;
};

export type RuntimeRouteSnapshotCandidate = {
  stationKeyId: string;
  stationId: string;
  stationName: string;
  keyName: string;
  enabled: boolean;
  priority: number;
  upstreamBaseUrl: string;
  upstreamApiFormat: string;
  secretRef: RuntimeRouteSecretRef;
  groupBindingId: string | null;
  groupIdentityKey: string | null;
  rateMultiplier: number | null;
  rateSource: string | null;
  modelPolicy: {
    allowlist: string[];
    blocklist: string[];
    preferredModels: string[];
    onlyUseAsBackup: boolean;
    routingTags: string[];
  };
  pricingStatus: {
    pricingRuleId: string | null;
    priceConfidence: number | null;
    source: string | null;
  };
  balanceStatus: {
    status: string | null;
    value: number | null;
    currency: string;
    scope: string | null;
    collectedAt: string | null;
  };
  healthStatus: {
    consecutiveFailures: number;
    successCount: number;
    failureCount: number;
    cooldownUntil: string | null;
    lastErrorSummary: string | null;
  };
  evidence: {
    groupFactIdentity: string | null;
    groupRateRecordId: string | null;
    balanceSnapshotId: string | null;
    capabilityUpdatedAt: string | null;
    healthUpdatedAt: string | null;
  };
};

export type RuntimeRouteSnapshot = {
  snapshotId: string;
  generatedAt: string;
  version: 1;
  candidates: RuntimeRouteSnapshotCandidate[];
};

type RuntimeStationKey = StationKey & Partial<Pick<KeyPoolItem, "stationUpstreamApiFormat">>;

export function buildRuntimeRouteSnapshot(input: {
  generatedAt: string;
  stations: Station[];
  stationKeys: RuntimeStationKey[];
  capabilities: StationKeyCapabilities[];
  health: StationKeyHealth[];
  groupBindings: StationGroupBinding[];
  groupRates: GroupRateRecord[];
  pricingRules: PricingRule[];
  balances: BalanceSnapshot[];
}): RuntimeRouteSnapshot {
  const stationsById = new Map(input.stations.map((station) => [station.id, station]));
  const capabilitiesByKeyId = new Map(input.capabilities.map((item) => [item.stationKeyId, item]));
  const healthByKeyId = new Map(input.health.map((item) => [item.stationKeyId, item]));
  const groupFactByBindingId = groupFactsByBindingId(input.groupBindings, input.groupRates);
  const pricingByGroupBindingId = pricingCandidatesByGroupBindingId(input);
  const balancesByStationId = buildCurrentStationBalanceFacts({
    stations: input.stations,
    balances: input.balances,
  });

  const candidates = input.stationKeys
    .flatMap((stationKey) => {
      const {
        id,
        stationId,
        name,
        apiKeyMasked,
        apiKeyPresent,
        enabled,
        priority,
        groupBindingId,
        stationUpstreamApiFormat,
      } = stationKey;
      const station = stationsById.get(stationId);
      if (!station || !station.enabled || !enabled || !apiKeyPresent) {
        return [];
      }

      const groupFact = groupBindingId ? groupFactByBindingId.get(groupBindingId) ?? null : null;
      const pricing = groupBindingId ? pricingByGroupBindingId.get(groupBindingId) ?? null : null;
      const balance = balancesByStationId.get(stationId) ?? null;
      const capability = capabilitiesByKeyId.get(id) ?? null;
      const health = healthByKeyId.get(id) ?? null;

      return [
        {
          stationKeyId: id,
          stationId,
          stationName: station.name,
          keyName: name,
          enabled,
          priority,
          upstreamBaseUrl: station.baseUrl,
          upstreamApiFormat: stationUpstreamApiFormat ?? "auto",
          secretRef: {
            kind: "station_key_secret" as const,
            stationKeyId: id,
            present: apiKeyPresent,
            masked: apiKeyMasked,
          },
          groupBindingId,
          groupIdentityKey: groupFact?.identityKey ?? null,
          rateMultiplier: groupFact?.rateMultiplier ?? null,
          rateSource: groupFact?.rateSource ?? null,
          modelPolicy: modelPolicyFor(capability),
          pricingStatus: pricingStatusFor(pricing),
          balanceStatus: {
            status: balance?.status ?? null,
            value: balance?.value ?? null,
            currency: balance?.currency ?? "CNY",
            scope: balance?.sourceSnapshot?.scope ?? balance?.source ?? null,
            collectedAt: balance?.collectedAt ?? null,
          },
          healthStatus: {
            consecutiveFailures: health?.consecutiveFailures ?? 0,
            successCount: health?.successCount ?? 0,
            failureCount: health?.failureCount ?? 0,
            cooldownUntil: health?.cooldownUntil ?? null,
            lastErrorSummary: health?.lastErrorSummary ?? null,
          },
          evidence: {
            groupFactIdentity: groupFact?.identityKey ?? null,
            groupRateRecordId: groupFact?.rateEvidenceId ?? null,
            balanceSnapshotId: balance?.snapshotId ?? null,
            capabilityUpdatedAt: capability?.updatedAt ?? null,
            healthUpdatedAt: health?.updatedAt ?? null,
          },
        },
      ];
    })
    .sort((left, right) => left.priority - right.priority || left.stationKeyId.localeCompare(right.stationKeyId));

  return {
    snapshotId: `runtime-route-${input.generatedAt}`,
    generatedAt: input.generatedAt,
    version: 1,
    candidates,
  };
}

function groupFactsByBindingId(
  bindings: StationGroupBinding[],
  rates: GroupRateRecord[],
) {
  const facts = buildCurrentStationGroupFacts({ bindings, rates });
  return new Map(
    facts.flatMap((fact) => {
      if (!fact.available || !fact.groupBindingId) {
        return [];
      }
      return [[fact.groupBindingId, fact] as const];
    }),
  );
}

function pricingCandidatesByGroupBindingId(input: {
  stations: Station[];
  stationKeys: RuntimeStationKey[];
  groupBindings: StationGroupBinding[];
  groupRates: GroupRateRecord[];
  pricingRules: PricingRule[];
}) {
  const candidates = buildPricingGroupCandidates({
    stations: input.stations,
    stationKeys: input.stationKeys,
    groupBindings: input.groupBindings,
    groupRates: input.groupRates,
    pricingRules: input.pricingRules,
  });
  return new Map(
    candidates.flatMap((candidate) => {
      if (!candidate.groupBindingId) {
        return [];
      }
      return [[candidate.groupBindingId, candidate] as const];
    }),
  );
}

function modelPolicyFor(capability: StationKeyCapabilities | null) {
  return {
    allowlist: capability?.modelAllowlist ?? [],
    blocklist: capability?.modelBlocklist ?? [],
    preferredModels: capability?.preferredModels ?? [],
    onlyUseAsBackup: capability?.onlyUseAsBackup ?? false,
    routingTags: capability?.routingTags ?? [],
  };
}

function pricingStatusFor(candidate: PricingGroupCandidate | null) {
  return {
    pricingRuleId: candidate?.pricingRuleId ?? null,
    priceConfidence: null,
    source: candidate?.source ?? null,
  };
}
