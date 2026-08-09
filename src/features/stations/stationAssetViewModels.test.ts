import { describe, expect, it } from "vitest";
import { hasPositiveBalance, STATION_ISSUE_FILTER_OPTIONS } from "./stationAssetViewModels";

describe("hasPositiveBalance", () => {
  it.each([
    [null, false],
    [undefined, false],
    [0, false],
    [-0.01, false],
    [Number.NaN, false],
    [Number.POSITIVE_INFINITY, false],
    [0.01, true],
  ])("classifies %s as %s", (value, expected) => {
    expect(hasPositiveBalance(value)).toBe(expected);
  });
});

describe("station issue filters", () => {
  it("does not treat a missing station API key as an issue", () => {
    expect(STATION_ISSUE_FILTER_OPTIONS.map((option) => option.label)).not.toContain("缺 API 密钥");
  });
});
