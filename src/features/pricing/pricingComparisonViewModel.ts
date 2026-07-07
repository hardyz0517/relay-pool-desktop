import type {
  OfficialModelCatalogEntry,
  OfficialModelProvider,
} from "./officialModelCatalog";
import type { GroupRateRecord, StationGroupBinding } from "../../lib/types/groupFacts";
import type { PricingRule } from "../../lib/types/economics";
import type { StationKey } from "../../lib/types/stationKeys";
import type { Station } from "../../lib/types/stations";

export type PricingEvidenceStatus = "discovered" | "unverified" | "unavailable";

export type PricingModelEvidence = {
  stationId: string;
  modelId: string;
  status: PricingEvidenceStatus;
};

export type PricingComparisonFilters = {
  provider?: OfficialModelProvider | "all";
  modelQuery?: string;
  stationId?: string | "all";
  verifiedOnly?: boolean;
};

type PricingComparisonCatalogEntry = Omit<
  OfficialModelCatalogEntry,
  "priceSourceUrl" | "priceSourceLabel"
> &
  Partial<Pick<OfficialModelCatalogEntry, "priceSourceUrl" | "priceSourceLabel">>;

export type PricingComparisonInput = {
  models: PricingComparisonCatalogEntry[];
  stations: Station[];
  stationKeys?: StationKey[];
  groupBindings: StationGroupBinding[];
  groupRates: GroupRateRecord[];
  pricingRules: PricingRule[];
  modelEvidence?: PricingModelEvidence[];
  filters?: PricingComparisonFilters;
};

export type PricingComparisonRow = {
  id: string;
  provider: OfficialModelProvider;
  modelId: string;
  displayName: string;
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
  officialInputPrice: number;
  officialOutputPrice: number;
  estimatedInputCny: number | null;
  estimatedOutputCny: number | null;
  evidenceStatus: PricingEvidenceStatus;
  evidenceLabel: string;
  source: string;
  checkedAt: string | null;
  isCheapest: boolean;
};

export type PricingModelSection = {
  provider: OfficialModelProvider;
  modelId: string;
  displayName: string;
  officialInputPrice: number;
  officialOutputPrice: number;
  priceSourceUrl: string;
  priceSourceLabel: string;
  aliases: string[];
  rows: PricingComparisonRow[];
};

export type PricingComparisonMetrics = {
  coveredModelCount: number;
  comparableGroupCount: number;
  lowestEffectiveMultiplier: number | null;
  lowestEffectiveMultiplierLabel: string;
};

export type PricingComparisonViewModel = {
  filters: Required<PricingComparisonFilters>;
  sections: PricingModelSection[];
  metrics: PricingComparisonMetrics;
  emptyReason: "no_catalog_models" | "no_group_rates" | "filtered_empty" | null;
};

type GroupCandidate = {
  station: Station;
  stationKeyId: string | null;
  stationKeyName: string | null;
  groupBindingId: string | null;
  groupRateRecordId: string | null;
  groupKeyHash: string;
  groupName: string;
  groupRawJsonRedacted: Record<string, unknown> | null;
  groupMultiplier: number | null;
  source: string;
  checkedAt: string | null;
};

const evidenceLabels: Record<PricingEvidenceStatus, string> = {
  discovered: "已发现",
  unverified: "未验证",
  unavailable: "不可用",
};

export function buildPricingComparisonViewModel(
  input: PricingComparisonInput,
): PricingComparisonViewModel {
  void input.pricingRules;

  const filters = normalizeFilters(input.filters);
  const stationsById = new Map(input.stations.map((station) => [station.id, station]));
  const stationKeyNameById = new Map((input.stationKeys ?? []).map((key) => [key.id, key.name]));
  const evidenceByStationModel = buildEvidenceIndex(input.modelEvidence ?? []);
  const enabledModels = input.models
    .filter((model) => model.enabledByDefault)
    .map((model, index) => ({ model, index }))
    .sort((left, right) => compareModels(left.model, right.model) || left.index - right.index);

  if (enabledModels.length === 0) {
    return {
      filters,
      sections: [],
      metrics: emptyMetrics(),
      emptyReason: "no_catalog_models",
    };
  }

  const visibleModels = enabledModels.filter(({ model }) => modelMatchesProviderFilter(model, filters));
  if (visibleModels.length === 0) {
    return {
      filters,
      sections: [],
      metrics: emptyMetrics(),
      emptyReason: "filtered_empty",
    };
  }

  const baseSections = visibleModels.map(({ model }) => {
    const modelQueryMatches = modelMatchesQuery(model, filters.modelQuery);
    const unfilteredRows = buildRowsForModel(
      model,
      input.groupBindings,
      input.groupRates,
      stationsById,
      stationKeyNameById,
      evidenceByStationModel,
    );
    const rows = markCheapestRows(
      unfilteredRows
        .filter((row) => rowMatchesFilters(row, filters, modelQueryMatches))
        .sort(compareRows),
    );

    return {
      provider: model.provider,
      modelId: model.modelId,
      displayName: model.displayName,
      officialInputPrice: model.officialInputPrice,
      officialOutputPrice: model.officialOutputPrice,
      priceSourceUrl: model.priceSourceUrl ?? "",
      priceSourceLabel: model.priceSourceLabel ?? "官方价格",
      aliases: model.aliases,
      rows,
      unfilteredRowCount: unfilteredRows.length,
      modelQueryMatches,
    };
  });

  const sections: PricingModelSection[] = baseSections
    .filter((section) => {
      const hasQuery = Boolean(normalizeText(filters.modelQuery));
      return !hasQuery || section.modelQueryMatches || section.rows.length > 0;
    })
    .map(({ unfilteredRowCount, modelQueryMatches, ...section }) => {
      void unfilteredRowCount;
      void modelQueryMatches;
      return section;
    });
  const hasComparableGroups = baseSections.some((section) => section.unfilteredRowCount > 0);
  const hasVisibleRows = sections.some((section) => section.rows.length > 0);

  if (!hasComparableGroups) {
    return {
      filters,
      sections,
      metrics: buildMetrics(sections),
      emptyReason: "no_group_rates",
    };
  }

  if (!hasVisibleRows) {
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
    provider: filters?.provider ?? "all",
    modelQuery: filters?.modelQuery ?? "",
    stationId: filters?.stationId ?? "all",
    verifiedOnly: filters?.verifiedOnly ?? false,
  };
}

function buildEvidenceIndex(modelEvidence: PricingModelEvidence[]) {
  return new Map(
    modelEvidence.map((evidence) => [`${evidence.stationId}\u0000${evidence.modelId}`, evidence.status]),
  );
}

function buildRowsForModel(
  model: PricingComparisonCatalogEntry,
  groupBindings: StationGroupBinding[],
  groupRates: GroupRateRecord[],
  stationsById: Map<string, Station>,
  stationKeyNameById: Map<string, string>,
  evidenceByStationModel: Map<string, PricingEvidenceStatus>,
) {
  const rows: PricingComparisonRow[] = [];
  const consumedRateIds = new Set<string>();
  const activeGroupBindingIds = new Set(groupBindings.filter(isStationGroupBinding).map((binding) => binding.id));
  const activeGroupKeys = new Set(
    groupBindings.filter(isStationGroupBinding).map((binding) => binding.groupKeyHash),
  );
  const inactiveGroupKeys = new Set(
    groupBindings
      .filter((binding) => binding.bindingKind === "station_group" && !isStationGroupBinding(binding))
      .map((binding) => binding.groupKeyHash),
  );

  for (const binding of groupBindings) {
    const station = stationsById.get(binding.stationId);
    if (!station || !isStationGroupBinding(binding)) {
      continue;
    }

    const relatedRates = groupRates
      .filter((rate) => isRateForBinding(rate, binding))
      .sort(compareRatesByFreshness);
    const matchingRate = relatedRates.find((rate) => groupRateMatchesModel(rate, model));
    const bindingMatches = groupBindingMatchesModel(binding, model);
    if (!bindingMatches && !matchingRate) {
      continue;
    }

    const rate = matchingRate ?? relatedRates[0] ?? null;
    for (const relatedRate of relatedRates) {
      consumedRateIds.add(relatedRate.id);
    }
    rows.push(
      createRowFromCandidate(
        model,
        bindingCandidate(binding, station, rate, stationKeyNameById),
        evidenceByStationModel,
      ),
    );
  }

  const latestStandaloneRates = latestRatesByStationGroup(
    groupRates.filter(
      (rate) =>
        !consumedRateIds.has(rate.id) &&
        isRateBackedByActiveGroup(rate, activeGroupBindingIds, activeGroupKeys, inactiveGroupKeys),
    ),
  );

  for (const rate of latestStandaloneRates) {
    if (consumedRateIds.has(rate.id)) {
      continue;
    }
    const station = stationsById.get(rate.stationId);
    if (!station || !isStationGroupRate(rate) || !groupRateMatchesModel(rate, model)) {
      continue;
    }

    rows.push(
      createRowFromCandidate(
        model,
        rateCandidate(rate, station, stationKeyNameById),
        evidenceByStationModel,
      ),
    );
  }

  return rows.sort(compareRows);
}

function createRowFromCandidate(
  model: PricingComparisonCatalogEntry,
  candidate: GroupCandidate,
  evidenceByStationModel: Map<string, PricingEvidenceStatus>,
): PricingComparisonRow {
  const creditPerCny = safeCreditPerCny(candidate.station.creditPerCny);
  const effectiveMultiplier =
    candidate.groupMultiplier === null ? null : candidate.groupMultiplier / creditPerCny;
  const estimatedInputCny =
    effectiveMultiplier === null ? null : model.officialInputPrice * effectiveMultiplier;
  const estimatedOutputCny =
    effectiveMultiplier === null ? null : model.officialOutputPrice * effectiveMultiplier;
  const evidenceStatus =
    evidenceByStationModel.get(`${candidate.station.id}\u0000${model.modelId}`) ?? "unverified";

  return {
    id: [
      model.modelId,
      candidate.station.id,
      candidate.groupBindingId ?? "no-binding",
      candidate.groupRateRecordId ?? "no-rate",
      candidate.groupKeyHash,
    ].join(":"),
    provider: model.provider,
    modelId: model.modelId,
    displayName: model.displayName,
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
    officialInputPrice: model.officialInputPrice,
    officialOutputPrice: model.officialOutputPrice,
    estimatedInputCny,
    estimatedOutputCny,
    evidenceStatus,
    evidenceLabel: evidenceLabels[evidenceStatus],
    source: candidate.source,
    checkedAt: candidate.checkedAt,
    isCheapest: false,
  };
}

function bindingCandidate(
  binding: StationGroupBinding,
  station: Station,
  rate: GroupRateRecord | null,
  stationKeyNameById: Map<string, string>,
): GroupCandidate {
  const stationKeyId = binding.stationKeyId ?? rate?.stationKeyId ?? null;
  return {
    station,
    stationKeyId,
    stationKeyName: stationKeyId ? stationKeyNameById.get(stationKeyId) ?? null : null,
    groupBindingId: binding.id,
    groupRateRecordId: rate?.id ?? null,
    groupKeyHash: binding.groupKeyHash,
    groupName: binding.groupName,
    groupRawJsonRedacted: rate?.rawJsonRedacted ?? binding.rawJsonRedacted,
    groupMultiplier: firstFiniteNumber(
      rate?.effectiveRateMultiplier,
      binding.effectiveRateMultiplier,
      rate?.userRateMultiplier,
      binding.userRateMultiplier,
      rate?.defaultRateMultiplier,
      binding.defaultRateMultiplier,
    ),
    source: rate?.source ?? binding.rateSource ?? "station_group_binding",
    checkedAt: rate?.checkedAt ?? binding.lastCheckedAt ?? binding.updatedAt,
  };
}

function rateCandidate(
  rate: GroupRateRecord,
  station: Station,
  stationKeyNameById: Map<string, string>,
): GroupCandidate {
  const stationKeyId = rate.stationKeyId ?? null;
  return {
    station,
    stationKeyId,
    stationKeyName: stationKeyId ? stationKeyNameById.get(stationKeyId) ?? null : null,
    groupBindingId: rate.groupBindingId,
    groupRateRecordId: rate.id,
    groupKeyHash: rate.groupKeyHash,
    groupName: rate.groupName,
    groupRawJsonRedacted: rate.rawJsonRedacted,
    groupMultiplier: firstFiniteNumber(
      rate.effectiveRateMultiplier,
      rate.userRateMultiplier,
      rate.defaultRateMultiplier,
    ),
    source: rate.source,
    checkedAt: rate.checkedAt,
  };
}

function modelMatchesProviderFilter(
  model: PricingComparisonCatalogEntry,
  filters: Required<PricingComparisonFilters>,
) {
  return filters.provider === "all" || model.provider === filters.provider;
}

function modelMatchesQuery(model: PricingComparisonCatalogEntry, modelQuery: string) {
  const query = normalizeText(modelQuery);
  if (!query) {
    return true;
  }

  return [model.modelId, model.displayName, ...model.aliases]
    .map(normalizeText)
    .some((value) => value.includes(query));
}

function rowMatchesFilters(
  row: PricingComparisonRow,
  filters: Required<PricingComparisonFilters>,
  modelQueryMatches: boolean,
) {
  if (filters.stationId !== "all" && row.stationId !== filters.stationId) {
    return false;
  }
  if (filters.verifiedOnly && row.evidenceStatus !== "discovered") {
    return false;
  }
  const query = normalizeText(filters.modelQuery);
  if (query && !modelQueryMatches && !rowMatchesQuery(row, query)) {
    return false;
  }
  return true;
}

function rowMatchesQuery(row: PricingComparisonRow, query: string) {
  return [row.stationName, row.stationKeyName ?? "", row.groupName]
    .map(normalizeText)
    .some((value) => value.includes(query));
}

function groupBindingMatchesModel(
  binding: StationGroupBinding,
  model: PricingComparisonCatalogEntry,
) {
  const platform = groupPlatformFromRawJson(binding.rawJsonRedacted);
  if (platform) {
    return platformMatchesProvider(platform, model.provider);
  }
  const groupType = binding.groupIdHash?.trim() ?? "";
  if (groupType) {
    return groupTypeMatchesModel(binding.stationId, groupType, binding.groupName, model);
  }
  return legacyGroupTextMatchesModel(
    [binding.groupName, binding.bindingStatus, binding.rateSource, searchableJsonText(binding.rawJsonRedacted)].join(" "),
    model.groupMatchers,
  );
}

function groupRateMatchesModel(
  rate: GroupRateRecord,
  model: PricingComparisonCatalogEntry,
) {
  const platform = groupPlatformFromRawJson(rate.rawJsonRedacted);
  if (platform) {
    return platformMatchesProvider(platform, model.provider);
  }
  return legacyGroupTextMatchesModel(
    [rate.groupName, rate.source, searchableJsonText(rate.rawJsonRedacted)].join(" "),
    model.groupMatchers,
  );
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

function platformMatchesProvider(platform: string, provider: OfficialModelProvider) {
  if (platform === provider) {
    return true;
  }
  return platform === "gemini" && provider === "google";
}

function groupTypeMatchesModel(
  stationId: string,
  groupType: string,
  groupName: string,
  model: PricingComparisonCatalogEntry,
) {
  if (isImageNamedGptGroup(groupType, groupName)) {
    return isImageGenerationModel(model);
  }
  if (textMatchesAnyMatcher(groupType, model.groupMatchers)) {
    return true;
  }
  return structuredProviderGroupTypesByStation[stationId]?.[model.provider]?.includes(groupType) ?? false;
}

function isImageNamedGptGroup(groupType: string, groupName: string) {
  return normalizeText(groupType) === "gpt" && textMatchesAnyMatcher(groupName, imageGenerationGroupMatchers);
}

function isImageGenerationModel(model: PricingComparisonCatalogEntry) {
  return textMatchesAnyMatcher(
    [model.modelId, model.displayName, ...model.aliases, ...model.groupMatchers].join(" "),
    imageGenerationGroupMatchers,
  );
}

const imageGenerationGroupMatchers = ["image", "images", "图片", "图像", "绘图", "画图", "生图"];

const structuredProviderGroupTypesByStation: Record<
  string,
  Partial<Record<OfficialModelProvider, string[]>>
> = {
  "station-1783311325734-4639": {
    openai: ["3", "23"],
    anthropic: ["13", "17"],
  },
  "station-1783351745197-26": {
    openai: ["2", "24", "59", "62", "75"],
    anthropic: ["22", "57", "61"],
    google: ["7"],
  },
  "station-1783237821989-3": {
    openai: ["23", "25", "26", "27", "28", "29", "30", "32", "33", "34", "36"],
  },
  "station-1783042263655-1": {
    openai: ["2", "4", "5", "12", "13"],
    anthropic: ["7", "8", "11", "17"],
  },
  "station-1783351851692-74": {
    openai: ["2", "7", "9", "10", "15"],
    anthropic: ["4", "16", "17"],
  },
  "station-1782477763399": {
    openai: ["8"],
    anthropic: ["15"],
  },
};

function legacyGroupTextMatchesModel(value: string, matchers: string[]) {
  return textMatchesAnyMatcher(value, matchers);
}

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

function isStationGroupBinding(binding: StationGroupBinding) {
  return (
    binding.bindingKind === "station_group" &&
    binding.bindingStatus !== "disabled" &&
    binding.bindingStatus !== "missing" &&
    binding.bindingStatus !== "manual_legacy"
  );
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

function isStationGroupRate(rate: GroupRateRecord) {
  return rate.bindingKind === "station_group";
}

function isRateForBinding(rate: GroupRateRecord, binding: StationGroupBinding) {
  return (
    rate.stationId === binding.stationId &&
    isStationGroupRate(rate) &&
    (rate.groupBindingId === binding.id ||
      rate.groupKeyHash === binding.groupKeyHash ||
      normalizeText(rate.groupName) === normalizeText(binding.groupName))
  );
}

function isRateBackedByActiveGroup(
  rate: GroupRateRecord,
  activeGroupBindingIds: Set<string>,
  activeGroupKeys: Set<string>,
  inactiveGroupKeys: Set<string>,
) {
  if (!isStationGroupRate(rate)) {
    return false;
  }
  if (rate.groupBindingId) {
    return activeGroupBindingIds.has(rate.groupBindingId);
  }
  if (inactiveGroupKeys.has(rate.groupKeyHash) && !activeGroupKeys.has(rate.groupKeyHash)) {
    return false;
  }
  return true;
}

function latestRatesByStationGroup(rates: GroupRateRecord[]) {
  const latestByKey = new Map<string, GroupRateRecord>();

  for (const rate of rates) {
    if (!isStationGroupRate(rate)) {
      continue;
    }

    const key = [rate.stationId, rate.groupKeyHash || normalizeText(rate.groupName)].join("\u0000");
    const current = latestByKey.get(key);
    if (!current || compareRatesByFreshness(rate, current) < 0) {
      latestByKey.set(key, rate);
    }
  }

  return Array.from(latestByKey.values()).sort(compareRatesByFreshness);
}

function firstFiniteNumber(...values: Array<number | null | undefined>) {
  for (const value of values) {
    if (typeof value === "number" && Number.isFinite(value)) {
      return value;
    }
  }
  return null;
}

function safeCreditPerCny(value: number) {
  return Number.isFinite(value) && value > 0 ? value : 1;
}

function markCheapestRows(rows: PricingComparisonRow[]) {
  const cheapestIndex = rows.findIndex((row) => row.estimatedOutputCny !== null);
  if (cheapestIndex < 0) {
    return rows;
  }
  return rows.map((row, index) => ({ ...row, isCheapest: index === cheapestIndex }));
}

function buildMetrics(sections: PricingModelSection[]): PricingComparisonMetrics {
  const rows = sections.flatMap((section) => section.rows);
  const rowsWithEffectiveMultiplier = rows.filter(
    (row): row is PricingComparisonRow & { effectiveMultiplier: number } =>
      row.effectiveMultiplier !== null,
  );
  const lowestRow = rowsWithEffectiveMultiplier.sort(
    (left, right) =>
      left.effectiveMultiplier - right.effectiveMultiplier ||
      compareText(left.displayName, right.displayName) ||
      compareText(left.stationName, right.stationName) ||
      compareText(left.groupName, right.groupName),
  )[0];

  return {
    coveredModelCount: sections.filter((section) => section.rows.length > 0).length,
    comparableGroupCount: rows.filter((row) => row.estimatedOutputCny !== null).length,
    lowestEffectiveMultiplier: lowestRow?.effectiveMultiplier ?? null,
    lowestEffectiveMultiplierLabel: lowestRow
      ? `${lowestRow.displayName} / ${lowestRow.stationName} / ${lowestRow.groupName}`
      : "",
  };
}

function emptyMetrics(): PricingComparisonMetrics {
  return {
    coveredModelCount: 0,
    comparableGroupCount: 0,
    lowestEffectiveMultiplier: null,
    lowestEffectiveMultiplierLabel: "",
  };
}

function compareModels(left: PricingComparisonCatalogEntry, right: PricingComparisonCatalogEntry) {
  return compareText(left.displayName, right.displayName) || compareText(left.modelId, right.modelId);
}

function compareRows(left: PricingComparisonRow, right: PricingComparisonRow) {
  return (
    compareNullableNumbers(left.estimatedOutputCny, right.estimatedOutputCny) ||
    compareText(left.stationName, right.stationName) ||
    compareText(left.groupName, right.groupName) ||
    compareText(left.id, right.id)
  );
}

function compareRatesByFreshness(left: GroupRateRecord, right: GroupRateRecord) {
  return dateTimeValue(right.checkedAt) - dateTimeValue(left.checkedAt) || compareText(left.id, right.id);
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

function dateTimeValue(value: string | null) {
  if (!value) {
    return 0;
  }
  const time = new Date(value).getTime();
  return Number.isFinite(time) ? time : 0;
}
