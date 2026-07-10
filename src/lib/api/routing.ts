import { invoke } from "@tauri-apps/api/core";
import type {
  ModelAlias,
  RouteSimulationInput,
  RouteSimulationResult,
  StationKeyCapabilities,
  StationKeyHealth,
  UpdateStationKeyCapabilitiesInput,
  UpsertModelAliasInput,
} from "@/lib/types/routing";

let memoryAliases: ModelAlias[] = [];
const memoryCapabilities = new Map<string, StationKeyCapabilities>();
const memoryHealth = new Map<string, StationKeyHealth>();

export function getStationKeyCapabilities(stationKeyId: string) {
  return invoke<StationKeyCapabilities>("get_station_key_capabilities", { stationKeyId }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      return memoryCapabilities.get(stationKeyId) ?? defaultCapabilities(stationKeyId);
    }
    throw error;
  });
}

export function updateStationKeyCapabilities(input: UpdateStationKeyCapabilitiesInput) {
  return invoke<StationKeyCapabilities>("update_station_key_capabilities", { input }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      const next = { ...input, updatedAt: new Date().toISOString() };
      memoryCapabilities.set(input.stationKeyId, next);
      return next;
    }
    throw error;
  });
}

export function listModelAliases() {
  return invoke<ModelAlias[]>("list_model_aliases").catch((error) => {
    if (isInvokeUnavailable(error)) {
      return memoryAliases;
    }
    throw error;
  });
}

export function upsertModelAlias(input: UpsertModelAliasInput) {
  return invoke<ModelAlias>("upsert_model_alias", { input }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      const now = new Date().toISOString();
      const next: ModelAlias = {
        id: input.id ?? `alias-${Date.now()}`,
        clientModel: input.clientModel,
        upstreamModel: input.upstreamModel,
        enabled: input.enabled,
        note: input.note,
        createdAt: now,
        updatedAt: now,
      };
      memoryAliases = [next, ...memoryAliases.filter((alias) => alias.id !== next.id)];
      return next;
    }
    throw error;
  });
}

export function deleteModelAlias(id: string) {
  return invoke<void>("delete_model_alias", { id }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      memoryAliases = memoryAliases.filter((alias) => alias.id !== id);
      return;
    }
    throw error;
  });
}

export function listStationKeyHealth() {
  return invoke<StationKeyHealth[]>("list_station_key_health").catch((error) => {
    if (isInvokeUnavailable(error)) {
      return Array.from(memoryHealth.values());
    }
    throw error;
  });
}

export function getStationKeyHealth(stationKeyId: string) {
  return invoke<StationKeyHealth>("get_station_key_health", { stationKeyId }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      return memoryHealth.get(stationKeyId) ?? defaultHealth(stationKeyId);
    }
    throw error;
  });
}

export function simulateRoute(input: RouteSimulationInput) {
  return invoke<RouteSimulationResult>("simulate_route", { input }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      return {
        selectedStationKeyId: null,
        selectedStationId: null,
        mappedModel: input.model,
        policy: input.policy ?? "cost_stable_first",
        candidates: [],
        message: "浏览器预览环境没有 Tauri 后端，无法模拟真实候选。",
      };
    }
    throw error;
  });
}

function defaultCapabilities(stationKeyId: string): StationKeyCapabilities {
  return {
    stationKeyId,
    supportsChatCompletions: true,
    supportsResponses: true,
    supportsEmbeddings: false,
    supportsStream: true,
    supportsTools: false,
    supportsVision: false,
    supportsReasoning: false,
    modelAllowlist: [],
    modelBlocklist: [],
    preferredModels: [],
    onlyUseAsBackup: false,
    routingTags: [],
    updatedAt: new Date().toISOString(),
  };
}

function defaultHealth(stationKeyId: string): StationKeyHealth {
  return {
    stationKeyId,
    lastSuccessAt: null,
    lastFailureAt: null,
    consecutiveFailures: 0,
    successCount: 0,
    failureCount: 0,
    avgLatencyMs: null,
    lastErrorSummary: null,
    cooldownUntil: null,
    updatedAt: new Date().toISOString(),
  };
}

function isInvokeUnavailable(error: unknown) {
  return error instanceof Error && /invoke|__TAURI__/i.test(error.message);
}
