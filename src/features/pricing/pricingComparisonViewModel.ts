import {
  buildPricingGroupCandidates,
  type PricingGroupCandidate,
} from "../../lib/projections/pricingFacts";
import type { GroupRateRecord, StationGroupBinding } from "../../lib/types/groupFacts";
import type { PricingRule } from "../../lib/types/economics";
import type { StationKey } from "../../lib/types/stationKeys";
import type { Station } from "../../lib/types/stations";

export type PricingGroupType = "gpt" | "claude" | "gemini" | "grok" | "image_generation";

export type PricingComparisonFilters = {
  groupType?: PricingGroupType | "all";
  query?: string;
  stationId?: string | "all";
};

export type PricingComparisonInput = {
  stations: Station[];
  stationKeys?: StationKey[];
  groupBindings: StationGroupBinding[];
  groupRates: GroupRateRecord[];
  pricingRules: PricingRule[];
  filters?: PricingComparisonFilters;
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
  isCheapest: boolean;
};

export type PricingGroupSection = {
  groupType: PricingGroupType;
  title: string;
  rows: PricingComparisonRow[];
};

export type PricingComparisonMetrics = {
  coveredGroupTypeCount: number;
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

const groupTypeDefinitions: Array<{ groupType: PricingGroupType; title: string }> = [
  { groupType: "gpt", title: "GPT" },
  { groupType: "claude", title: "Claude" },
  { groupType: "gemini", title: "Gemini" },
  { groupType: "grok", title: "Grok" },
  { groupType: "image_generation", title: "生成图片" },
];

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
  const pricingCandidates = buildPricingGroupCandidates({
    stations: input.stations,
    stationKeys: input.stationKeys,
    groupBindings: input.groupBindings,
    groupRates: input.groupRates,
    pricingRules: input.pricingRules,
  });
  const rows = pricingCandidates
    .map(createRowFromCandidate)
    .filter((row): row is PricingComparisonRow => row !== null);

  if (rows.length === 0) {
    return {
      filters,
      sections: [],
      metrics: emptyMetrics(),
      emptyReason: "no_group_rates",
    };
  }

  const sections = groupTypeDefinitions
    .filter((definition) => filters.groupType === "all" || filters.groupType === definition.groupType)
    .map((definition) => {
      const sectionRows = markCheapestRows(
        rows
          .filter((row) => row.groupType === definition.groupType)
          .filter((row) => rowMatchesFilters(row, filters, definition.title))
          .sort(compareRows),
      );
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

function normalizeFilters(filters: PricingComparisonFilters | undefined): Required<PricingComparisonFilters> {
  return {
    groupType: filters?.groupType ?? "all",
    query: filters?.query ?? "",
    stationId: filters?.stationId ?? "all",
  };
}

function createRowFromCandidate(candidate: PricingGroupCandidate): PricingComparisonRow | null {
  const groupType = groupTypeFromCandidate(candidate);
  if (!groupType) {
    return null;
  }
  const creditPerCny = safeCreditPerCny(candidate.station.creditPerCny);
  const effectiveMultiplier =
    candidate.groupMultiplier === null ? null : candidate.groupMultiplier / creditPerCny;

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
    isCheapest: false,
  };
}

function groupTypeFromCandidate(candidate: PricingGroupCandidate): PricingGroupType | null {
  if (isImageGenerationGroupName(candidate.groupName)) {
    return "image_generation";
  }

  const platform = groupPlatformFromRawJson(candidate.groupRawJsonRedacted);
  if (platform) {
    const platformType = groupTypeFromPlatform(platform);
    if (platformType) {
      return platformType;
    }
  }

  const structuredGroupType = structuredGroupTypeForCandidate(candidate);
  if (structuredGroupType) {
    return structuredGroupType;
  }

  return groupTypeFromText(
    [candidate.groupIdHash ?? "", candidate.groupName, searchableJsonText(candidate.groupRawJsonRedacted)].join(" "),
  );
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

function groupTypeFromPlatform(platform: string): PricingGroupType | null {
  const normalized = normalizeText(platform);
  if (["openai", "gpt"].includes(normalized)) {
    return "gpt";
  }
  if (["anthropic", "claude"].includes(normalized)) {
    return "claude";
  }
  if (["google", "gemini"].includes(normalized)) {
    return "gemini";
  }
  if (["grok", "xai", "x-ai"].includes(normalized)) {
    return "grok";
  }
  return null;
}

function groupTypeFromText(value: string): PricingGroupType | null {
  if (textMatchesAnyMatcher(value, ["claude", "anthropic", "sonnet", "opus", "haiku", "yellow", "amber"])) {
    return "claude";
  }
  if (textMatchesAnyMatcher(value, ["gemini", "google"])) {
    return "gemini";
  }
  if (textMatchesAnyMatcher(value, ["grok", "xai", "x-ai"])) {
    return "grok";
  }
  if (textMatchesAnyMatcher(value, ["openai", "gpt", "codex", "default", "green"])) {
    return "gpt";
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
  const query = normalizeText(filters.query);
  if (query && !rowMatchesQuery(row, query, sectionTitle)) {
    return false;
  }
  return true;
}

function rowMatchesQuery(row: PricingComparisonRow, query: string, sectionTitle: string) {
  return [sectionTitle, row.stationName, row.stationKeyName ?? "", row.groupName]
    .map(normalizeText)
    .some((value) => value.includes(query));
}

function groupPlatformFromRawJson(value: Record<string, unknown> | null) {
  const platform = stringFieldFromRecord(value, [
    "platform",
    "provider",
    "model_provider",
    "modelProvider",
  ]);
  return platform?.trim().toLowerCase() ?? null;
}

function isImageGenerationGroupName(value: string) {
  return textMatchesAnyMatcher(value, imageGenerationGroupMatchers);
}

const imageGenerationGroupMatchers = ["图", "image", "images", "picture", "pictures", "dall-e", "midjourney"];

function textMatchesAnyMatcher(value: string, matchers: string[]) {
  const normalizedValue = normalizeText(value);
  return matchers.map(normalizeText).filter(Boolean).some((matcher) => normalizedValue.includes(matcher));
}

function normalizeText(value: string) {
  return value.trim().toLowerCase().replace(/[_\s]+/g, "-");
}

function searchableJsonText(value: Record<string, unknown> | null) {
  if (!value) {
    return "";
  }
  return collectJsonText(value).join(" ");
}

function collectJsonText(value: unknown): string[] {
  if (value === null || value === undefined) {
    return [];
  }
  if (typeof value === "string" || typeof value === "number" || typeof value === "boolean") {
    return [String(value)];
  }
  if (Array.isArray(value)) {
    return value.flatMap(collectJsonText);
  }
  if (typeof value === "object") {
    return Object.entries(value).flatMap(([key, nestedValue]) => [key, ...collectJsonText(nestedValue)]);
  }
  return [];
}

function stringFieldFromRecord(value: Record<string, unknown> | null, keys: string[]) {
  if (!value) {
    return null;
  }
  for (const key of keys) {
    const fieldValue = value[key];
    if (typeof fieldValue === "string" && fieldValue.trim()) {
      return fieldValue;
    }
  }
  return null;
}

function safeCreditPerCny(value: number) {
  return Number.isFinite(value) && value > 0 ? value : 1;
}

function markCheapestRows(rows: PricingComparisonRow[]) {
  const cheapestIndex = rows.findIndex((row) => row.effectiveMultiplier !== null);
  if (cheapestIndex < 0) {
    return rows;
  }
  return rows.map((row, index) => ({ ...row, isCheapest: index === cheapestIndex }));
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
    coveredGroupTypeCount: sections.filter((section) => section.rows.length > 0).length,
    comparableGroupCount: rows.filter((entry) => entry.row.effectiveMultiplier !== null).length,
    lowestEffectiveMultiplier: lowest?.row.effectiveMultiplier ?? null,
    lowestEffectiveMultiplierLabel: lowest
      ? `${lowest.sectionTitle} / ${lowest.row.stationName} / ${lowest.row.groupName}`
      : "",
  };
}

function emptyMetrics(): PricingComparisonMetrics {
  return {
    coveredGroupTypeCount: 0,
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
