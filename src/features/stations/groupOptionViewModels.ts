import { formatCompactMultiplier } from "@/lib/formatters";
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

export function findMatchingGroupOption(
  row: { groupBindingId: string | null; groupIdHash: string | null; groupName: string },
  options: StationGroupOption[],
) {
  const groupBindingId = row.groupBindingId?.trim() ?? "";
  if (groupBindingId) {
    return options.find((option) => option.groupBindingId === groupBindingId) ?? null;
  }

  const groupIdHash = row.groupIdHash?.trim() ?? "";
  if (groupIdHash) {
    return options.find((option) => option.groupIdHash === groupIdHash) ?? null;
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
) {
  return normalizeStationGroupOptions(
    buildStationGroupOptionsFromCurrentFacts(
      facts.filter(isDisplayableStationGroupCurrentFact),
    ),
  );
}
