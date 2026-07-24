import {
  deleteModelAlias as deleteModelAliasGenerated,
  getStationKeyCapabilities as getStationKeyCapabilitiesGenerated,
  getStationKeyHealth as getStationKeyHealthGenerated,
  listModelAliases as listModelAliasesGenerated,
  listStationKeyHealth as listStationKeyHealthGenerated,
  simulateRoute as simulateRouteGenerated,
  updateStationKeyCapabilities as updateStationKeyCapabilitiesGenerated,
  upsertModelAlias as upsertModelAliasGenerated,
} from "@/lib/bridge/generated";
import { isTauriInvokeUnavailable } from "@/lib/tauriErrors";
import type {
  ModelAlias,
  RouteSimulationInput,
  StationKeyCapabilities,
  StationKeyHealth,
  UpdateStationKeyCapabilitiesInput,
  UpsertModelAliasInput,
} from "@/lib/types/routing";

let memoryAliases: ModelAlias[] = [];
const memoryCapabilities = new Map<string, StationKeyCapabilities>();
const memoryHealth = new Map<string, StationKeyHealth>();

export function getStationKeyCapabilities(stationKeyId: string) {
  return getStationKeyCapabilitiesGenerated({ stationKeyId }).catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      return memoryCapabilities.get(stationKeyId) ?? defaultCapabilities(stationKeyId);
    }
    throw error;
  });
}

export function updateStationKeyCapabilities(input: UpdateStationKeyCapabilitiesInput) {
  return updateStationKeyCapabilitiesGenerated(input).catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      const next = { ...input, updatedAt: new Date().toISOString() };
      memoryCapabilities.set(input.stationKeyId, next);
      return next;
    }
    throw error;
  });
}

export function listModelAliases() {
  return listModelAliasesGenerated().catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      return memoryAliases;
    }
    throw error;
  });
}

export function upsertModelAlias(input: UpsertModelAliasInput) {
  return upsertModelAliasGenerated(input).catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
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
  return deleteModelAliasGenerated({ id }).catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      memoryAliases = memoryAliases.filter((alias) => alias.id !== id);
      return;
    }
    throw error;
  });
}

export function listStationKeyHealth() {
  return listStationKeyHealthGenerated().catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      return Array.from(memoryHealth.values());
    }
    throw error;
  });
}

export function getStationKeyHealth(stationKeyId: string) {
  return getStationKeyHealthGenerated({ stationKeyId }).catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      return memoryHealth.get(stationKeyId) ?? defaultHealth(stationKeyId);
    }
    throw error;
  });
}

export function simulateRoute(input: RouteSimulationInput) {
  return simulateRouteGenerated({
    endpoint: input.endpoint,
    model: input.model,
    stream: input.stream,
    usesTools: input.usesTools,
    usesVision: input.usesVision,
    usesReasoning: input.usesReasoning,
    policy: input.policy,
    maxRateMultiplier: input.maxRateMultiplier ?? null,
    routingGroupFilter: input.routingGroupFilter ?? null,
    sessionHash: input.sessionHash ?? null,
    previousResponseId: input.previousResponseId ?? null,
  }).catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
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
