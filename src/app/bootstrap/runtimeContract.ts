import {
  IPC_BINDING_HASH,
  IPC_CONTRACT_VERSION,
  type RuntimeCapability,
  type RuntimeContractInfo,
} from "@/lib/bridge/contract";

const MAX_PAYLOAD_BYTES = 8_192;
const MAX_CAPABILITIES = 64;
const CONTRACT_KEYS = ["appVersion", "ipcContractVersion", "bindingHash", "capabilities"] as const;
const CAPABILITIES = new Set<RuntimeCapability>([
  "runtime_contract",
  "data_recovery",
  "settings",
  "stations",
  "station_keys",
  "collectors",
  "routing",
  "proxy",
  "channel_monitoring",
  "pricing",
  "alerting",
  "capture",
  "typed_streaming",
]);

export type RuntimeContractFailure =
  | "invalid_payload"
  | "version_mismatch"
  | "hash_mismatch"
  | "unknown_capability"
  | "missing_capability";

export type RuntimeContractValidation =
  | { ok: true; contract: RuntimeContractInfo }
  | { ok: false; reason: RuntimeContractFailure };

export function validateRuntimeContract(value: unknown): RuntimeContractValidation {
  if (!isRecord(value) || serializedSize(value) > MAX_PAYLOAD_BYTES) return { ok: false, reason: "invalid_payload" };
  const keys = Object.keys(value);
  if (keys.length !== CONTRACT_KEYS.length || keys.some((key) => !CONTRACT_KEYS.includes(key as typeof CONTRACT_KEYS[number]))) {
    return { ok: false, reason: "invalid_payload" };
  }
  if (typeof value.appVersion !== "string" || !safeVersion(value.appVersion)) return { ok: false, reason: "invalid_payload" };
  if (!Number.isInteger(value.ipcContractVersion) || typeof value.bindingHash !== "string" || !Array.isArray(value.capabilities)) {
    return { ok: false, reason: "invalid_payload" };
  }
  if (value.ipcContractVersion !== IPC_CONTRACT_VERSION) return { ok: false, reason: "version_mismatch" };
  if (value.bindingHash !== IPC_BINDING_HASH) return { ok: false, reason: "hash_mismatch" };
  if (value.capabilities.length === 0 || value.capabilities.length > MAX_CAPABILITIES) return { ok: false, reason: "invalid_payload" };
  const capabilities = value.capabilities.filter((capability): capability is RuntimeCapability => typeof capability === "string");
  if (capabilities.length !== value.capabilities.length || capabilities.some((capability) => !CAPABILITIES.has(capability))) {
    return { ok: false, reason: "unknown_capability" };
  }
  if (!capabilities.includes("runtime_contract")) return { ok: false, reason: "missing_capability" };
  const unique = new Set(capabilities);
  if (unique.size !== capabilities.length) return { ok: false, reason: "invalid_payload" };
  return {
    ok: true,
    contract: {
      appVersion: value.appVersion,
      ipcContractVersion: value.ipcContractVersion,
      bindingHash: value.bindingHash,
      capabilities,
    },
  };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function serializedSize(value: unknown): number {
  try {
    return new TextEncoder().encode(JSON.stringify(value)).byteLength;
  } catch {
    return MAX_PAYLOAD_BYTES + 1;
  }
}

function safeVersion(value: string): boolean {
  return value.length > 0 && value.length <= 128 && /^[0-9A-Za-z.+_-]+$/.test(value);
}
