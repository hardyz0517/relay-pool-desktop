import { invoke } from "@tauri-apps/api/core";
import { isTauriInvokeUnavailable } from "@/lib/tauriErrors";
import { mockStations } from "@/lib/mock";
import type { EndpointPingResult, Station, StationEndpointHealth, StationInput, StationUpdateInput } from "@/lib/types/stations";

let memoryStations: Station[] | null = null;
const memoryEndpointHealth = new Map<string, StationEndpointHealth>();

function ensureMemoryStations() {
  if (memoryStations) {
    return memoryStations;
  }

  memoryStations = mockStations.map((station, index) => ({
    id: station.id,
    name: station.name,
    stationType: station.type,
    websiteUrl: `https://${station.endpointHost}`,
    apiBaseUrl: `https://${station.endpointHost}/v1`,
    endpointRevision: 1,
    collectorProxyMode: "inherit",
    collectorProxyUrl: null,
    apiKeyMasked: "sk-local-****",
    apiKeyPresent: station.enabled,
    keyCount: station.enabled ? 1 : 0,
    enabled: station.enabled,
    priority: index,
    creditPerCny: 1,
    balanceRaw: null,
    balanceCny: station.balanceCny,
    lowBalanceThresholdCny: 15,
    collectionIntervalMinutes: 5,
    status: station.status,
    latencyMs: station.latencyMs,
    lastCheckedAt: station.lastCheckedAt,
    lastPricingFetchedAt: station.lastPricingFetchedAt,
    note: station.recentError ?? null,
    createdAt: new Date().toISOString(),
    updatedAt: new Date().toISOString(),
  }));

  return memoryStations;
}

function updateMemoryStations(mutator: (stations: Station[]) => Station[]) {
  memoryStations = mutator(ensureMemoryStations().map((station) => ({ ...station })));
  return memoryStations;
}

export function listStations() {
  return invoke<Station[]>("list_stations").catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      return ensureMemoryStations();
    }
    throw error;
  });
}

export function createStation(input: StationInput) {
  return invoke<Station>("create_station", { input }).catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      const now = new Date().toISOString();
      const nextStation: Station = {
        id: `mock-${Date.now()}`,
        name: input.name,
        stationType: input.stationType,
        websiteUrl: input.websiteUrl,
        apiBaseUrl: input.apiBaseUrl,
        endpointRevision: 1,
        collectorProxyMode: input.collectorProxyMode,
        collectorProxyUrl: input.collectorProxyUrl,
        apiKeyMasked: "sk-mock-****",
        apiKeyPresent: Boolean(input.apiKey),
        keyCount: input.apiKey ? 1 : 0,
        enabled: input.enabled,
        priority: ensureMemoryStations().length,
        creditPerCny: input.creditPerCny,
        balanceRaw: null,
        balanceCny: null,
        lowBalanceThresholdCny: input.lowBalanceThresholdCny,
        collectionIntervalMinutes: input.collectionIntervalMinutes,
        status: input.enabled ? "unchecked" : "disabled",
        latencyMs: null,
        lastCheckedAt: null,
        lastPricingFetchedAt: null,
        note: input.note,
        createdAt: now,
        updatedAt: now,
      };

      updateMemoryStations((stations) => [...stations, nextStation]);
      return nextStation;
    }
    throw error;
  });
}

export function updateStation(input: StationUpdateInput) {
  return invoke<Station>("update_station", { input }).catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      const now = new Date().toISOString();
      const nextStations = updateMemoryStations((stations) =>
        stations.map((station) =>
          station.id === input.id
            ? (() => {
                const endpointChanged =
                  endpointRevisionKey(station.websiteUrl) !== endpointRevisionKey(input.websiteUrl) ||
                  endpointRevisionKey(station.apiBaseUrl) !== endpointRevisionKey(input.apiBaseUrl);
                return {
                ...station,
                name: input.name,
                stationType: input.stationType,
                websiteUrl: input.websiteUrl,
                apiBaseUrl: input.apiBaseUrl,
                endpointRevision: endpointChanged ? station.endpointRevision + 1 : station.endpointRevision,
                collectorProxyMode: input.collectorProxyMode,
                collectorProxyUrl: input.collectorProxyUrl,
                apiKeyMasked: input.apiKey ? "sk-mock-****" : station.apiKeyMasked,
                apiKeyPresent: input.apiKey ? true : station.apiKeyPresent,
                keyCount: input.apiKey ? Math.max(1, station.keyCount) : station.keyCount,
                enabled: input.enabled,
                creditPerCny: input.creditPerCny,
                lowBalanceThresholdCny: input.lowBalanceThresholdCny,
                collectionIntervalMinutes: input.collectionIntervalMinutes,
                note: input.note,
                updatedAt: now,
                };
              })()
            : station,
        ),
      );
      const nextStation = nextStations.find((station) => station.id === input.id);
      if (!nextStation) {
        throw error;
      }
      return nextStation;
    }
    throw error;
  });
}

export function deleteStation(id: string) {
  return invoke<void>("delete_station", { id }).catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      updateMemoryStations((stations) => stations.filter((station) => station.id !== id));
      return;
    }
    throw error;
  });
}

export function openStationWebsite(url: string) {
  return invoke<void>("open_external_url", { url }).catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      window.open(url, "_blank", "noopener,noreferrer");
      return;
    }
    throw error;
  });
}

function endpointRevisionKey(value: string) {
  return value.trim().replace(/\/+$/, "");
}

export function reorderStations(stationIds: string[]) {
  return invoke<Station[]>("reorder_stations", { stationIds }).catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      const byId = new Map(ensureMemoryStations().map((station) => [station.id, station] as const));
      const nextStations = stationIds
        .map((id, index) => {
          const station = byId.get(id);
          return station ? { ...station, priority: index } : null;
        })
        .filter((station): station is Station => Boolean(station));
      memoryStations = nextStations;
      return nextStations;
    }
    throw error;
  });
}

export function listStationEndpointHealth() {
  return invoke<StationEndpointHealth[]>("list_station_endpoint_health").catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      return Array.from(memoryEndpointHealth.values());
    }
    throw error;
  });
}

export function pingStationEndpoint(stationId: string) {
  return invoke<EndpointPingResult>("ping_station_endpoint", { stationId }).catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      const now = new Date().toISOString();
      const result: EndpointPingResult = {
        stationId,
        ok: false,
        status: "failed",
        latencyMs: null,
        checkedAt: now,
        errorSummary: "浏览器预览环境没有 Tauri 后端，无法执行真实端点 PING。",
      };
      memoryEndpointHealth.set(stationId, {
        stationId,
        status: result.status,
        latencyMs: result.latencyMs,
        checkedAt: result.checkedAt,
        errorSummary: result.errorSummary,
        updatedAt: now,
      });
      return result;
    }
    throw error;
  });
}
