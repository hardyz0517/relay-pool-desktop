import { effectiveRateMultiplierForCredit, formatCompactMultiplier } from "@/lib/formatters";
import {
  buildStationGroupOptionsFromCurrentFacts,
  isDisplayableStationGroupCurrentFact,
  type StationGroupCurrentFact,
} from "@/lib/projections/groupFacts";
import type { StationGroupOption } from "@/lib/types/groupFacts";

export const noGroupOptionValue = "__none__";

export function stationGroupSelectValue(
  option: Pick<StationGroupOption, "value" | "groupBindingId" | "groupIdHash" | "groupName">,
) {
  if (option.value) return option.value;
  if (option.groupBindingId) return `binding:${option.groupBindingId}`;
  if (option.groupIdHash) return `remote:${option.groupIdHash}`;
  return `name:${option.groupName.trim()}`;
}

export function formatMultiplier(value: number | null | undefined, fallback = "未采集倍率") {
  return formatCompactMultiplier(value, fallback);
}

export function formatStationGroupOptionLabel(option: Pick<StationGroupOption, "groupName" | "rateMultiplier">) {
  const rateLabel =
    option.rateMultiplier === null || option.rateMultiplier === undefined
      ? "倍率未知"
      : `${formatMultiplier(option.rateMultiplier)}x`;
  return `${option.groupName} · ${rateLabel}`;
}

export function findMatchingGroupOption(
  row: { groupBindingId: string | null; groupIdHash: string | null; groupName: string },
  options: StationGroupOption[],
) {
  const groupBindingId = row.groupBindingId?.trim() ?? "";
  if (groupBindingId) {
    const bindingMatch = options.find((option) => option.groupBindingId === groupBindingId);
    if (bindingMatch) {
      return bindingMatch;
    }
  }

  const groupIdHash = row.groupIdHash?.trim() ?? "";
  if (groupIdHash) {
    const groupIdMatch = options.find((option) => option.groupIdHash === groupIdHash);
    if (groupIdMatch) {
      return groupIdMatch;
    }
  }

  const groupName = row.groupName.trim();
  if (!groupName) {
    return null;
  }
  return options.find((option) => option.groupName.trim() === groupName) ?? null;
}

export function normalizeStationGroupOptions(options: StationGroupOption[]) {
  const seen = new Set<string>();
  return options.filter((option) => {
    const value = stationGroupSelectValue(option);
    if (seen.has(value)) return false;
    seen.add(value);
    return true;
  });
}

export function buildStationGroupOptionsFromCurrentFactsForSelect(
  facts: StationGroupCurrentFact[],
  creditPerCny = 1,
) {
  return normalizeStationGroupOptions(
    buildStationGroupOptionsFromCurrentFacts(
      facts.filter(isDisplayableStationGroupCurrentFact),
    ).map((option) => ({
      ...option,
      rateMultiplier: effectiveRateMultiplierForCredit(option.rateMultiplier, creditPerCny),
    })),
  );
}

export function buildStationGroupOptionFromRawMultiplierForSelect(
  binding: {
    id: string;
    groupIdHash: string | null;
    groupName: string;
    defaultRateMultiplier: number | null;
    userRateMultiplier: number | null;
    effectiveRateMultiplier: number | null;
    inferredGroupCategory: StationGroupOption["inferredGroupCategory"] | null;
    groupCategoryOverride: StationGroupOption["groupCategoryOverride"];
    rateSource: string | null;
  },
  creditPerCny = 1,
): StationGroupOption {
  const rateMultiplier = firstNumber(
    binding.userRateMultiplier,
    binding.effectiveRateMultiplier,
    binding.defaultRateMultiplier,
  );
  const inferredGroupCategory = binding.inferredGroupCategory ?? "unknown";
  const effectiveGroupCategory = binding.groupCategoryOverride ?? inferredGroupCategory;
  return {
    value: binding.id
      ? `binding:${binding.id}`
      : binding.groupIdHash
        ? `remote:${binding.groupIdHash}`
        : `name:${binding.groupName.trim()}`,
    groupBindingId: binding.id,
    groupIdHash: binding.groupIdHash,
    groupName: binding.groupName,
    rateMultiplier: effectiveRateMultiplierForCredit(rateMultiplier, creditPerCny),
    inferredGroupCategory,
    groupCategoryOverride: binding.groupCategoryOverride,
    effectiveGroupCategory,
    rateSource: binding.rateSource,
    selectableForRemoteKey: Boolean(binding.id || binding.groupIdHash),
  };
}

function firstNumber(...values: Array<number | null | undefined>) {
  for (const value of values) {
    if (typeof value === "number" && Number.isFinite(value)) {
      return value;
    }
  }
  return null;
}
