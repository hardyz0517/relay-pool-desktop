import { describe, expect, it } from "vitest";
import { IPC_BINDING_HASH, IPC_CONTRACT_VERSION } from "@/lib/bridge/contract";
import { validateRuntimeContract } from "./runtimeContract";

const matching = {
  appVersion: "0.3.2",
  ipcContractVersion: IPC_CONTRACT_VERSION,
  bindingHash: IPC_BINDING_HASH,
  capabilities: ["runtime_contract", "settings", "stations", "alerting"],
};

describe("runtime contract validator", () => {
  it("accepts a matching contract", () => {
    expect(validateRuntimeContract(matching)).toEqual({ ok: true, contract: matching });
  });

  it("fails closed for version, hash, unknown capability and oversized payload", () => {
    expect(validateRuntimeContract({ ...matching, ipcContractVersion: 99 })).toEqual({ ok: false, reason: "version_mismatch" });
    expect(validateRuntimeContract({ ...matching, bindingHash: "0".repeat(64) })).toEqual({ ok: false, reason: "hash_mismatch" });
    expect(validateRuntimeContract({ ...matching, capabilities: ["future_capability"] })).toEqual({ ok: false, reason: "unknown_capability" });
    expect(validateRuntimeContract({ ...matching, appVersion: "x".repeat(9_000) })).toEqual({ ok: false, reason: "invalid_payload" });
  });

  it("rejects contracts without the handshake capability", () => {
    expect(validateRuntimeContract({ ...matching, capabilities: ["settings"] })).toEqual({ ok: false, reason: "missing_capability" });
  });

  it.each([
    { ...matching, secret: "sk-private" },
    { ...matching, databasePath: "C:/Users/private/data.db" },
    { ...matching, nested: { authorization: "Bearer private" } },
  ])("rejects extra or nested fields in a handshake payload", (payload) => {
    expect(validateRuntimeContract(payload)).toEqual({ ok: false, reason: "invalid_payload" });
  });

  it("measures the payload using UTF-8 bytes", () => {
    expect(validateRuntimeContract({ ...matching, appVersion: "界".repeat(3_000) })).toEqual({
      ok: false,
      reason: "invalid_payload",
    });
  });
});
