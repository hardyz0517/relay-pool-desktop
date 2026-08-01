import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const generated = vi.hoisted(() => ({
  getLocalAccessKey: vi.fn(),
  getSettings: vi.fn(),
  importRelayPoolToCcswitch: vi.fn(),
  chooseDataDir: vi.fn(),
  confirmHierarchicalRoutingMigration: vi.fn(),
  resetDataDir: vi.fn(),
  updateLocalAccessKey: vi.fn(),
  updateSettings: vi.fn(),
}));
const transport = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock("@/lib/bridge/generated", () => generated);
vi.mock("@/lib/bridge/transport", () => transport);

import { setActiveBackendClient } from "@/lib/bridge/activeBackendClient";
import { DesktopBackend } from "@/lib/bridge/DesktopBackend";
import {
  getLocalAccessKey,
  chooseDataDir,
  importRelayPoolToCCSwitch,
  resetDataDir,
  confirmHierarchicalRoutingMigration,
  updateLocalAccessKey,
} from "./settings";

describe("settings bootstrap generated transport cutover", () => {
  beforeEach(() => {
    setActiveBackendClient(new DesktopBackend());
    generated.getLocalAccessKey.mockReset().mockResolvedValue("sk-local-fixture");
    generated.updateLocalAccessKey.mockReset().mockResolvedValue(fixtureSettings());
    generated.importRelayPoolToCcswitch.mockReset().mockResolvedValue({
      app: "codex",
      providerName: "Relay Pool Desktop",
      endpoint: "http://127.0.0.1:8787/v1",
    });
    generated.chooseDataDir.mockReset().mockResolvedValue(fixtureSettings());
    generated.confirmHierarchicalRoutingMigration.mockReset().mockResolvedValue(fixtureSettings());
    generated.resetDataDir.mockReset().mockResolvedValue(fixtureSettings());
    transport.invoke.mockReset().mockRejectedValue(new Error("legacy transport invoked"));
  });

  afterEach(() => {
    setActiveBackendClient(null);
  });

  it("routes local access key reads and writes through generated wrappers", async () => {
    await expect(getLocalAccessKey()).resolves.toBe("sk-local-fixture");
    await updateLocalAccessKey("sk-local-updated");

    expect(generated.getLocalAccessKey).toHaveBeenCalledWith();
    expect(generated.updateLocalAccessKey).toHaveBeenCalledWith({ value: "sk-local-updated" });
    expect(transport.invoke).not.toHaveBeenCalled();
  });

  it("routes CCSwitch import through the generated non-idempotent wrapper", async () => {
    await expect(importRelayPoolToCCSwitch()).resolves.toMatchObject({
      app: "codex",
      providerName: "Relay Pool Desktop",
    });

    expect(generated.importRelayPoolToCcswitch).toHaveBeenCalledWith();
    expect(transport.invoke).not.toHaveBeenCalled();
  });

  it("routes data-directory selection through generated wrappers", async () => {
    await chooseDataDir();
    await resetDataDir();

    expect(generated.chooseDataDir).toHaveBeenCalledWith();
    expect(generated.resetDataDir).toHaveBeenCalledWith();
    expect(transport.invoke).not.toHaveBeenCalled();
  });

  it("routes hierarchical routing migration through the generated wrapper", async () => {
    await confirmHierarchicalRoutingMigration({
      orderingProfile: "cost_first",
      multiplierCeiling: 2,
      groupScope: "all_groups",
      allowDepletedFallback: false,
      affinityMode: "session",
      legacyPolicy: "cost_stable_first",
    });

    expect(generated.confirmHierarchicalRoutingMigration).toHaveBeenCalledWith({
      orderingProfile: "cost_first",
      multiplierCeiling: 2,
      groupScope: "all_groups",
      allowDepletedFallback: false,
      affinityMode: "session",
      legacyPolicy: "cost_stable_first",
    });
    expect(transport.invoke).not.toHaveBeenCalled();
  });
});

function fixtureSettings() {
  return {
    localProxyPort: 8787,
    localProxyStartOnLaunch: false,
    localKeyMasked: "sk-l****ture",
    defaultRoutingStrategy: "automatic_balanced",
    collectorProxyMode: "direct",
    collectorProxyUrl: null,
    maxRateMultiplier: null,
    defaultRoutingGroupFilter: "all_groups",
    schedulerAdvancedSettings: {},
    lowBalanceThresholdCny: 15,
    collectorIntervalMinutes: 30,
    balanceIntervalMinutes: 5,
    groupRateIntervalMinutes: 20,
    modelListIntervalMinutes: 60,
    pricingRefreshIntervalMinutes: 60,
    collectorTimeoutSeconds: 15,
    collectorMaxConcurrency: 3,
    allowDepletedFallback: false,
    hierarchicalRoutingMigration: null,
    developerModeEnabled: false,
    trayBehavior: "close_to_tray",
    dataDir: "fixture",
    pendingDataDir: null,
    dataDirChangeRequiresRestart: false,
  };
}
