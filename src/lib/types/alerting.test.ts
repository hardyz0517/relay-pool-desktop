import { describe, expect, it } from "vitest";
import {
  ALERT_EVENT_OPTIONS,
  DEFAULT_ALERTING_SETTINGS,
  defaultAlertPolicy,
  toAlertPolicyInput,
  toAlertingSettingsInput,
} from "@/lib/types/alerting";

describe("alerting frontend contract", () => {
  it("keeps every registered event configurable and creates valid defaults", () => {
    expect(ALERT_EVENT_OPTIONS.length).toBeGreaterThan(10);
    for (const option of ALERT_EVENT_OPTIONS) {
      const policy = defaultAlertPolicy(option.value);
      expect(policy.eventType).toBe(option.value);
      expect(policy.state).toBe("active");
      expect(policy.recoveryCount).toBe(1);
      if (policy.triggerMode === "immediate") {
        expect(policy.triggerCount).toBeNull();
        expect(policy.triggerDurationSeconds).toBeNull();
      }
    }
  });

  it("requires a recovery notification by default", () => {
    expect(defaultAlertPolicy("station_down")).toMatchObject({
      triggerMode: "consecutive_occurrences",
      triggerCount: 2,
      recoveryMode: "consecutive_healthy",
      recoveryCount: 1,
      recoveryNotificationEnabled: true,
      inAppEnabled: true,
    });
  });

  it("uses stricter defaults for key validity and audit changes", () => {
    expect(defaultAlertPolicy("key_invalid")).toMatchObject({
      triggerMode: "immediate",
      triggerCount: null,
      recoveryCount: 1,
    });
    expect(defaultAlertPolicy("balance_depleted")).toMatchObject({
      triggerMode: "consecutive_occurrences",
      triggerCount: 2,
    });
    expect(defaultAlertPolicy("collector_failed")).toMatchObject({
      triggerMode: "consecutive_occurrences",
      triggerCount: 3,
    });
    expect(defaultAlertPolicy("price_changed")).toMatchObject({
      triggerMode: "immediate",
      triggerCount: null,
      recoveryCount: 1,
    });
  });

  it("removes read-only fields before saving settings or an existing policy", () => {
    const settingsInput = toAlertingSettingsInput({
      ...DEFAULT_ALERTING_SETTINGS,
      revision: 4,
      updatedAtMs: 1_700_000_000_000,
    });
    expect(settingsInput).toMatchObject({ expectedRevision: 4, enabled: true });
    expect(settingsInput).not.toHaveProperty("revision");
    expect(settingsInput).not.toHaveProperty("updatedAtMs");

    const policyInput = toAlertPolicyInput(defaultAlertPolicy("station_down"), 3);
    expect(policyInput).toMatchObject({ expectedRevision: 3, eventType: "station_down" });
    expect(policyInput).not.toHaveProperty("revision");
    expect(policyInput).not.toHaveProperty("createdAtMs");
    expect(policyInput).not.toHaveProperty("updatedAtMs");
  });
});
