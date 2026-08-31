import { describe, expect, it } from "vitest";
import type { RoutingCandidateView } from "@/lib/types/routingWorkspace";
import { buildCandidateDisplayFacts } from "./localRoutingStatusViewModel";

function candidate(overrides: Partial<RoutingCandidateView> = {}): RoutingCandidateView {
  return {
    stationKeyId: "key-1",
    stationId: "station-1",
    stationName: "Station",
    keyName: "密钥",
    endpoint: "chat_completions",
    priority: 1,
    enabled: true,
    schedulable: true,
    healthState: "ready",
    score: null,
    scoreDetails: null,
    currentConcurrency: null,
    lastSuccessAt: null,
    lastFailureAt: null,
    cooldownUntil: null,
    routingGroupScope: "all_groups",
    routingGroupMatch: true,
    scoreStatus: "scored",
    plannerExclusionCodes: [],
    assessmentSnapshotId: null,
    assessmentDurableRevision: null,
    assessmentRequestContextFingerprint: null,
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
  it("renders candidate economics from backend facts without legacy multiplier fields", () => {
    const display = buildCandidateDisplayFacts(candidate());

    expect(display.multiplierLabel).toBe("1.250x");
    expect(display.multiplierDetail).toBeNull();
    expect(display.balanceLabel).toBe("正常");
    expect(display.balanceDetail).toBeNull();
  });

  it("renders effective multipliers with three decimal places", () => {
    const display = buildCandidateDisplayFacts(
      candidate({
        facts: [
          {
            kind: "pricing",
            label: "Effective multiplier",
            value: "0.075x",
            severity: "info",
          },
        ],
      }),
    );

    expect(display.multiplierLabel).toBe("0.075x");
  });

  it("shows the collected balance amount instead of its status", () => {
    const display = buildCandidateDisplayFacts(
      candidate({ balanceValue: 12.5, balanceCurrency: "USD" }),
    );

    expect(display.balanceLabel).toBe("12.50$");
    expect(display.balanceDetail).toBeNull();
  });

  it("preserves a negative collected balance even when its status is stale", () => {
    const display = buildCandidateDisplayFacts(
      candidate({ balanceValue: -0.05, balanceCurrency: "USD" }),
    );

    expect(display.balanceLabel).toBe("-0.05$");
    expect(display.balanceAmountLabel).toBe("-0.05");
  });

  it("does not show a conflicting normal status when routing rejected the balance", () => {
    const display = buildCandidateDisplayFacts(
      candidate({ previewRejectReasons: ["balance_depleted"] }),
    );

    expect(display.balanceLabel).toBe("余额不足");
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

  it("maps canonical backend rejection codes without falling back to a generic message", () => {
    expect(
      buildCandidateDisplayFacts(
        candidate({
          schedulable: true,
          previewEligible: false,
          previewRejectReasons: ["group_mismatch"],
        }),
      ).rejectReasonLabel,
    ).toBe("分组不匹配");
    expect(
      buildCandidateDisplayFacts(
        candidate({
          schedulable: false,
          previewEligible: false,
          previewRejectReasons: ["candidate_unschedulable"],
        }),
      ).rejectReasonLabel,
    ).toBe("密钥已暂停路由");
  });
});
