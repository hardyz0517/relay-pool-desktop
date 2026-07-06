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
  if (value === null || value === undefined) return fallback;
  return Number.isInteger(value) ? String(value) : Number(value.toFixed(6)).toString();
}

export function findMatchingGroupOption(
  row: { groupBindingId: string | null; groupIdHash: string | null; groupName: string },
  options: StationGroupOption[],
) {
  return (
    options.find((option) =>
      Boolean(
        (row.groupBindingId && option.groupBindingId === row.groupBindingId) ||
          (row.groupIdHash && option.groupIdHash === row.groupIdHash) ||
          (row.groupName.trim() && option.groupName.trim() === row.groupName.trim()),
      ),
    ) ?? null
  );
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
