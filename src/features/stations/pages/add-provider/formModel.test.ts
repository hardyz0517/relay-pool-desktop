import { describe, expect, it } from "vitest";
import {
  createDefaultProviderForm,
  draftRemoteCapability,
  formFromStation,
  serializeProviderDraft,
} from "./formModel";
import type { StationGroupDraft } from "../../components/StationGroupRowsEditor";
import type { StationKeyDraft } from "../../components/StationKeyRowsEditor";
import type { StationCredentials } from "@/lib/types/stationKeys";
import type { Station } from "@/lib/types/stations";

function station(overrides: Partial<Station> = {}): Station {
  return {
    id: "station-1",
    name: "Relay",
    stationType: "sub2api",
    websiteUrl: "https://console.example",
    apiBaseUrl: "https://api.example/v1",
    endpointRevision: 3,
    collectorProxyMode: "manual",
    collectorProxyUrl: "http://127.0.0.1:7890",
    apiKeyMasked: "sk-***",
    apiKeyPresent: true,
    keyCount: 1,
    enabled: true,
    priority: 0,
    creditPerCny: 7.2,
    balanceRaw: null,
    balanceCny: null,
    lowBalanceThresholdCny: null,
    collectionIntervalMinutes: 15,
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

function credentials(overrides: Partial<StationCredentials> = {}): StationCredentials {
  return {
    stationId: "station-1",
    loginUsername: "operator",
    passwordPresent: true,
    rememberPassword: true,
    loginStatus: "authenticated",
    loginError: null,
    lastLoginAt: null,
    sessionStatus: "unknown",
    sessionExpiresAt: null,
    accessTokenPresent: false,
    refreshTokenPresent: false,
    cookiePresent: false,
    sessionSource: null,
    newapiUserId: null,
    tokenExpiresAt: null,
    tokenRefreshedAt: null,
    updatedAt: null,
    ...overrides,
  };
}

describe("add provider form model", () => {
  it("creates the default create-page form without transport state", () => {
    expect(createDefaultProviderForm()).toMatchObject({
      apiKey: "",
      collectorProxyMode: "inherit",
      collectorProxyUrl: "",
      creditPerCny: "1",
      enabled: true,
      collectionIntervalMinutes: "5",
    });
  });

  it("keeps dirty snapshots stable when draft client ids change", () => {
    const form = createDefaultProviderForm();
    const groupRow: StationGroupDraft = {
      clientId: "volatile-group-a",
      groupBindingId: null,
      groupKeyHash: "manual:test",
      groupIdHash: null,
      groupName: "default",
      rateMultiplier: "1",
      inferredGroupCategory: "unknown",
      groupCategoryOverride: null,
      source: "manual",
      deleteRequested: false,
    };
    const keyRow: StationKeyDraft = {
      clientId: "volatile-key-a",
      id: null,
      name: "default",
      apiKey: "sk-test",
      groupBindingId: null,
      groupIdHash: null,
      groupName: "default",
      rateMultiplier: "1",
      enabled: true,
      note: "",
      deleteRequested: false,
    };

    const changedGroupId = { ...groupRow, clientId: "volatile-group-b" };
    const changedKeyId = { ...keyRow, clientId: "volatile-key-b" };

    expect(serializeProviderDraft(form, [groupRow], [keyRow])).toEqual(
      serializeProviderDraft(form, [changedGroupId], [changedKeyId]),
    );
  });

  it("hydrates edit-page form state from station and credentials", () => {
    expect(
      formFromStation(
        station({
          note: "saved note",
        }),
        credentials({ loginUsername: "alice", rememberPassword: false }),
      ),
    ).toMatchObject({
      name: "Relay",
      apiKey: "",
      loginUsername: "alice",
      rememberPassword: false,
      note: "saved note",
      collectorProxyMode: "manual",
      collectorProxyUrl: "http://127.0.0.1:7890",
    });
  });

  it("drafts remote-key capability for supported station types", () => {
    expect(draftRemoteCapability("sub2api")).toMatchObject({
      canListRemoteKeys: true,
      canCreateRemoteKey: false,
      canDeleteRemoteKeys: false,
      canReadGroups: true,
    });
    expect(draftRemoteCapability("newapi")).toMatchObject({
      canListRemoteKeys: true,
      canCreateRemoteKey: false,
      canDeleteRemoteKeys: false,
      canReadGroups: true,
    });
  });
});
