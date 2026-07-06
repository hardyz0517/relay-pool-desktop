import type { PricingRule } from "@/lib/types/economics";
import type { Station } from "@/lib/types/stations";
import type { RateMultiplierRow } from "./rateSnapshotParser";

export type PriceMatrixCell = {
  stationId: string;
  model: string;
  groupName: string | null;
  inputPrice: number | null;
  outputPrice: number | null;
  fixedPrice: number | null;
  currency: string;
  normalizationStatus: string;
  rateMultiplier: number | null;
  groupBindingId: string | null;
  source: string;
  confidence: number;
  updatedAt: string;
  isCheapestOutput: boolean;
  available: boolean;
};

export type PriceMatrixRow = {
  model: string;
  cells: PriceMatrixCell[];
};

export type RateMatrixRow = {
  groupName: string;
  cells: Array<{
    stationId: string;
    multiplier: number | null;
    updatedAt: string;
    source: string;
    status: string;
    confidence: number;
  }>;
};

export function buildGroupRateOnlyPricingRules(
  rates: RateMultiplierRow[],
  stations: Station[],
  existingRules: PricingRule[],
): PricingRule[] {
  const stationIds = new Set(stations.map((station) => station.id));
  const enabledModelNames = Array.from(new Set(existingRules.filter((rule) => rule.enabled).map((rule) => rule.model)));
  const generated: PricingRule[] = [];

  for (const rate of newestRatesByStationGroup(rates)) {
    if (!stationIds.has(rate.stationId) || rate.multiplier == null || rate.modelPrefixes.length === 0) {
      continue;
    }
    const models = matchingModels(enabledModelNames, rate.modelPrefixes, fallbackModelsForProvider(rate.modelProvider));
    for (const model of models) {
      if (hasRuleForStationModel(existingRules, rate.stationId, model)) {
        continue;
      }
      generated.push({
        id: `group-rate-only:${rate.stationId}:${rate.groupBindingId ?? rate.groupName}:${model}`,
        stationId: rate.stationId,
        stationKeyId: null,
        groupBindingId: rate.groupBindingId,
        groupName: rate.groupName,
        tierLabel: rate.modelProvider,
        model,
        inputPrice: null,
        outputPrice: null,
        fixedPrice: null,
        rateMultiplier: rate.multiplier,
        currency: "CNY",
        unit: "rate_multiplier",
        priceType: "group_rate",
        basePriceSource: null,
        normalizationStatus: "group_rate_only",
        source: `${rate.source}:model_family`,
        confidence: Math.min(rate.confidence, 0.75),
        enabled: true,
        note: rate.modelFamilies.length > 0 ? rate.modelFamilies.join(", ") : null,
        collectedAt: rate.updatedAt,
        validFrom: null,
        validUntil: null,
        createdAt: rate.updatedAt,
        updatedAt: rate.updatedAt,
      });
    }
  }

  return generated;
}

export function buildPriceMatrix(rules: PricingRule[], stations: Station[]): PriceMatrixRow[] {
  const enabledRules = rules.filter((rule) => rule.enabled);
  const models = Array.from(new Set(enabledRules.map((rule) => rule.model))).sort((a, b) => a.localeCompare(b));
  return models.map((model) => {
    const modelRules = enabledRules.filter((rule) => rule.model === model);
    const cheapest = cheapestOutput(modelRules);
    return {
      model,
      cells: stations.map((station) => {
        const rule = newestRule(modelRules.filter((item) => item.stationId === station.id));
        return {
          stationId: station.id,
          model,
          groupName: rule?.groupName ?? null,
          inputPrice: rule?.inputPrice ?? null,
          outputPrice: rule?.outputPrice ?? null,
          fixedPrice: rule?.fixedPrice ?? null,
          currency: rule?.currency ?? "-",
          normalizationStatus: rule?.normalizationStatus ?? "unknown",
          rateMultiplier: rule?.rateMultiplier ?? null,
          groupBindingId: rule?.groupBindingId ?? null,
          source: rule?.source ?? "",
          confidence: rule?.confidence ?? 0,
          updatedAt: rule?.updatedAt ?? "",
          isCheapestOutput: Boolean(rule && cheapest && rule.id === cheapest.id),
          available: Boolean(rule),
        };
      }),
    };
  });
}

export function buildRateMatrix(rates: RateMultiplierRow[], stations: Station[]): RateMatrixRow[] {
  const groupNames = Array.from(new Set(rates.map((rate) => rate.groupName))).sort((a, b) => a.localeCompare(b));
  return groupNames.map((groupName) => ({
    groupName,
    cells: stations.map((station) => {
      const newest = newestRate(rates.filter((rate) => rate.stationId === station.id && rate.groupName === groupName));
      return {
        stationId: station.id,
        multiplier: newest?.multiplier ?? null,
        updatedAt: newest?.updatedAt ?? "",
        source: newest?.source ?? "",
        status: newest?.status ?? "unavailable",
        confidence: newest?.confidence ?? 0,
      };
    }),
  }));
}

function cheapestOutput(rules: PricingRule[]) {
  return rules.filter((rule) => rule.normalizationStatus === "complete").reduce<PricingRule | null>((best, rule) => {
    const value = comparablePrice(rule);
    if (!Number.isFinite(value)) {
      return best;
    }
    if (!best || value < comparablePrice(best)) {
      return rule;
    }
    return best;
  }, null);
}

function comparablePrice(rule: PricingRule) {
  return rule.outputPrice ?? rule.inputPrice ?? rule.fixedPrice ?? Number.POSITIVE_INFINITY;
}

function newestRule(rules: PricingRule[]) {
  return [...rules].sort((a, b) => toTime(b.updatedAt) - toTime(a.updatedAt))[0] ?? null;
}

function newestRate(rates: RateMultiplierRow[]) {
  return [...rates].sort((a, b) => toTime(b.updatedAt) - toTime(a.updatedAt))[0] ?? null;
}

function newestRatesByStationGroup(rates: RateMultiplierRow[]) {
  const latest = new Map<string, RateMultiplierRow>();
  for (const rate of rates) {
    const key = `${rate.stationId}:${rate.groupBindingId ?? rate.groupName}`;
    const previous = latest.get(key);
    if (!previous || toTime(rate.updatedAt) > toTime(previous.updatedAt)) {
      latest.set(key, rate);
    }
  }
  return Array.from(latest.values());
}

function matchingModels(modelNames: string[], prefixes: string[], fallbacks: string[]) {
  const matches = modelNames.filter((model) =>
    prefixes.some((prefix) => model.toLowerCase().startsWith(prefix.toLowerCase())),
  );
  return matches.length > 0 ? matches : fallbacks;
}

function fallbackModelsForProvider(provider: string | null) {
  if (provider === "openai") {
    return ["gpt-*"];
  }
  if (provider === "anthropic") {
    return ["claude-*"];
  }
  return [];
}

function hasRuleForStationModel(rules: PricingRule[], stationId: string, model: string) {
  return rules.some((rule) => rule.enabled && rule.stationId === stationId && rule.model === model);
}

function toTime(value: string) {
  const numeric = Number(value);
  const date = Number.isFinite(numeric) && numeric > 1000000000000 ? new Date(numeric) : new Date(value);
  return Number.isNaN(date.getTime()) ? 0 : date.getTime();
}
