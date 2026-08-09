import { describe, expect, it } from "vitest";
import type { StationKey } from "@/lib/types/stationKeys";
import type { Station } from "@/lib/types/stations";
import {
  emptyForm,
  emptyKeyForm,
  formToInput,
  keyToForm,
  normalizeCollectionIntervalMinutes,
  stationEndpointOriginWarnings,
  toCreateKeyInput,
  toUpdateKeyInput,
} from "./formModel";

function station(overrides: Partial<Station> = {}): Station {
  return {
    id: "station-1",
    name: "Relay",
    stationType: "sub2api",
    websiteUrl: "https://console.example",
    apiBaseUrl: "https://api.example/v1",
    endpointRevision: 1,
    collectorProxyMode: "inherit",
    collectorProxyUrl: null,
    apiKeyMasked: "sk-***",
    apiKeyPresent: true,
    keyCount: 1,
    enabled: true,
    priority: 0,
    creditPerCny: 1,
    balanceRaw: null,
    balanceCny: null,
    lowBalanceThresholdCny: null,
    collectionIntervalMinutes: 5,
    status: "healthy",
    latencyMs: null,
    lastCheckedAt: null,
    lastPricingFetchedAt: null,
    note: null,
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
    ...overrides,
  };
}

function stationKey(overrides: Partial<StationKey> = {}): StationKey {
  return {
    id: "key-1",
    stationId: "station-1",
    name: "Primary",
    apiKeyMasked: "sk-***",
    apiKeyPresent: true,
    enabled: true,
    priority: 2,
    maxConcurrency: 4,
    loadFactor: null,
    schedulable: true,
    groupBindingId: null,
    groupIdHash: null,
    groupName: "default",
    tierLabel: "paid",
    rateMultiplier: null,
    manualRateMultiplier: null,
    manualRateUpdatedAt: null,
    rateSource: null,
    rateCollectedAt: null,
    balanceScope: null,
    status: "healthy",
    lastCheckedAt: null,
    lastUsedAt: null,
    note: "keep",
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
    ...overrides,
  };
}

describe("stations page form model", () => {
  it("serializes station form state with existing defaults", () => {
    expect(
      formToInput({
        ...emptyForm,
        name: " Relay ",
        websiteUrl: " https://console.example ",
        apiBaseUrl: " https://api.example/v1 ",
        apiKey: " sk-test ",
        creditPerCny: "7.2",
        collectionIntervalMinutes: "0",
        note: " hello ",
      }),
    ).toMatchObject({
      name: "Relay",
      apiKey: "sk-test",
      collectorProxyMode: "inherit",
      collectorProxyUrl: null,
      creditPerCny: 7.2,
      lowBalanceThresholdCny: null,
      collectionIntervalMinutes: 5,
      note: "hello",
    });
  });

  it("serializes key create/update forms and edit hydration", () => {
    const createInput = toCreateKeyInput(
      {
        ...emptyKeyForm,
        name: " 密钥 ",
        apiKey: " sk-new ",
        priority: "3",
        groupName: " default ",
        tierLabel: " paid ",
        note: " note ",
      },
      "station-1",
    );
    expect(createInput).toMatchObject({
      stationId: "station-1",
      name: "密钥",
      apiKey: "sk-new",
      priority: 3,
      groupName: "default",
      tierLabel: "paid",
      note: "note",
    });

    expect(toUpdateKeyInput({ ...emptyKeyForm, id: "key-1", status: "healthy" }, "station-1")).toMatchObject({
      id: "key-1",
      apiKey: null,
      status: "healthy",
    });
    expect(keyToForm(stationKey())).toMatchObject({
      id: "key-1",
      apiKey: "",
      priority: "2",
      groupName: "default",
      tierLabel: "paid",
      status: "healthy",
    });
  });

  it("normalizes collection interval and detects endpoint origin warnings", () => {
    expect(normalizeCollectionIntervalMinutes("15")).toBe(15);
    expect(normalizeCollectionIntervalMinutes("-1")).toBe(5);

    expect(
      stationEndpointOriginWarnings(
        station(),
        {
          ...emptyForm,
          websiteUrl: "https://other.example",
          apiBaseUrl: "https://api.other.example/v1",
        },
      ),
    ).toEqual([
      "前端网址 origin 变化后，保存的登录状态会被清除。",
      "API origin 变化后，站点会被禁用，现有密钥将不会路由，直到重新验证并启用。",
    ]);
  });
});
