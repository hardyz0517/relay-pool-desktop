import {
  derivePricingGroupDisplayCandidates,
  type PricingGroupCandidate,
} from "../../lib/projections/pricingFacts";
import { groupCategoryDefinitions, type StationGroupCategory } from "../../lib/groupCategories";
import type { GroupRateRecord, StationGroupBinding } from "../../lib/types/groupFacts";
import type { PricingRule } from "../../lib/types/economics";
import type { StationKey } from "../../lib/types/stationKeys";
import type { Station } from "../../lib/types/stations";
import type {
  PricingGroupMonitorDisplayState,
  PricingGroupMonitorStatusWorkspace,
  PricingGroupMonitorSummary,
} from "@/lib/types/pricingMonitoring";
import type { PricingGroupRefInput } from "@/lib/projections/pricingGroupRefs";
import { normalizePricingGroupDisplayRefs } from "@/lib/projections/pricingGroupRefs";
import { effectiveRateMultiplierForCredit } from "@/lib/formatters";

export type PricingGroupType = StationGroupCategory;

export type PricingComparisonFilters = {
  groupType?: PricingGroupType | "all";
  query?: string;
  stationId?: string | "all";
  keyPresence?: "all" | "with_key" | "with_credentialed_key";
  monitorPresence?: "all" | "monitored" | "unmonitored";
  monitorOutcome?:
    | "all"
    | "success"
    | "degraded"
    | "failure"
    | "skipped"
    | "running"
    | "untested"
    | "unavailable_data"
    | "unresolved";
};

export type PricingComparisonInput = {
  stations: Station[];
  stationKeys?: StationKey[];
  groupBindings: StationGroupBinding[];
  groupRates: GroupRateRecord[];
  pricingRules: PricingRule[];
  developerModeEnabled?: boolean;
  filters?: PricingComparisonFilters;
  monitorWorkspace?: PricingGroupMonitorStatusWorkspace | null;
  monitorDataState?: "loading" | "ready" | "error";
};

export type PricingComparisonRow = {
  id: string;
  groupType: PricingGroupType;
  stationId: string;
  stationName: string;
  stationKeyId: string | null;
  stationKeyName: string | null;
  groupBindingId: string | null;
  groupRateRecordId: string | null;
  groupName: string;
  groupRawJsonRedacted: Record<string, unknown> | null;
  groupMultiplier: number | null;
  creditPerCny: number;
  effectiveMultiplier: number | null;
  source: string;
  checkedAt: string | null;
  monitorSummary: PricingGroupMonitorSummary | null;
  monitorDisplayState: PricingGroupMonitorDisplayState;
  monitorRef: PricingGroupRefInput;
  hasBoundKey: boolean;
  hasCredentialedKey: boolean;
};

export type PricingGroupSection = {
  groupType: PricingGroupType;
  title: string;
  rows: PricingComparisonRow[];
};

export type PricingComparisonMetrics = {
  comparableGroupCount: number;
  lowestEffectiveMultiplier: number | null;
  lowestEffectiveMultiplierLabel: string;
};

export type PricingComparisonViewModel = {
  filters: Required<PricingComparisonFilters>;
  sections: PricingGroupSection[];
  metrics: PricingComparisonMetrics;
  emptyReason: "no_group_rates" | "filtered_empty" | null;
};

const groupTypeDefinitions: Array<{ groupType: PricingGroupType; title: string }> =
  groupCategoryDefinitions.map((definition) => ({
    groupType: definition.value,
    title: definition.label,
  }));

const structuredProviderGroupTypesByStation: Record<
  string,
  Partial<Record<Exclude<PricingGroupType, "image_generation" | "grok">, string[]>>
> = {
  "station-1783311325734-4639": {
    gpt: ["3", "23"],
    claude: ["13", "17"],
  },
  "station-1783351745197-26": {
    gpt: ["2", "24", "59", "62", "75"],
    claude: ["22", "57", "61"],
    gemini: ["7"],
  },
  "station-1783237821989-3": {
    gpt: ["23", "25", "26", "27", "28", "29", "30", "32", "33", "34", "36"],
  },
  "station-1783042263655-1": {
    gpt: ["2", "4", "5", "12", "13"],
    claude: ["7", "8", "11", "17"],
  },
  "station-1783351851692-74": {
    gpt: ["2", "7", "9", "10", "15"],
    claude: ["4", "16", "17"],
  },
  "station-1782477763399": {
    gpt: ["8"],
    claude: ["15"],
  },
};

export function buildPricingComparisonViewModel(
  input: PricingComparisonInput,
): PricingComparisonViewModel {
  const filters = normalizeFilters(input.filters);
  const monitorByRef = new Map(
    (input.monitorWorkspace?.items ?? []).map((item) => [monitorRefKey(item), item]),
  );
  const stationKeysById = new Map((input.stationKeys ?? []).map((key) => [key.id, key]));
  const pricingCandidates = derivePricingGroupDisplayCandidates({
    stations: input.stations,
    stationKeys: input.stationKeys,
    groupBindings: input.groupBindings,
    groupRates: input.groupRates,
    pricingRules: input.pricingRules,
  });
  const rows = pricingCandidates
    .map((candidate) =>
      createRowFromCandidate(
        candidate,
        monitorByRef,
        stationKeysById,
        input.monitorDataState ?? "ready",
      ),
    )
    .filter((row): row is PricingComparisonRow => row !== null);

  if (rows.length === 0) {
    return {
      filters,
      sections: [],
      metrics: emptyMetrics(),
      emptyReason: "no_group_rates",
    };
  }

  const sections = visibleGroupTypeDefinitions(input.developerModeEnabled === true)
    .filter((definition) => filters.groupType === "all" || filters.groupType === definition.groupType)
    .map((definition) => {
      const sectionRows = rows
        .filter((row) => row.groupType === definition.groupType)
        .filter((row) => rowMatchesFilters(row, filters, definition.title))
        .sort(compareRows);
      return { ...definition, rows: sectionRows };
    })
    .filter((section) => section.rows.length > 0);

  if (sections.length === 0) {
    return {
      filters,
      sections: [],
      metrics: emptyMetrics(),
      emptyReason: "filtered_empty",
    };
  }

  return {
    filters,
    sections,
    metrics: buildMetrics(sections),
    emptyReason: null,
  };
}

function visibleGroupTypeDefinitions(developerModeEnabled: boolean) {
  return groupTypeDefinitions.filter(
    (definition) =>
      developerModeEnabled || (definition.groupType !== "embedding" && definition.groupType !== "rerank"),
  );
}

function normalizeFilters(filters: PricingComparisonFilters | undefined): Required<PricingComparisonFilters> {
  return {
    groupType: filters?.groupType ?? "all",
    query: filters?.query ?? "",
    stationId: filters?.stationId ?? "all",
    keyPresence: filters?.keyPresence ?? "all",
    monitorPresence: filters?.monitorPresence ?? "all",
    monitorOutcome: filters?.monitorOutcome ?? "all",
  };
}

export function buildPricingMonitorRefs(input: Omit<PricingComparisonInput, "filters" | "monitorWorkspace" | "monitorDataState">): PricingGroupRefInput[] {
  const candidates = derivePricingGroupDisplayCandidates({
    stations: input.stations,
    stationKeys: input.stationKeys,
    groupBindings: input.groupBindings,
    groupRates: input.groupRates,
    pricingRules: input.pricingRules,
  });
  const refs = candidates.map((candidate) => ({
    stationId: candidate.station.id,
    groupBindingId: candidate.groupBindingId,
    groupIdHash: candidate.groupIdHash,
    groupKeyHash: candidate.groupKeyHash,
  }));
  try {
    return normalizePricingGroupDisplayRefs(refs).map((group) => ({
      stationId: group.stationId,
      groupBindingId: group.groupBindingId,
      groupIdHash: group.groupIdHash,
      groupKeyHash: group.groupKeyHash,
    }));
  } catch {
    return [];
  }
}

function createRowFromCandidate(
  candidate: PricingGroupCandidate,
  monitorByRef: Map<string, PricingGroupMonitorSummary>,
  stationKeysById: Map<string, StationKey>,
  monitorDataState: "loading" | "ready" | "error",
): PricingComparisonRow | null {
  const groupType = groupTypeFromCandidate(candidate);
  if (!groupType) {
    return null;
  }
  const creditPerCny = candidate.station.creditPerCny;
  const effectiveMultiplier = effectiveRateMultiplierForCredit(
    candidate.groupMultiplier,
    creditPerCny,
  );

  const monitorRef = {
    stationId: candidate.station.id,
    groupBindingId: candidate.groupBindingId,
    groupIdHash: candidate.groupIdHash,
    groupKeyHash: candidate.groupKeyHash,
  };
  const monitorSummary = monitorByRef.get(monitorRefKey(monitorRef)) ?? null;
  const fallbackKey = candidate.stationKeyId ? stationKeysById.get(candidate.stationKeyId) : null;
  const hasBoundKey = monitorSummary?.hasBoundKey ?? Boolean(fallbackKey);
  const hasCredentialedKey =
    (monitorSummary?.credentialedKeyCount ?? 0) > 0 || Boolean(fallbackKey?.apiKeyPresent);
  const monitorDisplayState = monitorSummary?.displayState
    ?? (monitorDataState === "loading" || monitorDataState === "error"
      ? "unavailable_data"
      : candidate.groupKeyHash.trim()
        ? "unresolved"
        : "unresolved");

  return {
    id: [groupType, candidate.identityKey].join(":"),
    groupType,
    stationId: candidate.station.id,
    stationName: candidate.station.name,
    stationKeyId: candidate.stationKeyId,
    stationKeyName: candidate.stationKeyName,
    groupBindingId: candidate.groupBindingId,
    groupRateRecordId: candidate.groupRateRecordId,
    groupName: candidate.groupName,
    groupRawJsonRedacted: candidate.groupRawJsonRedacted,
    groupMultiplier: candidate.groupMultiplier,
    creditPerCny,
    effectiveMultiplier,
    source: candidate.source,
    checkedAt: candidate.checkedAt,
    monitorSummary,
    monitorDisplayState,
    monitorRef,
    hasBoundKey,
    hasCredentialedKey,
  };
}

function groupTypeFromCandidate(candidate: PricingGroupCandidate): PricingGroupType {
  if (candidate.currentFact.effectiveGroupCategory !== "unknown") {
    return candidate.currentFact.effectiveGroupCategory;
  }
  return structuredGroupTypeForCandidate(candidate) ?? "unknown";
}

function structuredGroupTypeForCandidate(candidate: PricingGroupCandidate): PricingGroupType | null {
  const groupIdHash = candidate.groupIdHash?.trim();
  if (!groupIdHash) {
    return null;
  }
  const stationTypes = structuredProviderGroupTypesByStation[candidate.station.id];
  if (!stationTypes) {
    return null;
  }
  for (const definition of groupTypeDefinitions) {
    if (definition.groupType === "image_generation" || definition.groupType === "grok") {
      continue;
    }
    if (stationTypes[definition.groupType]?.includes(groupIdHash)) {
      return definition.groupType;
    }
  }
  return null;
}

function rowMatchesFilters(
  row: PricingComparisonRow,
  filters: Required<PricingComparisonFilters>,
  sectionTitle: string,
) {
  if (filters.stationId !== "all" && row.stationId !== filters.stationId) {
    return false;
  }
  if (filters.keyPresence === "with_key" && !row.hasBoundKey) {
    return false;
  }
  if (
    filters.keyPresence === "with_credentialed_key" &&
    !row.hasCredentialedKey
  ) {
    return false;
  }
  if (filters.monitorPresence !== "all") {
    if (!row.monitorSummary) {
      return false;
    }
    const monitored = row.monitorSummary.enabledMonitorDefinitionCount > 0;
    if (filters.monitorPresence === "monitored" && !monitored) {
      return false;
    }
    if (filters.monitorPresence === "unmonitored" && monitored) {
      return false;
    }
  }
  if (filters.monitorOutcome !== "all" && !matchesMonitorOutcome(row, filters.monitorOutcome)) {
    return false;
  }
  const query = normalizeText(filters.query);
  if (query && !rowMatchesQuery(row, query, sectionTitle)) {
    return false;
  }
  return true;
}

function matchesMonitorOutcome(
  row: PricingComparisonRow,
  outcome: Exclude<Required<PricingComparisonFilters>["monitorOutcome"], "all">,
) {
  switch (outcome) {
    case "success":
      return row.monitorDisplayState === "available";
    case "degraded":
      return row.monitorDisplayState === "degraded";
    case "failure":
      return row.monitorDisplayState === "unavailable";
    case "skipped":
      return row.monitorDisplayState === "skipped";
    case "running":
      return row.monitorDisplayState === "running";
    case "untested":
      return row.monitorDisplayState === "untested";
    case "unavailable_data":
      return row.monitorDisplayState === "unavailable_data";
    case "unresolved":
      return row.monitorDisplayState === "unresolved";
  }
}

function monitorRefKey(value: Pick<PricingGroupRefInput, "stationId" | "groupBindingId" | "groupIdHash" | "groupKeyHash">) {
  return [
    value.stationId.trim(),
    value.groupBindingId?.trim() ?? "",
    value.groupIdHash?.trim() ?? "",
    value.groupKeyHash.trim(),
  ].join("|");
}

export function pricingMonitorRefFromRow(row: PricingComparisonRow): PricingGroupRefInput {
  return row.monitorRef;
}

function rowMatchesQuery(row: PricingComparisonRow, query: string, sectionTitle: string) {
  return [sectionTitle, row.stationName, row.stationKeyName ?? "", row.groupName]
    .map(normalizeText)
    .some((value) => value.includes(query));
}

function normalizeText(value: string) {
  return value.trim().toLowerCase().replace(/[_\s]+/g, "-");
}

function buildMetrics(sections: PricingGroupSection[]): PricingComparisonMetrics {
  const rows = sections.flatMap((section) =>
    section.rows.map((row) => ({ row, sectionTitle: section.title })),
  );
  const lowest = rows
    .filter((entry): entry is { row: PricingComparisonRow & { effectiveMultiplier: number }; sectionTitle: string } =>
      entry.row.effectiveMultiplier !== null,
    )
    .sort(
      (left, right) =>
        left.row.effectiveMultiplier - right.row.effectiveMultiplier ||
        compareText(left.sectionTitle, right.sectionTitle) ||
        compareText(left.row.stationName, right.row.stationName) ||
        compareText(left.row.groupName, right.row.groupName),
    )[0];

  return {
    comparableGroupCount: rows.filter((entry) => entry.row.effectiveMultiplier !== null).length,
    lowestEffectiveMultiplier: lowest?.row.effectiveMultiplier ?? null,
    lowestEffectiveMultiplierLabel: lowest
      ? `${lowest.sectionTitle} / ${lowest.row.stationName} / ${lowest.row.groupName}`
      : "",
  };
}

function emptyMetrics(): PricingComparisonMetrics {
  return {
    comparableGroupCount: 0,
    lowestEffectiveMultiplier: null,
    lowestEffectiveMultiplierLabel: "",
  };
}

function compareRows(left: PricingComparisonRow, right: PricingComparisonRow) {
  return (
    compareNullableNumbers(left.effectiveMultiplier, right.effectiveMultiplier) ||
    compareText(left.stationName, right.stationName) ||
    compareText(left.groupName, right.groupName) ||
    compareText(left.id, right.id)
  );
}

function compareNullableNumbers(left: number | null, right: number | null) {
  if (left === null && right === null) {
    return 0;
  }
  if (left === null) {
    return 1;
  }
  if (right === null) {
    return -1;
  }
  return left - right;
}

function compareText(left: string, right: string) {
  return left.localeCompare(right, "en", { sensitivity: "base" });
}
