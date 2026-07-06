import type { GroupRateRecord, StationGroupBinding } from "@/lib/types/groupFacts";

export type RateMultiplierRow = {
  stationId: string;
  groupBindingId: string | null;
  groupName: string;
  multiplier: number | null;
  source: string;
  status: string;
  confidence: number;
  updatedAt: string;
  modelProvider: string | null;
  modelFamilies: string[];
  modelPrefixes: string[];
};

export function rateRowsFromGroupFacts(
  bindings: StationGroupBinding[],
  records: GroupRateRecord[],
): RateMultiplierRow[] {
  const bindingRows = bindings
    .filter((binding) => binding.bindingKind === "station_group")
    .map((binding) => {
      const coverage = inferModelCoverage(binding.groupName, binding.rawJsonRedacted);
      return {
        stationId: binding.stationId,
        groupBindingId: binding.id,
        groupName: binding.groupName,
        multiplier: binding.effectiveRateMultiplier,
        source: binding.rateSource ?? "binding",
        status: binding.bindingStatus,
        confidence: binding.confidence,
        updatedAt: binding.lastCheckedAt ?? binding.updatedAt,
        modelProvider: coverage.provider,
        modelFamilies: coverage.families,
        modelPrefixes: coverage.prefixes,
      };
    });

  const recordRows = records.map((record) => {
    const coverage = inferModelCoverage(record.groupName, record.rawJsonRedacted);
    return {
      stationId: record.stationId,
      groupBindingId: record.groupBindingId,
      groupName: record.groupName,
      multiplier: record.effectiveRateMultiplier,
      source: record.source,
      status: "history",
      confidence: record.confidence,
      updatedAt: record.checkedAt,
      modelProvider: coverage.provider,
      modelFamilies: coverage.families,
      modelPrefixes: coverage.prefixes,
    };
  });

  return [...bindingRows, ...recordRows];
}

type ModelCoverage = {
  provider: string | null;
  families: string[];
  prefixes: string[];
};

const providerCoverage: Record<string, ModelCoverage> = {
  openai: {
    provider: "openai",
    families: ["openai", "gpt"],
    prefixes: ["gpt-", "o1", "o3", "o4", "chatgpt-"],
  },
  anthropic: {
    provider: "anthropic",
    families: ["anthropic", "claude"],
    prefixes: ["claude-"],
  },
};

function inferModelCoverage(groupName: string, raw: Record<string, unknown> | null): ModelCoverage {
  const haystack = [groupName, ...collectRawText(raw)].join(" ").toLowerCase();
  if (/\b(anthropic|claude|yellow|amber|orange)\b/.test(haystack)) {
    return providerCoverage.anthropic;
  }
  if (/\b(openai|gpt|green|emerald|teal|default)\b/.test(haystack)) {
    return providerCoverage.openai;
  }
  return { provider: null, families: [], prefixes: [] };
}

function collectRawText(value: unknown): string[] {
  if (value == null) {
    return [];
  }
  if (typeof value === "string" || typeof value === "number" || typeof value === "boolean") {
    return [String(value)];
  }
  if (Array.isArray(value)) {
    return value.flatMap(collectRawText);
  }
  if (typeof value === "object") {
    return Object.entries(value as Record<string, unknown>).flatMap(([key, item]) => [
      key,
      ...collectRawText(item),
    ]);
  }
  return [];
}
