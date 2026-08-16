import { describe, expect, it } from "vitest";
import type { AlertingChangeActivity } from "@/lib/types/alerting";
import {
  CHANGE_CENTER_CLEAR_SCOPE_BY_VIEW,
  CHANGE_CENTER_DEFAULT_VIEW,
  CHANGE_CENTER_MARK_SEEN_SCOPE_BY_VIEW,
  CHANGE_CENTER_VIEW_OPTIONS,
  changeObjectTitle,
  changeSummary,
  incidentSummary,
  incidentTitle,
} from "./ChangeCenterPage";

function changeActivity(overrides: Partial<AlertingChangeActivity> = {}): AlertingChangeActivity {
  return {
    recordType: "change",
    id: "change-1",
    eventType: "group_rate_changed",
    severity: "info",
    groupName: null,
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
  it("defaults to the all view while remaining externally controllable", () => {
    expect(CHANGE_CENTER_DEFAULT_VIEW).toBe("all");
    expect(CHANGE_CENTER_VIEW_OPTIONS.map((option) => option.value)).toEqual(["all", "unread"]);
  });

  it("orders the views with all first and unread second", () => {
    expect(CHANGE_CENTER_VIEW_OPTIONS).toEqual([
      { value: "all", label: "全部" },
      { value: "unread", label: "未读" },
    ]);
  });

  it("clears incidents and information according to the selected view", () => {
    expect(CHANGE_CENTER_CLEAR_SCOPE_BY_VIEW).toEqual({
      all: "all",
      unread: "all",
    });
    expect(CHANGE_CENTER_MARK_SEEN_SCOPE_BY_VIEW).toEqual(CHANGE_CENTER_CLEAR_SCOPE_BY_VIEW);
  });

  it("shows a readable informational rate transition", () => {
    expect(changeSummary(changeActivity())).toBe("默认组 · 倍率 1 → 0.8");
  });

  it("places the changed object ahead of its rate transition", () => {
    expect(changeObjectTitle(changeActivity({
      stationId: "station-1",
      newValueJson: JSON.stringify({
        groupName: "福利",
        oldEffectiveRateMultiplier: 0.1,
        newEffectiveRateMultiplier: 0.01,
      }),
    }), "KeikoAI")).toBe("KeikoAI · 福利");
  });

  it("labels newly discovered groups without treating them as incidents", () => {
    expect(changeSummary(changeActivity({
      eventType: "group_added",
      reasonCode: "group_added",
      newValueJson: JSON.stringify({ groupName: "开发组", status: "available" }),
    }))).toBe("开发组 · 新增分组");
  });

  it("names the missing group in informational incident titles", () => {
    expect(incidentTitle({ eventType: "group_missing", groupName: "Claude Kiro 高速" }))
      .toBe("分组缺失 · Claude Kiro 高速");
  });

  it("presents missing groups as information rather than an alert lifecycle", () => {
    const summary = incidentSummary({
      conditionKey: "station_group:station-1:group-1",
      eventType: "group_missing",
      groupName: "Claude Cursor",
      lifecycleState: "pending",
      occurrenceCount: 1,
      severity: "info",
      stationId: "station-1",
    }, "TNTAPI");

    expect(summary).toBe("TNTAPI · Claude Cursor · 远程分组未找到");
    expect(summary).not.toMatch(/检测中|已出现/);
  });
});
