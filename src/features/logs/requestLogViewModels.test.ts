import { describe, expect, it } from "vitest";
import type { RequestLog } from "@/lib/types/proxy";
import type { KeyPoolItem } from "@/lib/types/stationKeys";
import type { Station } from "@/lib/types/stations";
import {
  formatEndpoint,
  formatKeyName,
  formatKeyRate,
  formatRequestCost,
  pricingStatusLabel,
} from "./requestLogViewModels";

describe("formatKeyName", () => {
  it("shows the station and key names without exposing the masked API key", () => {
    const log = { stationKeyId: "key-1" } as RequestLog;
    const key = {
      id: "key-1",
      stationName: "示例站点",
      name: "日常密钥",
      apiKeyMasked: "sk-f********e",
    } as KeyPoolItem;

    expect(formatKeyName(log, new Map([[key.id, key]]))).toBe("示例站点 · 日常密钥");
  });

  it("keeps the existing fallback labels when no current key can be resolved", () => {
    expect(formatKeyName({ stationKeyId: null } as RequestLog, new Map())).toBe("未选择");
    expect(formatKeyName({ stationKeyId: "removed-key" } as RequestLog, new Map())).toBe("removed-key");
  });

  it("shows an unresolved key as processing while the request is active", () => {
    const log = {
      stationKeyId: null,
      status: "in_progress",
      lifecycleStatus: "admitted",
    } as RequestLog;

    expect(formatKeyName(log, new Map())).toBe("处理中");
  });
});

describe("formatEndpoint", () => {
  it.each([
    ["/v1/responses", "responses"],
    ["/v1/chat/completions", "completions"],
    ["/v1/responses?stream=true", "responses"],
  ])("formats %s as %s", (path, expected) => {
    expect(formatEndpoint(path)).toBe(expected);
  });
});

describe("formatKeyRate", () => {
  it("formats the multiplier after applying the station credit ratio", () => {
    const log = { stationKeyId: "key-1" } as RequestLog;
    const key = { id: "key-1", stationId: "station-1", rateMultiplier: 2 } as KeyPoolItem;
    const station = { id: "station-1", creditPerCny: 27 } as Station;

    expect(
      formatKeyRate(log, new Map([[key.id, key]]), new Map([[station.id, station]])),
    ).toBe("0.074x");
  });

  it("shows an unknown value when the key or multiplier is unavailable", () => {
    expect(formatKeyRate({ stationKeyId: null } as RequestLog, new Map(), new Map())).toBe("未知");
    expect(
      formatKeyRate(
        { stationKeyId: "key-1" } as RequestLog,
        new Map([["key-1", { id: "key-1", rateMultiplier: null } as KeyPoolItem]]),
        new Map(),
      ),
    ).toBe("未知");
  });
});

describe("formatRequestCost", () => {
  it("renders explicitly observed zero-token usage as zero cost", () => {
    expect(
      formatRequestCost({ totalTokens: 0, estimatedTotalCost: null, costStatus: null } as RequestLog),
    ).toBe("$0.000000");
  });
});

describe("pricingStatusLabel", () => {
  it("recognizes canonical request cost aggregate statuses", () => {
    expect(pricingStatusLabel("complete_single_currency")).toBe("已计价");
    expect(pricingStatusLabel("incomplete")).toBe("计费信息不完整");
    expect(pricingStatusLabel("not_applicable")).toBe("不适用");
  });
});
