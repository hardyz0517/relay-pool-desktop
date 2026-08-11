import { describe, expect, it } from "vitest";
import type { CollectorRunResult } from "@/lib/types/collector";
import { remoteKeyRefreshFailure } from "./collectorEvents";

function result(events: CollectorRunResult["events"]): CollectorRunResult {
  return {
    snapshot: {
      id: "snapshot-1",
      stationId: "station-1",
      endpointRevision: 1,
      source: "fixture",
      status: "success",
      fetchedAt: "1700000000000",
      summaryJson: {},
      normalizedJson: {},
      rawJsonRedacted: null,
      errorMessage: null,
      createdAt: "1700000000000",
    },
    events,
  };
}

describe("collector events", () => {
  it("returns a failed remote key refresh event", () => {
    expect(
      remoteKeyRefreshFailure(
        result([
          { eventType: "groups", message: "ok", status: "success" },
          { eventType: "remote_keys", message: "unavailable", status: "failed" },
        ]),
      ),
    ).toEqual({ eventType: "remote_keys", message: "unavailable", status: "failed" });
  });

  it("ignores successful and unrelated events", () => {
    expect(
      remoteKeyRefreshFailure(
        result([
          { eventType: "groups", message: "ok", status: "success" },
          { eventType: "remote_keys", message: "updated", status: "success" },
        ]),
      ),
    ).toBeNull();
  });
});
