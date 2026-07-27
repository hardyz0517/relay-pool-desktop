import { describe, expect, it } from "vitest";
import type { StationGroupOption } from "@/lib/types/groupFacts";
import type { StationKeyCapabilities } from "@/lib/types/routing";
import type { Station } from "@/lib/types/stations";
import type { KeyPoolItem } from "@/lib/types/stationKeys";
import {
  CLEAR_GROUP_BINDING_VALUE,
  KEEP_GROUP_BINDING_VALUE,
  capabilitiesFromEditForm,
  createFormForStation,
  emptyEditForm,
  formFromItem,
  groupSelectionFromCreateForm,
  groupSelectionFromEditForm,
  mergeCapabilitiesIntoForm,
} from "./KeyPoolFormModel";

function groupOption(overrides: Partial<StationGroupOption> = {}): StationGroupOption {
  return {
    value: "group-1",
    groupBindingId: "binding-1",
    groupIdHash: "hash-1",
    groupName: "default",
    rateMultiplier: 1,
    inferredGroupCategory: null,
    groupCategoryOverride: null,
    effectiveGroupCategory: "gpt",
    rateSource: "test",
    selectableForRemoteKey: true,
    ...overrides,
  };
}

function keyPoolItem(overrides: Partial<KeyPoolItem> = {}): KeyPoolItem {
  return {
    id: "key-1",
    stationId: "station-1",
    name: "Primary",
    enabled: true,
    schedulable: true,
    priority: 2,
    groupBindingId: "binding-1",
    groupIdHash: "hash-1",
    groupName: "default",
    tierLabel: "tier",
    rateMultiplier: 1,
    status: "healthy",
    note: "note",
    stationName: "Relay",
    onlyUseAsBackup: false,
    ...overrides,
  } as KeyPoolItem;
}

function capabilities(overrides: Partial<StationKeyCapabilities> = {}): StationKeyCapabilities {
  return {
    stationKeyId: "key-1",
    supportsChatCompletions: true,
    supportsResponses: false,
    supportsEmbeddings: true,
    supportsStream: false,
    supportsTools: true,
    supportsVision: false,
    supportsReasoning: true,
    modelAllowlist: ["gpt-a"],
    modelBlocklist: ["gpt-b"],
    preferredModels: ["gpt-c"],
    onlyUseAsBackup: true,
    routingTags: ["fast", "cheap"],
    updatedAt: "2026-01-01T00:00:00Z",
    ...overrides,
  };
}

describe("KeyPoolFormModel", () => {
  it("builds create/edit group selections with explicit keep and clear semantics", () => {
    const option = groupOption();
    expect(groupSelectionFromCreateForm({ ...emptyEditForm, groupBindingId: "" }, [option])).toEqual({ kind: "clear" });
    expect(groupSelectionFromCreateForm({ ...emptyEditForm, groupBindingId: "binding-1" }, [option])).toEqual({
      kind: "set",
      groupBindingId: "binding-1",
      groupIdHash: "hash-1",
      groupName: "default",
    });

    const sourceItem = keyPoolItem();
    expect(groupSelectionFromEditForm({ ...emptyEditForm, groupBindingId: KEEP_GROUP_BINDING_VALUE }, sourceItem, [option])).toEqual({ kind: "keep" });
    expect(groupSelectionFromEditForm({ ...emptyEditForm, groupBindingId: CLEAR_GROUP_BINDING_VALUE }, sourceItem, [option])).toEqual({ kind: "clear" });
    expect(groupSelectionFromEditForm({ ...emptyEditForm, groupBindingId: "binding-2" }, sourceItem, [groupOption({ groupBindingId: "binding-2", groupIdHash: "hash-2", groupName: "paid" })])).toEqual({
      kind: "set",
      groupBindingId: "binding-2",
      groupIdHash: "hash-2",
      groupName: "paid",
    });
  });

  it("normalizes form capabilities and merges saved capabilities into forms", () => {
    const form = {
      ...emptyEditForm,
      id: "key-1",
      modelAllowlist: "gpt-a\n gpt-a \n\ngpt-b",
      modelBlocklist: "bad-a\nbad-b",
      preferredModels: "pref-a\npref-a",
      routingTags: "fast, fast, cheap",
    };

    expect(capabilitiesFromEditForm(form)).toMatchObject({
      stationKeyId: "key-1",
      modelAllowlist: ["gpt-a", "gpt-b"],
      modelBlocklist: ["bad-a", "bad-b"],
      preferredModels: ["pref-a"],
      routingTags: ["fast", "cheap"],
    });

    expect(mergeCapabilitiesIntoForm(emptyEditForm, capabilities())).toMatchObject({
      supportsResponses: false,
      supportsEmbeddings: true,
      modelAllowlist: "gpt-a",
      modelBlocklist: "gpt-b",
      preferredModels: "gpt-c",
      onlyUseAsBackup: true,
      routingTags: "fast, cheap",
    });
  });

  it("builds edit and create forms from page-owned items and stations", () => {
    expect(formFromItem(keyPoolItem(), [groupOption()])).toMatchObject({
      id: "key-1",
      groupBindingId: "binding-1",
      stationName: "Relay",
      priority: "2",
    });

    expect(createFormForStation({ id: "station-1", name: "Relay" } as Station, [keyPoolItem()])).toMatchObject({
      stationId: "station-1",
      stationName: "Relay",
      name: "Relay Key 2",
      priority: "1",
    });
  });
});
