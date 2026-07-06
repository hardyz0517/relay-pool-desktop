import { invoke } from "@tauri-apps/api/core";
import type {
  GroupRateRecord,
  StationGroupBinding,
  UpsertStationGroupBindingInput,
} from "@/lib/types/groupFacts";

const memoryBindings = new Map<string, StationGroupBinding[]>();
const memoryRates = new Map<string, GroupRateRecord[]>();

export function listStationGroupBindings(stationId: string) {
  return invoke<StationGroupBinding[]>("list_station_group_bindings", { stationId }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      return memoryBindings.get(stationId) ?? [];
    }
    throw error;
  });
}

export function listGroupRateRecords(stationId: string) {
  return invoke<GroupRateRecord[]>("list_group_rate_records", { stationId }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      return memoryRates.get(stationId) ?? [];
    }
    throw error;
  });
}

export function upsertStationGroupBinding(input: UpsertStationGroupBindingInput) {
  return invoke<StationGroupBinding>("upsert_station_group_binding", { input }).catch((error) => {
    if (!isInvokeUnavailable(error)) {
      throw error;
    }
    const now = new Date().toISOString();
    const existingBindings = memoryBindings.get(input.stationId) ?? [];
    const existingIndex = existingBindings.findIndex(
      (binding) =>
        binding.bindingKind === input.bindingKind &&
        binding.groupKeyHash === input.groupKeyHash &&
        (input.bindingKind === "station_group" || binding.stationKeyId === input.stationKeyId),
    );
    const binding: StationGroupBinding = {
      id: existingIndex >= 0 ? existingBindings[existingIndex].id : `group-binding-${Date.now()}`,
      stationId: input.stationId,
      stationKeyId: input.stationKeyId,
      bindingKind: input.bindingKind,
      parentGroupBindingId: input.parentGroupBindingId,
      groupKeyHash: input.groupKeyHash,
      groupIdHash: input.groupIdHash,
      groupName: input.groupName,
      bindingStatus: input.bindingStatus,
      defaultRateMultiplier: input.defaultRateMultiplier,
      userRateMultiplier: input.userRateMultiplier,
      effectiveRateMultiplier: input.effectiveRateMultiplier,
      rateSource: input.rateSource,
      confidence: input.confidence,
      lastSeenAt: input.lastSeenAt,
      lastCheckedAt: now,
      lastRateChangedAt: now,
      rawJsonRedacted: input.rawJsonRedacted,
      createdAt: existingIndex >= 0 ? existingBindings[existingIndex].createdAt : now,
      updatedAt: now,
    };
    const nextBindings = [...existingBindings];
    if (existingIndex >= 0) {
      nextBindings[existingIndex] = binding;
    } else {
      nextBindings.push(binding);
    }
    memoryBindings.set(input.stationId, nextBindings);
    return binding;
  });
}

function isInvokeUnavailable(error: unknown) {
  return error instanceof Error && /invoke|__TAURI__/i.test(error.message);
}
