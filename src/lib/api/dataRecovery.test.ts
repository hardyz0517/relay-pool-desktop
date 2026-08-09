import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { setActiveBackendClient } from "@/lib/bridge/activeBackendClient";
import { normalizeDataStoreCandidate, normalizeDataStoreStartupView } from "@/lib/bridge/domainMapping";
import type { BackendClient } from "@/lib/bridge/BackendClient";
import type { DataStoreCandidate, DataStoreStartupView } from "@/lib/types/dataRecovery";

import {
  activateDataStoreCandidate,
  createNewDataStore,
  exportDataStoreDiagnostic,
  getDataStoreStartupState,
  locateDataStoreCandidate,
  openDataStoreBackupDir,
  refreshDataStoreCandidates,
  restartApp,
} from "./dataRecovery";

describe("data recovery backend cutover", () => {
  const dataRecovery = {
    getDataStoreStartupState: vi.fn(async () => startupView()),
    refreshDataStoreCandidates: vi.fn(async () => startupView()),
    locateDataStoreCandidate: vi.fn(async () => candidate()),
    activateDataStoreCandidate: vi.fn(async () => ({ restartRequired: true })),
    createNewDataStore: vi.fn(async () => ({ restartRequired: true })),
    restartApp: vi.fn(async () => undefined),
    openDataStoreBackupDir: vi.fn(async () => undefined),
    exportDataStoreDiagnostic: vi.fn(async () => "diagnostic.zip"),
  };

  beforeEach(() => {
    setActiveBackendClient({
      mode: "desktop",
      settings: {} as BackendClient["settings"],
      stations: {} as BackendClient["stations"],
      stationKeys: {} as BackendClient["stationKeys"],
    alerting: {} as BackendClient["alerting"],
      collectorRuns: {} as BackendClient["collectorRuns"],
      collectors: {} as BackendClient["collectors"],
      proxy: {} as BackendClient["proxy"],
      dashboard: {} as BackendClient["dashboard"],
      runtime: {} as BackendClient["runtime"],
      dataRecovery,
      dataMigration: {} as BackendClient["dataMigration"],
      economics: {} as BackendClient["economics"],
      groupFacts: {} as BackendClient["groupFacts"],
      pricing: {} as BackendClient["pricing"],
      routing: {} as BackendClient["routing"],
      channels: {} as BackendClient["channels"],
      updater: {} as BackendClient["updater"],
      handshake: vi.fn(async () => ({}) as never),
    });
    for (const fn of Object.values(dataRecovery)) {
      fn.mockClear();
    }
  });

  afterEach(() => {
    setActiveBackendClient(null);
  });

  it("routes startup and recovery commands through the active backend client", async () => {
    await getDataStoreStartupState();
    await refreshDataStoreCandidates();
    await locateDataStoreCandidate();
    await activateDataStoreCandidate("candidate-7");
    await createNewDataStore(true);
    await restartApp();
    await openDataStoreBackupDir();
    await exportDataStoreDiagnostic();

    expect(dataRecovery.getDataStoreStartupState).toHaveBeenCalledTimes(1);
    expect(dataRecovery.refreshDataStoreCandidates).toHaveBeenCalledTimes(1);
    expect(dataRecovery.locateDataStoreCandidate).toHaveBeenCalledTimes(1);
    expect(dataRecovery.activateDataStoreCandidate).toHaveBeenCalledWith("candidate-7");
    expect(dataRecovery.createNewDataStore).toHaveBeenCalledWith(true);
    expect(dataRecovery.restartApp).toHaveBeenCalledTimes(1);
    expect(dataRecovery.openDataStoreBackupDir).toHaveBeenCalledTimes(1);
    expect(dataRecovery.exportDataStoreDiagnostic).toHaveBeenCalledTimes(1);
  });

  it("fails closed when a stale or malformed startup DTO is returned", () => {
    expect(() => normalizeDataStoreStartupView({
      decision: { kind: "ready", candidateId: "legacy" },
      candidates: [],
    })).toThrow(/invalid data store startup response/i);
  });

  it("fails closed when manual location returns a malformed candidate", () => {
    expect(() => normalizeDataStoreCandidate({ id: "candidate-without-health" }))
      .toThrow(/invalid data store candidate response/i);
  });

  it("parses backend recovery evidence into a selectable candidate", async () => {
    const state = normalizeDataStoreStartupView({
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
      candidates: [candidate()],
    });
    const { buildRecoveryViewModel } = await import("@/features/data-recovery/recoveryViewModel");
    const viewModel = buildRecoveryViewModel(state);

    expect(viewModel.candidates[0]).toMatchObject({
      generationLabel: "Generation 2",
      selectable: true,
    });
  });
});

function startupView(): DataStoreStartupView {
  return {
    mode: "writable",
    databaseGeneration: "two",
    compatibility: {
      decisionCode: "writable",
      schemaVersion: null,
      appVersion: "0.3.2",
    },
    capabilities: {
      canBackup: false,
      canExportDiagnostic: false,
      canCheckForUpdates: false,
      canLocateCandidate: false,
      canActivateCandidate: false,
      canCreateDataStore: false,
    },
    decision: { kind: "ready", candidateId: "active" },
    candidates: [],
  };
}

function candidate(): DataStoreCandidate {
  return {
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
  };
}
