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
  }>;
};

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
      };
    }),
  }));
}

function cheapestOutput(rules: PricingRule[]) {
  return rules.reduce<PricingRule | null>((best, rule) => {
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

function toTime(value: string) {
  const numeric = Number(value);
  const date = Number.isFinite(numeric) && numeric > 1000000000000 ? new Date(numeric) : new Date(value);
  return Number.isNaN(date.getTime()) ? 0 : date.getTime();
}
