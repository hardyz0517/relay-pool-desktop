import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { setActiveBackendClient } from "@/lib/bridge/activeBackendClient";
import type { BackendClient } from "@/lib/bridge/BackendClient";

import {
  clearCaptureSession,
  closeCaptureSession,
  collectStationInfo,
  collectStationTask,
  collectSub2apiStation,
  detectStationInfo,
  detectSub2apiStation,
  finishCaptureSession,
  finishWebAuthorizationSession,
  getCaptureSessionStatus,
  getLatestCollectorSnapshot,
  listCollectorSnapshots,
  listLatestCollectorSnapshots,
  startCaptureSession,
  startManualAuthorization,
  testStationLogin,
  testStationLoginInput,
} from "./collector";

describe("collector backend cutover", () => {
  const collectors = {
    detectSub2apiStation: vi.fn(async () => runResult()),
    collectSub2apiStation: vi.fn(async () => runResult()),
    detectStationInfo: vi.fn(async () => runResult()),
    collectStationInfo: vi.fn(async () => runResult()),
    collectStationTask: vi.fn(async () => runResult()),
    testStationLogin: vi.fn(async () => runResult()),
    testStationLoginInput: vi.fn(async () => ({
      status: "success",
      message: "ok",
      diagnosis: null,
      tokenPresent: true,
    })),
    listCollectorSnapshots: vi.fn(async () => []),
    getLatestCollectorSnapshot: vi.fn(async () => null),
    listLatestCollectorSnapshots: vi.fn(async () => []),
    startCaptureSession: vi.fn(async () => captureStatus("capturing")),
    getCaptureSessionStatus: vi.fn(async () => captureStatus("idle")),
    finishCaptureSession: vi.fn(async () => runResult()),
    finishWebAuthorizationSession: vi.fn(async () => runResult()),
    clearCaptureSession: vi.fn(async () => captureStatus("idle")),
    closeCaptureSession: vi.fn(async () => captureStatus("idle")),
  };

  beforeEach(() => {
    setActiveBackendClient(testBackendClient({ collectors: collectors as BackendClient["collectors"] }));
    for (const fn of Object.values(collectors)) {
      fn.mockClear();
    }
  });

  afterEach(() => {
    setActiveBackendClient(null);
  });

  it("routes collector and capture operations through the active backend client", async () => {
    const loginInput = {
      stationType: "newapi",
      websiteUrl: "https://example.test",
      loginUsername: "fixture-user",
      loginPassword: "fixture-password",
    };

    await detectSub2apiStation("station-1");
    await collectSub2apiStation("station-1");
    await detectStationInfo("station-1");
    await collectStationInfo("station-1");
    await collectStationTask("station-1", "full");
    await testStationLogin("station-1");
    await testStationLoginInput(loginInput);
    await listCollectorSnapshots("station-1");
    await getLatestCollectorSnapshot("station-1");
    await listLatestCollectorSnapshots(["station-1", "station-2"]);
    await startCaptureSession("station-1");
    await startManualAuthorization("station-1");
    await getCaptureSessionStatus("station-1");
    await finishCaptureSession("station-1");
    await finishWebAuthorizationSession("station-1");
    await clearCaptureSession("station-1");
    await closeCaptureSession("station-1");

    expect(collectors.detectSub2apiStation).toHaveBeenCalledWith("station-1");
    expect(collectors.collectSub2apiStation).toHaveBeenCalledWith("station-1");
    expect(collectors.detectStationInfo).toHaveBeenCalledWith("station-1");
    expect(collectors.collectStationInfo).toHaveBeenCalledWith("station-1");
    expect(collectors.collectStationTask).toHaveBeenCalledWith("station-1", "full");
    expect(collectors.testStationLogin).toHaveBeenCalledWith("station-1");
    expect(collectors.testStationLoginInput).toHaveBeenCalledWith(loginInput);
    expect(collectors.listCollectorSnapshots).toHaveBeenCalledWith("station-1");
    expect(collectors.getLatestCollectorSnapshot).toHaveBeenCalledWith("station-1");
    expect(collectors.listLatestCollectorSnapshots).toHaveBeenCalledWith(["station-1", "station-2"]);
    expect(collectors.startCaptureSession).toHaveBeenCalledTimes(2);
    expect(collectors.getCaptureSessionStatus).toHaveBeenCalledWith("station-1");
    expect(collectors.finishCaptureSession).toHaveBeenCalledWith("station-1");
    expect(collectors.finishWebAuthorizationSession).toHaveBeenCalledWith("station-1");
    expect(collectors.clearCaptureSession).toHaveBeenCalledWith("station-1");
    expect(collectors.closeCaptureSession).toHaveBeenCalledWith("station-1");
  });
});

function testBackendClient(overrides: Partial<BackendClient>): BackendClient {
  return {
    mode: "desktop",
    settings: {} as BackendClient["settings"],
    stations: {} as BackendClient["stations"],
    stationKeys: {} as BackendClient["stationKeys"],
    changeEvents: {} as BackendClient["changeEvents"],
    collectorRuns: {} as BackendClient["collectorRuns"],
    collectors: {} as BackendClient["collectors"],
    proxy: {} as BackendClient["proxy"],
    runtime: {} as BackendClient["runtime"],
    localRouting: {} as BackendClient["localRouting"],
    dataRecovery: {} as BackendClient["dataRecovery"],
    economics: {} as BackendClient["economics"],
    groupFacts: {} as BackendClient["groupFacts"],
    pricing: {} as BackendClient["pricing"],
    routing: {} as BackendClient["routing"],
    channels: {} as BackendClient["channels"],
    updater: {} as BackendClient["updater"],
    handshake: vi.fn(async () => ({}) as never),
    ...overrides,
  };
}

function captureStatus(status: string) {
  return {
    stationId: "station-1",
    status,
    captureCount: 0,
    recognizedFieldCount: 0,
    pendingConfirmationCount: 0,
    webAuthorizationCandidate: false,
    lastError: null,
  };
}

function runResult() {
  return {
    snapshot: {
      id: "snapshot-1",
      stationId: "station-1",
      endpointRevision: 1,
      source: "fixture",
      status: "checked",
      fetchedAt: "now",
      summaryJson: {},
      normalizedJson: {},
      rawJsonRedacted: null,
      errorMessage: null,
      createdAt: "now",
    },
    events: [],
  };
}
