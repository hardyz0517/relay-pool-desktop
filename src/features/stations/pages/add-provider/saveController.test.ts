import { describe, expect, it } from "vitest";
import type { StationGroupDraft } from "../../components/StationGroupRowsEditor";
import type { StationKeyDraft } from "../../components/StationKeyRowsEditor";
import type { StationGroupBinding, UpsertStationGroupBindingInput } from "@/lib/types/groupFacts";
import type { CreateStationKeyInput, UpdateStationKeyInput } from "@/lib/types/stationKeys";
import {
  saveGroupRows,
  saveKeyRows,
  type SaveGroupRowsDependencies,
  type SaveKeyRowsDependencies,
} from "./saveController";

function keyDraft(overrides: Partial<StationKeyDraft> = {}): StationKeyDraft {
  return {
    clientId: "key-draft",
    id: null,
    name: "Key",
    apiKey: "sk-test",
    groupBindingId: null,
    groupIdHash: null,
    groupName: "",
    rateMultiplier: "",
    enabled: true,
    note: "",
    deleteRequested: false,
    ...overrides,
  };
}

function groupDraft(overrides: Partial<StationGroupDraft> = {}): StationGroupDraft {
  return {
    clientId: "group-draft",
    groupBindingId: null,
    groupKeyHash: "",
    groupIdHash: null,
    groupName: "default",
    rateMultiplier: "1",
    inferredGroupCategory: "unknown",
    groupCategoryOverride: null,
    source: "manual",
    deleteRequested: false,
    ...overrides,
  };
}

function stationGroupBinding(overrides: Partial<StationGroupBinding> = {}): StationGroupBinding {
  return {
    id: "binding-1",
    stationId: "station-1",
    stationKeyId: null,
    bindingKind: "station_group",
    parentGroupBindingId: null,
    groupKeyHash: "manual:default",
    groupIdHash: null,
    groupName: "default",
    bindingStatus: "available",
    defaultRateMultiplier: null,
    userRateMultiplier: 1,
    effectiveRateMultiplier: 1,
    inferredGroupCategory: "unknown",
    groupCategoryOverride: null,
    rateSource: "manual",
    confidence: 1,
    lastSeenAt: null,
    lastCheckedAt: null,
    lastRateChangedAt: null,
    rawJsonRedacted: null,
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
    ...overrides,
  };
}

describe("add provider save controller", () => {
  it("deletes removed keys and saves visible rows with stable priorities", async () => {
    const created: CreateStationKeyInput[] = [];
    const updated: UpdateStationKeyInput[] = [];
    const deleted: string[] = [];
    const dependencies: SaveKeyRowsDependencies = {
      createStationKey: async (input) => {
        created.push(input);
        return { id: "created-key" };
      },
      updateStationKey: async (input) => {
        updated.push(input);
      },
      deleteStationKey: async (id) => {
        deleted.push(id);
      },
    };

    const createdStationKeyIds = await saveKeyRows(
      "station-1",
      [
        keyDraft({ id: "delete-me", deleteRequested: true }),
        keyDraft({
          id: "existing",
          name: " Existing ",
          apiKey: "",
          groupName: " paid ",
          rateMultiplier: "2",
          note: " keep ",
        }),
        keyDraft({ id: null, name: " New ", apiKey: " sk-new " }),
        keyDraft({ id: null, name: "", apiKey: "" }),
      ],
      dependencies,
    );

    expect(deleted).toEqual(["delete-me"]);
    expect(updated).toEqual([
      expect.objectContaining({
        id: "existing",
        stationId: "station-1",
        name: "Existing",
        apiKey: null,
        priority: 0,
        groupName: "paid",
        rateMultiplier: 2,
        rateSource: "manual",
        note: "keep",
        status: "unchecked",
      }),
    ]);
    expect(created).toEqual([
      expect.objectContaining({
        stationId: "station-1",
        name: "New",
        apiKey: "sk-new",
        priority: 1,
      }),
    ]);
    expect(createdStationKeyIds).toEqual(["created-key"]);
  });

  it("disables matching saved groups and upserts editable group rows", async () => {
    const upserts: UpsertStationGroupBindingInput[] = [];
    const dependencies: SaveGroupRowsDependencies = {
      listStationGroupBindings: async () => [stationGroupBinding()],
      upsertStationGroupBinding: async (input) => {
        upserts.push(input);
        return stationGroupBinding({
          id: input.bindingStatus === "disabled" ? "disabled" : "saved",
          groupKeyHash: input.groupKeyHash,
          groupIdHash: input.groupIdHash,
          groupName: input.groupName,
          bindingStatus: input.bindingStatus,
          defaultRateMultiplier: input.defaultRateMultiplier,
          userRateMultiplier: input.userRateMultiplier,
          effectiveRateMultiplier: input.effectiveRateMultiplier,
          rateSource: input.rateSource,
          confidence: input.confidence,
          lastSeenAt: input.lastSeenAt,
        });
      },
      nowIso: () => "2026-01-02T00:00:00Z",
    };

    const savedOptions = await saveGroupRows(
      "station-1",
      [
        groupDraft({ groupBindingId: "binding-1", deleteRequested: true }),
        groupDraft({
          groupName: " remote group ",
          groupIdHash: "remote-hash",
          rateMultiplier: "3",
          source: "remote",
        }),
      ],
      1,
      dependencies,
    );

    expect(upserts).toEqual([
      expect.objectContaining({
        bindingStatus: "disabled",
        groupName: "default",
        effectiveRateMultiplier: null,
      }),
      expect.objectContaining({
        bindingStatus: "available",
        groupKeyHash: "remote:remote-hash",
        groupIdHash: "remote-hash",
        groupName: "remote group",
        defaultRateMultiplier: 3,
        userRateMultiplier: null,
        effectiveRateMultiplier: 3,
        rateSource: "remote_scan",
        confidence: 0.95,
        lastSeenAt: "2026-01-02T00:00:00Z",
      }),
    ]);
    expect(savedOptions).toHaveLength(1);
    expect(savedOptions[0]).toMatchObject({
      groupBindingId: "saved",
      groupIdHash: "remote-hash",
      groupName: "remote group",
      rateMultiplier: 3,
    });
  });
});
