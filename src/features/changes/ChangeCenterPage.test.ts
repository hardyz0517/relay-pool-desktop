import { describe, expect, it } from "vitest";
import type { AlertingChangeActivity } from "@/lib/types/alerting";
import { CHANGE_CENTER_VIEW_OPTIONS, changeSummary } from "./ChangeCenterPage";

function changeActivity(overrides: Partial<AlertingChangeActivity> = {}): AlertingChangeActivity {
  return {
    recordType: "change",
    id: "change-1",
    eventType: "group_rate_changed",
    severity: "info",
    stationId: "station-1",
    objectType: "station_group_binding",
    objectId: "group-1",
    stationKeyId: null,
    source: "collector",
    reasonCode: "group_rate_changed",
    conditionKey: "station_group_rate:station-1:group-1",
    lifecycleState: null,
    episodeNumber: null,
    occurrenceCount: null,
    activityAtMs: 1_700_000_000_000,
    oldValueJson: null,
    newValueJson: JSON.stringify({
      groupName: "默认组",
      oldEffectiveRateMultiplier: 1,
      newEffectiveRateMultiplier: 0.8,
    }),
    impactJson: null,
    collectorFailedTaskTypes: [],
    resolvedAtMs: null,
    seenAtMs: null,
    snoozedUntilMs: null,
    ...overrides,
  };
}

describe("change center activity presentation", () => {
  it("orders the views with all first and unread second", () => {
    expect(CHANGE_CENTER_VIEW_OPTIONS).toEqual([
      { value: "all", label: "全部" },
      { value: "unread", label: "未读" },
      { value: "active", label: "活动" },
      { value: "info", label: "信息" },
    ]);
    expect(CHANGE_CENTER_VIEW_OPTIONS.some((option) => option.label === "已恢复")).toBe(false);
  });

  it("shows a readable informational rate transition", () => {
    expect(changeSummary(changeActivity())).toBe("默认组 · 倍率 1 → 0.8");
  });

  it("labels newly discovered groups without treating them as incidents", () => {
    expect(changeSummary(changeActivity({
      eventType: "group_added",
      reasonCode: "group_added",
      newValueJson: JSON.stringify({ groupName: "开发组", status: "available" }),
    }))).toBe("开发组 · 新增分组");
  });
});
