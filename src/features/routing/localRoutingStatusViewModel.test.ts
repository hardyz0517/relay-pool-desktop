import { describe, expect, it } from "vitest";
import type { RoutingCandidateView } from "@/lib/types/routingWorkspace";
import {
  buildCandidateDisplayFacts,
  buildCandidateHealthDisplay,
} from "./localRoutingStatusViewModel";

function candidate(overrides: Partial<RoutingCandidateView> = {}): RoutingCandidateView {
  return {
    stationKeyId: "key-1",
    stationId: "station-1",
    stationName: "Station",
    keyName: "Key",
    endpoint: "chat_completions",
    priority: 1,
    enabled: true,
    schedulable: true,
    healthState: "ready",
    lastSuccessAt: null,
    lastFailureAt: null,
    cooldownUntil: null,
    routingGroupScope: "all_groups",
    routingGroupMatch: true,
    previewEligible: true,
    previewRejectReasons: [],
    facts: [
      {
        kind: "pricing",
        label: "Effective multiplier",
        value: "1.2500x via backend_projection",
        severity: "info",
      },
      {
        kind: "balance",
        label: "Balance",
        value: "normal",
        severity: "info",
      },
    ],
    ...overrides,
  };
}

describe("local routing status view model", () => {
  it("maps routing health into a shared candidate display", () => {
    expect(buildCandidateHealthDisplay("ready")).toEqual({
      label: "就绪",
      tone: "healthy",
    });
    expect(buildCandidateHealthDisplay("unknown")).toEqual({
      label: "未知",
      tone: "disabled",
    });
  });

  it("renders candidate economics from backend facts without legacy multiplier fields", () => {
    const display = buildCandidateDisplayFacts(candidate());

    expect(display.multiplierLabel).toBe("1.2500x");
    expect(display.multiplierDetail).toBeNull();
    expect(display.balanceLabel).toBe("正常");
    expect(display.balanceDetail).toBeNull();
  });

  it("uses backend rejection codes and stays explicit when facts are missing", () => {
    const display = buildCandidateDisplayFacts(
      candidate({
        previewEligible: false,
        previewRejectReasons: ["multiplier_over_ceiling"],
        facts: [],
      }),
    );

    expect(display.rejectReasonLabel).toBe("超过倍率上限");
    expect(display.multiplierLabel).toBe("后端未提供");
    expect(display.balanceLabel).toBe("后端未提供");
  });
});
