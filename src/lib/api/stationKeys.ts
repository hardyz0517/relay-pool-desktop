import { invoke } from "@tauri-apps/api/core";
import type {
  CreateStationKeyInput,
  KeyPoolItem,
  StationCredentials,
  StationKey,
  UpdateStationKeyInput,
} from "@/lib/types/stationKeys";

const memoryKeys = new Map<string, StationKey[]>();
const memoryCredentials = new Map<string, StationCredentials>();
let memoryKeyPool: KeyPoolItem[] = [];

export function listStationKeys(stationId: string) {
  return invoke<StationKey[]>("list_station_keys", { stationId }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      return memoryKeys.get(stationId) ?? [];
    }
    throw error;
  });
}

export function createStationKey(input: CreateStationKeyInput) {
  return invoke<StationKey>("create_station_key", { input }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      const key = memoryKeyFromInput(input);
      memoryKeys.set(input.stationId, [...(memoryKeys.get(input.stationId) ?? []), key]);
      return key;
    }
    throw error;
  });
}

export function updateStationKey(input: UpdateStationKeyInput) {
  return invoke<StationKey>("update_station_key", { input }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      const keys = memoryKeys.get(input.stationId) ?? [];
      const nextKeys = keys.map((key) => key.id === input.id ? { ...key, ...input, apiKeyPresent: input.apiKey ? true : key.apiKeyPresent } : key);
      memoryKeys.set(input.stationId, nextKeys);
      return nextKeys.find((key) => key.id === input.id) ?? keys[0];
    }
    throw error;
  });
}

export function deleteStationKey(id: string) {
  return invoke<void>("delete_station_key", { id }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      for (const [stationId, keys] of memoryKeys) {
        memoryKeys.set(stationId, keys.filter((key) => key.id !== id));
      }
      return;
    }
    throw error;
  });
}

export function reorderStationKeys(stationId: string, keyIds: string[]) {
  return invoke<StationKey[]>("reorder_station_keys", { stationId, keyIds }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      const byId = new Map((memoryKeys.get(stationId) ?? []).map((key) => [key.id, key] as const));
      const nextKeys = keyIds.flatMap((id, index) => {
        const key = byId.get(id);
        return key ? [{ ...key, priority: index }] : [];
      });
      memoryKeys.set(stationId, nextKeys);
      return nextKeys;
    }
    throw error;
  });
}

export function listKeyPoolItems() {
  return invoke<KeyPoolItem[]>("list_key_pool_items").catch((error) => {
    if (isInvokeUnavailable(error)) {
      return memoryKeyPool;
    }
    throw error;
  });
}

export function reorderKeyPool(keyIds: string[]) {
  return invoke<KeyPoolItem[]>("reorder_key_pool", { keyIds }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      const byId = new Map(memoryKeyPool.map((item) => [item.id, item] as const));
      memoryKeyPool = keyIds.flatMap((id, index) => {
        const item = byId.get(id);
        return item ? [{ ...item, priority: index }] : [];
      });
      return memoryKeyPool;
    }
    throw error;
  });
}

export function getStationCredentials(stationId: string) {
  return invoke<StationCredentials>("get_station_credentials", { stationId }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      return memoryCredentials.get(stationId) ?? emptyCredentials(stationId);
    }
    throw error;
  });
}

export function updateStationCredentials(input: {
  stationId: string;
  loginUsername: string | null;
  loginPassword: string | null;
  rememberPassword: boolean;
}) {
  return invoke<StationCredentials>("update_station_credentials", { input }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      const next = {
        ...emptyCredentials(input.stationId),
        loginUsername: input.loginUsername,
        passwordPresent: Boolean(input.loginPassword && input.rememberPassword),
        rememberPassword: input.rememberPassword,
        loginStatus: "saved",
        updatedAt: new Date().toISOString(),
      };
      memoryCredentials.set(input.stationId, next);
      return next;
    }
    throw error;
  });
}

export function clearStationCredentials(stationId: string) {
  return invoke<StationCredentials>("clear_station_credentials", { stationId }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      const next = emptyCredentials(stationId);
      memoryCredentials.set(stationId, next);
      return next;
    }
    throw error;
  });
}

function emptyCredentials(stationId: string): StationCredentials {
  return {
    stationId,
    loginUsername: null,
    passwordPresent: false,
    rememberPassword: false,
    loginStatus: "unknown",
    loginError: null,
    lastLoginAt: null,
    sessionStatus: "none",
    sessionExpiresAt: null,
    updatedAt: null,
  };
}

function memoryKeyFromInput(input: CreateStationKeyInput): StationKey {
  const now = new Date().toISOString();
  return {
    id: `key-${Date.now()}`,
    stationId: input.stationId,
    name: input.name,
    apiKeyMasked: input.apiKey ? "sk-****" : "未设置",
    apiKeyPresent: Boolean(input.apiKey),
    enabled: input.enabled,
    priority: input.priority ?? (memoryKeys.get(input.stationId)?.length ?? 0),
    groupBindingId: input.groupBindingId ?? null,
    groupIdHash: input.groupIdHash ?? null,
    groupName: input.groupName,
    tierLabel: input.tierLabel,
    rateMultiplier: input.rateMultiplier ?? null,
    rateSource: input.rateSource ?? null,
    rateCollectedAt: null,
    balanceScope: input.balanceScope ?? null,
    status: "unchecked",
    lastCheckedAt: null,
    lastUsedAt: null,
    note: input.note,
    createdAt: now,
    updatedAt: now,
  };
}

function isInvokeUnavailable(error: unknown) {
  return error instanceof Error && /invoke|__TAURI__/i.test(error.message);
}
