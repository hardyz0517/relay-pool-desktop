import type { QueryClient } from "@tanstack/react-query";
import type { Event } from "@tauri-apps/api/event";
import { describe, expect, it, vi } from "vitest";
import {
  DOMAIN_REVISION_UPDATED_EVENT,
  normalizeDomainRevisionNotice,
  queryKeysForDomainRevisionScopes,
  reconcileStationDetailReadModel,
  reconcileStationReadModels,
  subscribeToDomainRevisionUpdates,
  synchronizeStationReadModels,
} from "./stationCollectionQuerySynchronization";

function queryClient(invalidateQueries: ReturnType<typeof vi.fn>): QueryClient {
  return { invalidateQueries } as unknown as QueryClient;
}

describe("station collection query synchronization", () => {
  it("normalizes the generated camelCase event payload", () => {
    expect(
      normalizeDomainRevisionNotice({
        mutationId: "mutation-1",
        affectedScopes: ["station_collection:s1"],
        revisionVector: [{ scope: "station_collection:s1", revision: 3 }],
      }),
    ).toEqual({
      mutationId: "mutation-1",
      affectedScopes: ["station_collection:s1"],
      revisionVector: [{ scope: "station_collection:s1", revision: 3 }],
    });
    expect(normalizeDomainRevisionNotice({ mutationId: "x", affectedScopes: ["bad"], extra: "ignored" })).toEqual({
      mutationId: "x",
      affectedScopes: ["bad"],
      revisionVector: [],
    });
    expect(normalizeDomainRevisionNotice({ affectedScopes: ["station_collection:s1"] })).toBeNull();
  });

  it("maps station and workspace scopes to stable query families", () => {
    expect(queryKeysForDomainRevisionScopes([
      "station_collection:s1",
      "station_authorization:s1",
      "read_model:station_assets",
    ])).toEqual([
      ["stations"],
      ["stationAssets"],
      ["routing"],
      ["stationDetail", "s1"],
      ["balanceSnapshots"],
      ["keyPool"],
      ["pricing"],
      ["stationPublishedStatus"],
      ["collectorSnapshots", "s1"],
      ["collectorRuns", "s1"],
      ["captureSessionStatus", "s1"],
    ]);
  });

  it("ignores duplicate and out-of-order revisions", async () => {
    const invalidateQueries = vi.fn().mockResolvedValue(undefined);
    const client = queryClient(invalidateQueries);
    const tracker = new Map<string, number>();
    const notice = { mutationId: "m1", affectedScopes: [], revisionVector: [{ scope: "station_collection:s1", revision: 2 }] };
    const first = await synchronizeStationReadModels(client, notice, tracker);
    const duplicate = await synchronizeStationReadModels(client, { ...notice, mutationId: "m2" }, tracker);
    const older = await synchronizeStationReadModels(client, {
      mutationId: "m3",
      affectedScopes: [],
      revisionVector: [{ scope: "station_collection:s1", revision: 1 }],
    }, tracker);

    expect(first.invalidatedKeys).toHaveLength(10);
    expect(duplicate.invalidatedKeys).toEqual([]);
    expect(older.invalidatedKeys).toEqual([]);
    expect(older.ignoredScopes).toEqual(["station_collection:s1"]);
    expect(invalidateQueries).toHaveBeenCalledTimes(10);
  });

  it("reconciles the station workspace through the canonical scope mapping", async () => {
    const invalidateQueries = vi.fn()
      .mockResolvedValueOnce(undefined)
      .mockRejectedValueOnce(new Error("refresh failed"));

    const tracker = new Map<string, number>();
    const probe = vi.fn().mockResolvedValue({ scope: "read_model:station_assets", revision: 9 });
    const result = await reconcileStationReadModels(queryClient(invalidateQueries), tracker, probe);

    expect(invalidateQueries.mock.calls).toEqual([
      [{ queryKey: ["stations"] }],
      [{ queryKey: ["stationAssets"] }],
      [{ queryKey: ["routing"] }],
    ]);
    expect(result.refreshed).toBe(false);
    expect(result.invalidatedKeys).toEqual([["stations"], ["stationAssets"], ["routing"]]);
    expect(result.errors).toHaveLength(1);
    expect(tracker.size).toBe(0);
  });

  it("uses the durable revision probe to skip unchanged work", async () => {
    const invalidateQueries = vi.fn().mockResolvedValue(undefined);
    const tracker = new Map([["read_model:station_assets", 9]]);
    const result = await reconcileStationReadModels(
      queryClient(invalidateQueries),
      tracker,
      async () => ({ scope: "read_model:station_assets", revision: 9 }),
    );

    expect(result.invalidatedKeys).toEqual([]);
    expect(result.ignoredScopes).toEqual(["read_model:station_assets"]);
    expect(invalidateQueries).not.toHaveBeenCalled();
  });

  it("reconciles a station-scoped detail revision without using the asset revision", async () => {
    const invalidateQueries = vi.fn().mockResolvedValue(undefined);
    const tracker = new Map<string, number>();
    const probe = vi.fn().mockResolvedValue({
      scope: "read_model:station_detail:s1",
      revision: 12,
    });

    const result = await reconcileStationDetailReadModel(
      queryClient(invalidateQueries),
      "s1",
      tracker,
      probe,
    );

    expect(probe).toHaveBeenCalledWith("s1");
    expect(result.invalidatedKeys).toEqual([["stationDetail", "s1"]]);
    expect(tracker.get("read_model:station_detail:s1")).toBe(12);
  });

  it("rejects a mismatched station detail revision scope", async () => {
    const invalidateQueries = vi.fn().mockResolvedValue(undefined);
    const result = await reconcileStationDetailReadModel(
      queryClient(invalidateQueries),
      "s1",
      new Map(),
      async () => ({ scope: "read_model:station_detail:s2", revision: 2 }),
    );

    expect(result.refreshed).toBe(false);
    expect(result.errors).toHaveLength(1);
    expect(invalidateQueries).not.toHaveBeenCalled();
  });

  it("subscribes to the versioned event and handles typed payloads", async () => {
    const invalidateQueries = vi.fn().mockResolvedValue(undefined);
    const tracker = new Map<string, number>();
    const unlisten = vi.fn();
    let handler: ((event: Event<unknown>) => void) | undefined;
    const subscribe = vi.fn(async (event: string, nextHandler: (event: Event<unknown>) => void) => {
      expect(event).toBe(DOMAIN_REVISION_UPDATED_EVENT);
      handler = nextHandler;
      return unlisten;
    });

    const returned = await subscribeToDomainRevisionUpdates(queryClient(invalidateQueries), subscribe, tracker);
    handler?.({ event: DOMAIN_REVISION_UPDATED_EVENT, id: 1, payload: {
      mutationId: "m1",
      affectedScopes: ["station_authorization:s1"],
      revisionVector: [{ scope: "station_authorization:s1", revision: 7 }],
    } });

    await vi.waitFor(() => expect(invalidateQueries).toHaveBeenCalledTimes(5));
    expect(returned).toBe(unlisten);
    await vi.waitFor(() => expect(tracker.get("station_authorization:s1")).toBe(7));
  });
});
