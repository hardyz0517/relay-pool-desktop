import { listStationGroupBindings, upsertStationGroupBinding } from "@/lib/api/groupFacts";
import { createStationKey, deleteStationKey, updateStationKey } from "@/lib/api/stationKeys";
import {
  isCollectedStationGroupBinding,
  type StationGroupBinding,
  type UpsertStationGroupBindingInput,
} from "@/lib/types/groupFacts";
import type { CreateStationKeyInput, UpdateStationKeyInput } from "@/lib/types/stationKeys";
import type { StationGroupDraft } from "../../components/StationGroupRowsEditor";
import type { StationKeyDraft, StationKeyGroupOption } from "../../components/StationKeyRowsEditor";
import {
  buildSavedGroupOptionForSelect,
  groupRowHasMeaningfulContent,
  parseOptionalRateMultiplier,
  validateGroupRows,
  validateKeyRows,
} from "./keyGroupModel";

export type SaveKeyRowsDependencies = {
  createStationKey: (input: CreateStationKeyInput) => Promise<unknown>;
  updateStationKey: (input: UpdateStationKeyInput) => Promise<unknown>;
  deleteStationKey: (id: string) => Promise<unknown>;
};

export type SaveGroupRowsDependencies = {
  listStationGroupBindings: (stationId: string) => Promise<StationGroupBinding[]>;
  upsertStationGroupBinding: (input: UpsertStationGroupBindingInput) => Promise<StationGroupBinding>;
  nowIso: () => string;
};

const defaultSaveKeyRowsDependencies: SaveKeyRowsDependencies = {
  createStationKey,
  updateStationKey,
  deleteStationKey,
};

const defaultSaveGroupRowsDependencies: SaveGroupRowsDependencies = {
  listStationGroupBindings,
  upsertStationGroupBinding,
  nowIso: () => new Date().toISOString(),
};

export async function saveKeyRows(
  targetStationId: string,
  rows: StationKeyDraft[],
  dependencies: SaveKeyRowsDependencies = defaultSaveKeyRowsDependencies,
) {
  validateKeyRows(rows);

  await Promise.all(
    rows
      .filter((row) => row.id && row.deleteRequested)
      .map((row) => dependencies.deleteStationKey(row.id ?? "")),
  );

  const visibleRows = rows
    .filter((row) => !row.deleteRequested)
    .filter((row) => row.id || row.apiKey.trim());

  for (const [priority, row] of visibleRows.entries()) {
    const rateMultiplier = parseOptionalRateMultiplier(row.rateMultiplier);
    const rateFields = row.rateMultiplier.trim()
      ? { rateMultiplier, rateSource: "manual" as const }
      : {};
    const input = {
      stationId: targetStationId,
      name: row.name.trim(),
      enabled: row.enabled,
      priority,
      groupBindingId: row.groupBindingId,
      groupIdHash: row.groupIdHash,
      groupName: row.groupName.trim() ? row.groupName.trim() : null,
      tierLabel: null,
      balanceScope: "station_key",
      note: row.note.trim() ? row.note.trim() : null,
      ...rateFields,
    };

    if (row.id) {
      await dependencies.updateStationKey({
        ...input,
        id: row.id,
        apiKey: row.apiKey.trim() ? row.apiKey.trim() : null,
        status: "unchecked",
      });
      continue;
    }

    if (!row.apiKey.trim()) {
      continue;
    }

    await dependencies.createStationKey({
      ...input,
      apiKey: row.apiKey.trim(),
    });
  }
}

export async function saveGroupRows(
  targetStationId: string,
  rows: StationGroupDraft[],
  creditPerCny = 1,
  dependencies: SaveGroupRowsDependencies = defaultSaveGroupRowsDependencies,
) {
  validateGroupRows(rows);
  const savedOptions: StationKeyGroupOption[] = [];
  const existingBindings = await dependencies.listStationGroupBindings(targetStationId);

  for (const row of rows) {
    if (!groupRowHasMeaningfulContent(row)) {
      continue;
    }

    if (row.deleteRequested) {
      await disableMatchingGroupBindings(targetStationId, row, existingBindings, dependencies);
      continue;
    }

    const groupName = row.groupName.trim();
    const groupKeyHash = resolveGroupKeyHash(row);
    const rateMultiplier = parseOptionalRateMultiplier(row.rateMultiplier);
    if (!groupName && !row.groupBindingId) {
      continue;
    }

    const input: UpsertStationGroupBindingInput = {
      stationId: targetStationId,
      stationKeyId: null,
      bindingKind: "station_group",
      parentGroupBindingId: null,
      groupKeyHash,
      groupIdHash: row.groupIdHash,
      groupName: groupName || row.groupName,
      bindingStatus: "available",
      defaultRateMultiplier: row.source === "remote" ? rateMultiplier : null,
      userRateMultiplier: row.source === "manual" ? rateMultiplier : null,
      effectiveRateMultiplier: rateMultiplier,
      inferredGroupCategory: row.inferredGroupCategory,
      groupCategoryOverride: row.groupCategoryOverride,
      rateSource: row.source === "remote" ? "remote_scan" : "manual",
      confidence: row.source === "remote" ? 0.95 : 1,
      lastSeenAt: row.source === "remote" ? dependencies.nowIso() : null,
      rawJsonRedacted: null,
    };
    const saved = await dependencies.upsertStationGroupBinding(input);
    savedOptions.push(buildSavedGroupOptionForSelect(saved, creditPerCny));
  }

  return savedOptions;
}

async function disableMatchingGroupBindings(
  targetStationId: string,
  row: StationGroupDraft,
  existingBindings: StationGroupBinding[],
  dependencies: SaveGroupRowsDependencies,
) {
  const bindingsToDisable = existingBindings
    .filter(isCollectedStationGroupBinding)
    .filter((binding) => groupBindingMatchesDraft(binding, row));
  for (const binding of bindingsToDisable) {
    await dependencies.upsertStationGroupBinding({
      stationId: targetStationId,
      stationKeyId: null,
      bindingKind: "station_group",
      parentGroupBindingId: binding.parentGroupBindingId,
      groupKeyHash: binding.groupKeyHash,
      groupIdHash: binding.groupIdHash,
      groupName: binding.groupName,
      bindingStatus: "disabled",
      defaultRateMultiplier: null,
      userRateMultiplier: null,
      effectiveRateMultiplier: null,
      inferredGroupCategory: binding.inferredGroupCategory,
      groupCategoryOverride: binding.groupCategoryOverride,
      rateSource: binding.rateSource,
      confidence: binding.confidence,
      lastSeenAt: binding.lastSeenAt,
      rawJsonRedacted: binding.rawJsonRedacted,
    });
  }
}

function groupBindingMatchesDraft(binding: StationGroupBinding, row: StationGroupDraft) {
  const rowName = row.groupName.trim();
  return Boolean(
    (row.groupBindingId && binding.id === row.groupBindingId) ||
      (row.groupIdHash && binding.groupIdHash === row.groupIdHash) ||
      (rowName && binding.groupName.trim() === rowName),
  );
}

function resolveGroupKeyHash(row: StationGroupDraft) {
  if (row.groupKeyHash.trim()) {
    return row.groupKeyHash.trim();
  }
  if (row.groupIdHash) {
    return `remote:${row.groupIdHash}`;
  }
  return buildManualGroupKeyHash(row.groupName);
}

function buildManualGroupKeyHash(groupName: string) {
  const normalizedName = groupName.trim().toLowerCase();
  return `manual:${encodeURIComponent(normalizedName || "unnamed")}`;
}
