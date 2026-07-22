import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: mocks.invoke,
}));

describe("data recovery API", () => {
  beforeEach(() => {
    mocks.invoke.mockReset();
  });

  it("returns the documented browser preview state only when Tauri invoke is unavailable", async () => {
    mocks.invoke.mockRejectedValue(new Error("window.__TAURI_INTERNALS__ is undefined"));
    const { getDataStoreStartupState } = await import("./dataRecovery");

    await expect(getDataStoreStartupState()).resolves.toEqual({
      mode: "writable",
      databaseGeneration: "two",
      compatibility: {
        decisionCode: "writable",
        schemaVersion: null,
        appVersion: "browser-preview",
      },
      capabilities: {
        canBackup: false,
        canExportDiagnostic: false,
        canCheckForUpdates: false,
        canLocateCandidate: false,
        canActivateCandidate: false,
        canCreateDataStore: false,
      },
      decision: { kind: "ready", candidateId: "browser-preview" },
      candidates: [],
    });
  });

  it("activates an inspected candidate by opaque id instead of returning its path", async () => {
    mocks.invoke.mockResolvedValue({ restartRequired: true });
    const { activateDataStoreCandidate } = await import("./dataRecovery");

    await activateDataStoreCandidate("candidate-7");

    expect(mocks.invoke).toHaveBeenCalledWith("activate_data_store_candidate", {
      candidateId: "candidate-7",
    });
  });

  it("does not hide ACL or missing-command errors behind browser preview fallback", async () => {
    const { getDataStoreStartupState } = await import("./dataRecovery");

    mocks.invoke.mockRejectedValue(new Error("Command get_data_store_startup_state not allowed by ACL"));
    await expect(getDataStoreStartupState()).rejects.toThrow(/not allowed by ACL/i);

    mocks.invoke.mockRejectedValue(new Error("Command get_data_store_startup_state not found"));
    await expect(getDataStoreStartupState()).rejects.toThrow(/not found/i);
  });

  it("fails closed when a stale or malformed startup DTO is returned", async () => {
    const { getDataStoreStartupState } = await import("./dataRecovery");

    mocks.invoke.mockResolvedValue({
      decision: { kind: "ready", candidateId: "legacy" },
      candidates: [],
    });

    await expect(getDataStoreStartupState()).rejects.toThrow(/invalid data store startup response/i);
  });

  it("fails closed when manual location returns a malformed candidate", async () => {
    mocks.invoke.mockResolvedValue({ id: "candidate-without-health" });
    const { locateDataStoreCandidate } = await import("./dataRecovery");

    await expect(locateDataStoreCandidate()).rejects.toThrow(/invalid data store candidate response/i);
  });

  it("parses backend recovery evidence into a selectable candidate", async () => {
    mocks.invoke.mockResolvedValue({
      mode: "recovery",
      databaseGeneration: "one",
      compatibility: null,
      capabilities: {
        canBackup: true,
        canExportDiagnostic: true,
        canCheckForUpdates: true,
        canLocateCandidate: true,
        canActivateCandidate: true,
        canCreateDataStore: true,
      },
      decision: { kind: "needsRecovery", reason: "upgradeRecoveryRequired" },
      candidates: [{
        id: "Located:D:\\Relay Pool\\relay-pool-desktop-v2.sqlite3",
        role: "located",
        path: "D:\\Relay Pool\\relay-pool-desktop-v2.sqlite3",
        health: "healthy",
        databaseGeneration: "two",
        compatibility: {
          decisionCode: "writable",
          schemaVersion: null,
          appVersion: "0.3.1",
        },
        sizeBytes: 4096,
        modifiedAt: null,
        counts: { stations: 2 },
      }],
    });
    const { getDataStoreStartupState } = await import("./dataRecovery");
    const { buildRecoveryViewModel } = await import("@/features/data-recovery/recoveryViewModel");

    const state = await getDataStoreStartupState();
    const viewModel = buildRecoveryViewModel(state);

    expect(viewModel.candidates[0]).toMatchObject({
      generationLabel: "Generation 2",
      selectable: true,
    });
  });
});
