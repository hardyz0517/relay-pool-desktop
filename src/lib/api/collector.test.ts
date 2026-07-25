import { beforeEach, describe, expect, it, vi } from "vitest";

const generated = vi.hoisted(() => ({
  clearCaptureSession: vi.fn(),
  collectStationInfo: vi.fn(),
  collectStationTask: vi.fn(),
  collectSub2apiStation: vi.fn(),
  closeCaptureSession: vi.fn(),
  detectStationInfo: vi.fn(),
  detectSub2apiStation: vi.fn(),
  finishCaptureSession: vi.fn(),
  finishWebAuthorizationSession: vi.fn(),
  getCaptureSessionStatus: vi.fn(),
  getLatestCollectorSnapshot: vi.fn(),
  listCollectorSnapshots: vi.fn(),
  startCaptureSession: vi.fn(),
  testStationLogin: vi.fn(),
  testStationLoginInput: vi.fn(),
}));

const transport = vi.hoisted(() => ({
  invoke: vi.fn(),
}));

vi.mock("@/lib/bridge/generated", () => generated);
vi.mock("@/lib/bridge/transport", () => transport);

import {
  clearCaptureSession,
  closeCaptureSession,
  finishCaptureSession,
  finishWebAuthorizationSession,
  getCaptureSessionStatus,
  startCaptureSession,
} from "./collector";

describe("collector capture generated transport cutover", () => {
  beforeEach(() => {
    for (const fn of Object.values(generated)) {
      fn.mockReset().mockResolvedValue(fixtureCaptureStatus());
    }
    generated.finishCaptureSession.mockResolvedValue(fixtureCollectorRun());
    generated.finishWebAuthorizationSession.mockResolvedValue(fixtureCollectorRun());
    transport.invoke.mockReset().mockRejectedValue(new Error("legacy transport invoked"));
  });

  it("routes capture session status commands through generated wrappers", async () => {
    await startCaptureSession("station-1");
    await getCaptureSessionStatus("station-1");
    await clearCaptureSession("station-1");
    await closeCaptureSession("station-1");

    expect(generated.startCaptureSession).toHaveBeenCalledWith({ stationId: "station-1" });
    expect(generated.getCaptureSessionStatus).toHaveBeenCalledWith({ stationId: "station-1" });
    expect(generated.clearCaptureSession).toHaveBeenCalledWith({ stationId: "station-1" });
    expect(generated.closeCaptureSession).toHaveBeenCalledWith({ stationId: "station-1" });
    expect(transport.invoke).not.toHaveBeenCalled();
  });

  it("routes capture finish commands through generated wrappers", async () => {
    await finishCaptureSession("station-1");
    await finishWebAuthorizationSession("station-1");

    expect(generated.finishCaptureSession).toHaveBeenCalledWith({ stationId: "station-1" });
    expect(generated.finishWebAuthorizationSession).toHaveBeenCalledWith({ stationId: "station-1" });
    expect(transport.invoke).not.toHaveBeenCalled();
  });
});

function fixtureCaptureStatus() {
  return {
    stationId: "station-1",
    status: "capturing",
    captureCount: 0,
    recognizedFieldCount: 0,
    pendingConfirmationCount: 0,
    webAuthorizationCandidate: false,
    lastError: null,
  };
}

function fixtureCollectorRun() {
  const now = "2026-07-24T00:00:00Z";
  return {
    snapshot: {
      id: "snapshot-1",
      stationId: "station-1",
      endpointRevision: 1,
      source: "webview-capture",
      status: "checked",
      fetchedAt: now,
      summaryJson: {},
      normalizedJson: {},
      rawJsonRedacted: null,
      errorMessage: null,
      createdAt: now,
    },
    events: [],
  };
}
