// @vitest-environment jsdom

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { DemoBackend } from "@/lib/bridge/DemoBackend";
import { setActiveBackendClient } from "@/lib/bridge/activeBackendClient";
import { DemoBackendUnsupportedError, type BackendClient } from "@/lib/bridge/BackendClient";
import {
  clearChangeEvents,
  dismissChangeEvent,
  listChangeEvents,
  listChangeEventsForStation,
  resolveChangeEvent,
  upsertChangeEvent,
  markChangeEventRead,
  markChangeEventsRead,
} from "./changeEvents";
import { listCollectorRuns } from "./collectorRuns";

describe("change event backend cutover", () => {
  const backend = makeBackendClient();

  beforeEach(() => {
    setActiveBackendClient(backend.client);
    backend.reset();
  });

  afterEach(() => {
    setActiveBackendClient(null);
  });

  it("routes change-event commands through the active backend client", async () => {
    const input = {
      severity: "warning" as const,
      eventType: "fixture.changed",
      title: "Fixture",
      message: "Fixture",
      objectType: "station",
      objectId: null,
      stationId: null,
      stationKeyId: null,
      pricingRuleId: null,
      requestLogId: null,
      oldValueJson: null,
      newValueJson: null,
      impactJson: null,
      dedupeKey: "fixture",
      source: "fixture",
    };

    await listChangeEvents();
    await clearChangeEvents();
    await listChangeEventsForStation("station-1");
    await upsertChangeEvent(input);
    await markChangeEventRead("change-1");
    await markChangeEventsRead(["change-1", "change-1", "change-2"]);
    await dismissChangeEvent("change-1");
    await resolveChangeEvent("change-1");

    expect(backend.changeEvents.listChangeEvents).toHaveBeenCalledTimes(1);
    expect(backend.changeEvents.clearChangeEvents).toHaveBeenCalledTimes(1);
    expect(backend.changeEvents.listChangeEventsForStation).toHaveBeenCalledWith("station-1");
    expect(backend.changeEvents.upsertChangeEvent).toHaveBeenCalledWith(input);
    expect(backend.changeEvents.markChangeEventRead).toHaveBeenCalledWith("change-1");
    expect(backend.changeEvents.markChangeEventsRead).toHaveBeenCalledWith(["change-1", "change-2"]);
    expect(backend.changeEvents.dismissChangeEvent).toHaveBeenCalledWith("change-1");
    expect(backend.changeEvents.resolveChangeEvent).toHaveBeenCalledWith("change-1");
  });

  it("routes collector-run reads through the active backend client", async () => {
    await listCollectorRuns("station-1");

    expect(backend.collectorRuns.listCollectorRuns).toHaveBeenCalledWith("station-1");
  });

  it("does not fake change-log success in demo mode", async () => {
    setActiveBackendClient(new DemoBackend());

    await expect(listChangeEvents()).rejects.toBeInstanceOf(DemoBackendUnsupportedError);
    await expect(listCollectorRuns("station-1")).rejects.toBeInstanceOf(DemoBackendUnsupportedError);
  });

});

function makeBackendClient() {
  const changeEvents = {
    listChangeEvents: vi.fn(async () => []),
    clearChangeEvents: vi.fn(async () => undefined),
    listChangeEventsForStation: vi.fn(async () => []),
    upsertChangeEvent: vi.fn(async (input) => ({ id: "change-1", ...input } as never)),
    markChangeEventRead: vi.fn(async (id) => ({ id } as never)),
    markChangeEventsRead: vi.fn(async (ids: string[]) => ids.map((id: string) => ({ id } as never))),
    dismissChangeEvent: vi.fn(async (id) => ({ id } as never)),
    resolveChangeEvent: vi.fn(async (id) => ({ id } as never)),
  };

  const collectorRuns = {
    listCollectorRuns: vi.fn(async () => []),
  };

  return {
    client: {
      mode: "desktop" as const,
      settings: {} as BackendClient["settings"],
      stations: {} as BackendClient["stations"],
      stationKeys: {} as BackendClient["stationKeys"],
      changeEvents,
      collectorRuns,
      collectors: {} as BackendClient["collectors"],
      proxy: {} as BackendClient["proxy"],
      dashboard: {} as BackendClient["dashboard"],
      runtime: {} as BackendClient["runtime"],
      localRouting: {} as BackendClient["localRouting"],
      dataRecovery: {} as BackendClient["dataRecovery"],
      dataMigration: {} as BackendClient["dataMigration"],
      economics: {} as BackendClient["economics"],
      groupFacts: {} as BackendClient["groupFacts"],
      pricing: {} as BackendClient["pricing"],
      routing: {} as BackendClient["routing"],
      channels: {} as BackendClient["channels"],
      updater: {} as BackendClient["updater"],
      handshake: vi.fn(async () => ({}) as never),
    } satisfies BackendClient,
    reset() {
      for (const fn of Object.values(changeEvents)) {
        fn.mockClear();
      }
      for (const fn of Object.values(collectorRuns)) {
        fn.mockClear();
      }
    },
    changeEvents,
    collectorRuns,
  };
}
