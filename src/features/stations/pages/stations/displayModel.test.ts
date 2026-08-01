import { afterEach, describe, expect, it, vi } from "vitest";
import type { StationAssetRow } from "../../stationAssetViewModels";
import {
  collectorRunStatusLabel,
  collectorTaskTypeLabel,
  formatMultiplier,
  formatNullableTime,
  formatRelativeTime,
  formatStationBalanceParts,
  formatStationDisplayUrl,
  groupBindingStatusLabel,
  stationAvatarLabel,
  stationIssueTagClassName,
} from "./displayModel";

describe("stations page display model", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("formats station identity and balance summaries", () => {
    expect(stationAvatarLabel(" 中转 ")).toBe("中");
    expect(stationAvatarLabel("")).toBe("?");
    expect(formatStationDisplayUrl("https://api.example/v1")).toBe("https://api.example");
    expect(formatStationDisplayUrl("not-a-url///")).toBe("not-a-url");

    expect(
      formatStationBalanceParts({
        station: { balanceCny: 12.345 },
        latestBalance: null,
      } as StationAssetRow),
    ).toEqual({ amount: "12.35", currency: "USD" });
    expect(
      formatStationBalanceParts({
        station: { balanceCny: null },
        latestBalance: { value: 8, currency: "USD" },
      } as StationAssetRow),
    ).toEqual({ amount: "8.00", currency: "USD" });

    expect(
      formatStationBalanceParts({
        station: { balanceCny: null },
        latestBalance: { value: 7.28, currency: "CNY" },
      } as StationAssetRow),
    ).toEqual({ amount: "7.28", currency: "USD" });
  });

  it("formats relative and nullable times", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-01-01T01:30:00Z"));

    expect(formatRelativeTime(null)).toBe("未采集");
    expect(formatRelativeTime("2026-01-01T01:29:30Z")).toBe("刚刚");
    expect(formatRelativeTime("2026-01-01T01:00:00Z")).toBe("30 分钟前");
    expect(formatRelativeTime("2025-12-31T01:30:00Z")).toBe("1 天前");
    expect(formatNullableTime(null)).toBe("未记录");
    expect(formatNullableTime("invalid")).toBe("invalid");
  });

  it("maps known status labels and keeps unknown values visible", () => {
    expect(formatMultiplier(1.234)).toBe("1.23x");
    expect(formatMultiplier(null)).toBe("-");
    expect(collectorTaskTypeLabel("groups")).toBe("分组");
    expect(collectorTaskTypeLabel("custom")).toBe("custom");
    expect(collectorRunStatusLabel("manual_required")).toBe("需要登录");
    expect(groupBindingStatusLabel("available")).toBe("可用");
    expect(stationIssueTagClassName("error")).toContain("text-danger-foreground");
  });
});
