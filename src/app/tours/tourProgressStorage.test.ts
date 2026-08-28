import { describe, expect, it } from "vitest";
import {
  MAX_TOUR_PROGRESS_PAYLOAD_LENGTH,
  TOUR_PROGRESS_STORAGE_KEY,
  createTourProgressStore,
  parseTourProgress,
  readTourProgress,
  resetTourProgress,
  writeTourProgress,
} from "./tourProgressStorage";

function fakeStorage(initial?: string) {
  let value = initial ?? null;
  let key: string | null = null;
  return {
    getItem: () => value,
    setItem: (_key: string, next: string) => {
      key = _key;
      value = next;
    },
    removeItem: () => {
      value = null;
    },
    read: () => value,
    readKey: () => key,
  };
}

describe("tourProgressStorage", () => {
  it("reads and writes a valid v1 payload", () => {
    const storage = fakeStorage();
    const progress = {
      schemaVersion: 1 as const,
      tours: { basic: { revision: 1, state: "completed" as const, updatedAt: 123 } },
    };
    expect(writeTourProgress(progress, storage)).toBe(true);
    expect(storage.readKey()).toBe(TOUR_PROGRESS_STORAGE_KEY);
    expect(readTourProgress(storage)).toEqual(progress);
  });

  it("returns empty progress for missing, old, malformed, oversized, or invalid data", () => {
    const empty = { schemaVersion: 1, tours: {} };
    expect(readTourProgress(fakeStorage())).toEqual(empty);
    expect(parseTourProgress({ schemaVersion: 0, tours: {} })).toEqual(empty);
    expect(readTourProgress(fakeStorage("not-json"))).toEqual(empty);
    expect(readTourProgress(fakeStorage("x".repeat(MAX_TOUR_PROGRESS_PAYLOAD_LENGTH + 1)))).toEqual(empty);
    expect(parseTourProgress({ schemaVersion: 1, tours: { basic: { revision: 0, state: "completed", updatedAt: 1 } } })).toEqual(empty);
  });

  it("ignores unknown ids and fields while retaining valid known entries", () => {
    expect(
      parseTourProgress({
        schemaVersion: 1,
        extra: true,
        tours: {
          basic: { revision: 2, state: "skipped", updatedAt: 5, extra: "ignored" },
          unknown: { revision: 1, state: "completed", updatedAt: 5 },
        },
      }),
    ).toEqual({
      schemaVersion: 1,
      tours: { basic: { revision: 2, state: "skipped", updatedAt: 5 } },
    });
  });

  it("parses current page tours and retains unpublished legacy progress", () => {
    expect(
      parseTourProgress({
        schemaVersion: 1,
        tours: {
          dashboard: { revision: 1, state: "completed", updatedAt: 5 },
          proxy: { revision: 1, state: "skipped", updatedAt: 6 },
          "station-setup": { revision: 1, state: "completed", updatedAt: 7 },
        },
      }).tours,
    ).toEqual({
      dashboard: { revision: 1, state: "completed", updatedAt: 5 },
      proxy: { revision: 1, state: "skipped", updatedAt: 6 },
      "station-setup": { revision: 1, state: "completed", updatedAt: 7 },
    });
  });

  it("handles storage and quota failures without throwing", () => {
    const broken = {
      getItem: () => {
        throw new Error("blocked");
      },
      setItem: () => {
        throw new Error("quota");
      },
      removeItem: () => {
        throw new Error("blocked");
      },
    };
    expect(readTourProgress(broken)).toEqual({ schemaVersion: 1, tours: {} });
    expect(writeTourProgress({ schemaVersion: 1, tours: {} }, broken)).toBe(false);
    expect(resetTourProgress(undefined, broken)).toBe(false);
  });

  it("keeps skipped status for the current revision while allowing manual restart", () => {
    const storage = fakeStorage();
    const store = createTourProgressStore(storage, () => 1000);
    expect(store.commitSkipped("basic", 1)).toBe(true);
    expect(store.getSnapshot().tours.basic).toEqual({ revision: 1, state: "skipped", updatedAt: 1000 });
    expect(readTourProgress(storage).tours.basic?.state).toBe("skipped");
    // Starting is a Manager concern; storage does not reject a second manual run.
    expect(store.commitCompletion("basic", 1, 1001)).toBe(true);
    expect(store.getSnapshot().tours.basic?.state).toBe("completed");
  });

  it("resets one tour or all tours", () => {
    const storage = fakeStorage();
    const store = createTourProgressStore(storage, () => 1);
    store.commitCompletion("basic", 1);
    store.commitSkipped("proxy", 1);
    expect(store.reset("basic")).toBe(true);
    expect(store.getSnapshot().tours.basic).toBeUndefined();
    expect(store.getSnapshot().tours.proxy).toBeDefined();
    expect(store.reset()).toBe(true);
    expect(store.getSnapshot()).toEqual({ schemaVersion: 1, tours: {} });
  });
});
