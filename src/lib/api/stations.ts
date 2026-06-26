import { invoke } from "@tauri-apps/api/core";
import { mockStations } from "@/lib/mock";
import type { Station, StationInput, StationUpdateInput } from "@/lib/types/stations";

let memoryStations: Station[] | null = null;

function isInvokeUnavailable(error: unknown) {
  return error instanceof Error && /invoke/i.test(error.message);
}

function ensureMemoryStations() {
  if (memoryStations) {
    return memoryStations;
  }

  memoryStations = mockStations.map((station, index) => ({
    id: station.id,
    name: station.name,
    stationType: station.type,
    baseUrl: `https://${station.baseUrlHost}/v1`,
    apiKeyMasked: "sk-local-****",
    apiKeyPresent: station.enabled,
    enabled: station.enabled,
    priority: index,
    creditPerCny: 1,
    balanceRaw: null,
    balanceCny: station.balanceCny,
    lowBalanceThresholdCny: 15,
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
    if (isInvokeUnavailable(error)) {
      return ensureMemoryStations();
    }
    throw error;
  });
}

export function createStation(input: StationInput) {
  return invoke<Station>("create_station", { input }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      const now = new Date().toISOString();
      const nextStation: Station = {
        id: `mock-${Date.now()}`,
        name: input.name,
        stationType: input.stationType,
        baseUrl: input.baseUrl,
        apiKeyMasked: "sk-mock-****",
        apiKeyPresent: Boolean(input.apiKey),
        enabled: input.enabled,
        priority: ensureMemoryStations().length,
        creditPerCny: input.creditPerCny,
        balanceRaw: null,
        balanceCny: null,
        lowBalanceThresholdCny: input.lowBalanceThresholdCny,
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
    if (isInvokeUnavailable(error)) {
      const now = new Date().toISOString();
      const nextStations = updateMemoryStations((stations) =>
        stations.map((station) =>
          station.id === input.id
            ? {
                ...station,
                name: input.name,
                stationType: input.stationType,
                baseUrl: input.baseUrl,
                apiKeyMasked: input.apiKey ? "sk-mock-****" : station.apiKeyMasked,
                apiKeyPresent: input.apiKey ? true : station.apiKeyPresent,
                enabled: input.enabled,
                creditPerCny: input.creditPerCny,
                lowBalanceThresholdCny: input.lowBalanceThresholdCny,
                note: input.note,
                updatedAt: now,
              }
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
    if (isInvokeUnavailable(error)) {
      updateMemoryStations((stations) => stations.filter((station) => station.id !== id));
      return;
    }
    throw error;
  });
}

export function reorderStations(stationIds: string[]) {
  return invoke<Station[]>("reorder_stations", { stationIds }).catch((error) => {
    if (isInvokeUnavailable(error)) {
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
