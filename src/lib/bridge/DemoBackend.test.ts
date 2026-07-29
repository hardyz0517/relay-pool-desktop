import { describe, expect, it } from "vitest";
import { DemoBackend } from "./DemoBackend";
import { IPC_BINDING_HASH, IPC_CONTRACT_VERSION } from "./contract";

describe("DemoBackend", () => {
  it("returns a deterministic isolated runtime contract", async () => {
    const backend = new DemoBackend();

    await expect(backend.handshake()).resolves.toEqual({
      appVersion: "demo-relay-pool-demo-v1",
      ipcContractVersion: IPC_CONTRACT_VERSION,
      bindingHash: IPC_BINDING_HASH,
      capabilities: ["runtime_contract"],
    });

    backend.reset();
    await expect(backend.handshake()).resolves.toEqual({
      appVersion: "demo-relay-pool-demo-v1",
      ipcContractVersion: IPC_CONTRACT_VERSION,
      bindingHash: IPC_BINDING_HASH,
      capabilities: ["runtime_contract"],
    });
  });

  it("fails closed with a typed unsupported shape", () => {
    const backend = new DemoBackend();

    let thrown: unknown;
    try {
      backend.unsupported("data_recovery");
    } catch (error) {
      thrown = error;
    }

    expect(thrown).toMatchObject({
      name: "DemoBackendUnsupportedError",
      code: "unsupported",
      retryable: false,
      capability: "data_recovery",
    });
  });

  it("does not provide fake settings or station data", async () => {
    const backend = new DemoBackend();

    await expect(backend.settings.getSettings()).rejects.toMatchObject({
      code: "unsupported",
      capability: "settings",
    });
    await expect(backend.stations.listStations()).rejects.toMatchObject({
      code: "unsupported",
      capability: "stations",
    });
  });

  it("does not provide fake station key or connectivity data", async () => {
    const backend = new DemoBackend();

    await expect(backend.stationKeys.listKeyPoolItems()).rejects.toMatchObject({
      code: "unsupported",
      capability: "station_keys.key_pool",
    });
    await expect(backend.stationKeys.testStationKeyConnectivity("key-1", "gpt-4o-mini")).rejects.toMatchObject({
      code: "unsupported",
      capability: "station_keys.connectivity",
    });
  });
});
