import { beforeEach, describe, expect, it, vi } from "vitest";

const generated = vi.hoisted(() => ({
  clearChangeEvents: vi.fn(),
  dismissChangeEvent: vi.fn(),
  listChangeEvents: vi.fn(),
  listChangeEventsForStation: vi.fn(),
  markChangeEventRead: vi.fn(),
  markChangeEventsRead: vi.fn(),
  resolveChangeEvent: vi.fn(),
  upsertChangeEvent: vi.fn(),
}));

vi.mock("@/lib/bridge/generated", () => generated);

import {
  clearChangeEvents,
  dismissChangeEvent,
  listChangeEvents,
  listChangeEventsForStation,
  markChangeEventRead,
  markChangeEventsRead,
  resolveChangeEvent,
  upsertChangeEvent,
} from "./changeEvents";

describe("change event generated transport cutover", () => {
  beforeEach(() => {
    for (const fn of Object.values(generated)) fn.mockReset().mockResolvedValue(undefined);
  });

  it("routes all eight commands through generated wrappers", async () => {
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

    expect(generated.listChangeEvents).toHaveBeenCalledWith();
    expect(generated.clearChangeEvents).toHaveBeenCalledWith();
    expect(generated.listChangeEventsForStation).toHaveBeenCalledWith({ stationId: "station-1" });
    expect(generated.upsertChangeEvent).toHaveBeenCalledWith(input);
    expect(generated.markChangeEventsRead).toHaveBeenCalledWith({ ids: ["change-1", "change-2"] });
  });
});
