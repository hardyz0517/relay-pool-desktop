import { describe, expect, it } from "vitest";
import { detectProvider, parseRechargeRun } from "./RechargeDialog";

const run = (summaryJson: Record<string, unknown>, normalizedJson: Record<string, unknown>, status = "success") => ({
  snapshot: {
    id: "snapshot-1",
    stationId: "station-1",
    endpointRevision: 1,
    source: "webview-capture",
    status,
    fetchedAt: "1",
    summaryJson,
    normalizedJson,
    rawJsonRedacted: null,
    errorMessage: null,
    createdAt: "1",
  },
  events: [],
});

describe("recharge collection result parsing", () => {
  it("recognizes Liandong and Cloudcat station identities", () => {
    expect(detectProvider({ name: "链动小铺", websiteUrl: "https://relay.example" })).toBe("liandong");
    expect(detectProvider({ name: "Cloudcat relay", websiteUrl: "https://relay.example" })).toBe("cloudcat");
    expect(detectProvider({ name: "普通站点", websiteUrl: "https://relay.example" })).toBe("custom");
  });

  it("only returns absolute entries supplied by the authenticated collector", () => {
    const parsed = parseRechargeRun(run(
      { status: "success", provider: "liandong" },
      { entries: [{ url: "https://relay.example/purchase", label: "充值中心", paymentMethods: ["alipay"] }, { url: "/topup", label: "猜测路径" }] },
    ));
    expect(parsed.entries).toEqual([{ url: "https://relay.example/purchase", label: "充值中心", provider: "liandong", paymentMethods: ["alipay"], source: "confirmed" }]);
  });

  it("removes sensitive query parameters from collector entries", () => {
    const parsed = parseRechargeRun(run(
      { status: "success" },
      { entries: [{ url: "https://relay.example/purchase?token=secret&plan=pro", label: "订阅购买" }] },
    ));
    expect(parsed.entries[0]?.url).toBe("https://relay.example/purchase?plan=pro");
  });

  it("does not expose entries for login-required or partial scans", () => {
    expect(parseRechargeRun(run({ status: "login_required" }, { entries: [] }, "manual_required")).status).toBe("manual_required");
    expect(parseRechargeRun(run({ status: "no_match" }, { entries: [] }, "partial")).entries).toEqual([]);
  });
});
