import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const generated = vi.hoisted(() => ({
  getStationPublishedStatusWorkspace: vi.fn(),
}));
const transport = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock("@/lib/bridge/generated", () => generated);
vi.mock("@/lib/bridge/transport", () => transport);

import { setActiveBackendClient } from "@/lib/bridge/activeBackendClient";
import { DemoBackend } from "@/lib/bridge/DemoBackend";
import { DesktopBackend } from "@/lib/bridge/DesktopBackend";
import { getStationPublishedStatusWorkspace } from "./stationPublishedStatus";

describe("station published-status API generated binding cutover", () => {
  beforeEach(() => {
    setActiveBackendClient(new DesktopBackend());
    generated.getStationPublishedStatusWorkspace.mockReset().mockResolvedValue(workspaceDto());
    transport.invoke.mockReset().mockRejectedValue(new Error("legacy transport invoked"));
  });

  afterEach(() => {
    setActiveBackendClient(null);
  });

  it("loads the dedicated workspace through the generated command and normalizes timestamps", async () => {
    await expect(getStationPublishedStatusWorkspace("station-1")).resolves.toEqual({
      stationId: "station-1",
      endpointRevision: 2,
      supported: true,
      sourceState: "available",
      completeness: "complete",
      lastAttemptAtMs: 1_700_000_000_000,
      lastSuccessAtMs: 1_700_000_000_000,
      lastCompleteAtMs: 1_700_000_000_000,
      monitorCount: 1,
      stale: false,
      safeErrorKind: null,
      rows: [
        {
          rowKey: "published-monitor-1",
          upstreamMonitorId: "upstream-monitor-1",
          identityKind: "upstream_id",
          name: "Fixture published monitor",
          provider: "fixture-provider",
          groupName: "default",
          primaryModel: "fixture-model",
          extraModels: ["fixture-extra-model"],
          currentOutcome: "available",
          currentLatencyMs: 42,
          currentPingLatencyMs: 7,
          recentAvailabilityPercent: 99.5,
          upstreamCheckedAtMs: 1_700_000_000_000,
          recentSamples: [
            {
              id: "published-monitor-1:fixture-model:1700000000000:0",
              model: "fixture-model",
              outcome: "available",
              latencyMs: 42,
              pingLatencyMs: 7,
              checkedAtMs: 1_700_000_000_000,
            },
          ],
        },
      ],
    });

    expect(generated.getStationPublishedStatusWorkspace).toHaveBeenCalledWith({ stationId: "station-1" });
    expect(transport.invoke).not.toHaveBeenCalled();
  });

  it("returns a controlled unsupported workspace when the active backend has no capability", async () => {
    setActiveBackendClient(new DemoBackend());

    await expect(getStationPublishedStatusWorkspace("station-1")).resolves.toEqual({
      stationId: "station-1",
      endpointRevision: 0,
      supported: false,
      sourceState: "unsupported",
      completeness: null,
      lastAttemptAtMs: null,
      lastSuccessAtMs: null,
      lastCompleteAtMs: null,
      monitorCount: 0,
      stale: false,
      safeErrorKind: null,
      rows: [],
    });
  });
});

function workspaceDto() {
  return {
    stationId: "station-1",
    endpointRevision: 2,
    supported: true,
    sourceState: "available" as const,
    completeness: "complete" as const,
    lastAttemptAtMs: 1_700_000_000_000,
    lastSuccessAtMs: 1_700_000_000_000,
    lastCompleteAtMs: 1_700_000_000_000,
    safeErrorKind: null,
    monitorCount: 1,
    stale: false,
    rows: [
      {
        id: "published-monitor-1",
        upstreamMonitorId: "upstream-monitor-1",
        identityKind: "upstream_id" as const,
        name: "Fixture published monitor",
        provider: "fixture-provider",
        groupName: "default",
        primaryModel: "fixture-model",
        extraModels: ["fixture-extra-model"],
        presenceStatus: "current" as const,
        currentOutcome: "available" as const,
        sourceStatus: "healthy",
        currentLatencyMs: 42,
        currentPingLatencyMs: 7,
        recentAvailabilityPercent: 99.5,
        upstreamCheckedAtMs: 1_700_000_000_000,
        samples: [
          {
            model: "fixture-model",
            checkedAtMs: 1_700_000_000_000,
            outcome: "available" as const,
            sourceStatus: "healthy",
            latencyMs: 42,
            pingLatencyMs: 7,
            safeMessage: null,
          },
        ],
      },
    ],
  };
}
