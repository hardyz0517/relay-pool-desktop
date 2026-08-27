// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { RechargeDialog } from "./RechargeDialog";
import { readRechargeEntries, writeRechargeEntries } from "./rechargeEntriesStorage";

const { getLatestCollectorSnapshot, redeemStationCode, scanStationRecharge } = vi.hoisted(() => ({
  getLatestCollectorSnapshot: vi.fn(),
  redeemStationCode: vi.fn(),
  scanStationRecharge: vi.fn(),
}));

vi.mock("@/lib/api/collector", () => ({ getLatestCollectorSnapshot, redeemStationCode, scanStationRecharge }));

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const station = {
  id: "station-1",
  name: "Grox",
  stationType: "sub2api",
  websiteUrl: "https://relay.example",
} as never;

function successfulScan() {
  return {
    snapshot: {
      id: "snapshot-1",
      stationId: "station-1",
      endpointRevision: 1,
      source: "webview-capture",
      status: "success",
      fetchedAt: "1",
      summaryJson: { status: "success", provider: "cloudcat" },
      normalizedJson: { entries: [{ url: "https://relay.example/purchase", label: "订阅购买", paymentMethods: [] }] },
      rawJsonRedacted: null,
      errorMessage: null,
      createdAt: "1",
    },
    events: [],
  };
}

function button(label: string): HTMLButtonElement {
  const match = [...document.querySelectorAll<HTMLButtonElement>("button")].find((item) => item.textContent?.includes(label));
  if (!match) throw new Error(`button not found: ${label}`);
  return match;
}

describe("RechargeDialog interaction state", () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    window.localStorage.clear();
    getLatestCollectorSnapshot.mockReset().mockResolvedValue(null);
    redeemStationCode.mockReset();
    scanStationRecharge.mockReset();
  });

  afterEach(async () => {
    await act(async () => root.unmount());
    document.body.innerHTML = "";
    vi.clearAllMocks();
  });

  it("does not scan when the dialog opens", async () => {
    await act(async () => {
      root.render(<RechargeDialog station={station} onClose={vi.fn()} onOpenUrl={vi.fn(async () => undefined)} />);
    });

    expect(scanStationRecharge).not.toHaveBeenCalled();
    expect(document.body.textContent).toContain("扫描充值方式");
    expect([...document.querySelectorAll<HTMLButtonElement>("button")].filter((item) => item.textContent?.includes("扫描充值方式"))).toHaveLength(1);
    expect(document.body.textContent).not.toContain("忽略");
    expect([...document.querySelectorAll("button")].some((item) => item.textContent === "完成")).toBe(false);
  });

  it("redeems a Sub2API code and clears it after success", async () => {
    redeemStationCode.mockResolvedValue({ provider: "sub2api", success: true, message: "兑换成功", creditedDetail: "已添加：$1.00" });
    await act(async () => {
      root.render(<RechargeDialog station={station} onClose={vi.fn()} onOpenUrl={vi.fn(async () => undefined)} />);
    });
    const input = document.querySelector<HTMLInputElement>('input[aria-label="兑换码"]');
    expect(input).not.toBeNull();
    await act(async () => {
      if (input) {
        Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set?.call(input, "SUB2-FAKE-CODE");
        input.dispatchEvent(new Event("input", { bubbles: true }));
      }
    });
    await act(async () => button("兑换").click());

    expect(redeemStationCode).toHaveBeenCalledWith("station-1", "SUB2-FAKE-CODE");
    expect(document.body.textContent).toContain("兑换成功");
    expect(document.body.textContent).toContain("已添加：$1.00");
    expect(input?.value).toBe("");
  });

  it("renders a localized failure panel with the concrete reason", async () => {
    redeemStationCode.mockResolvedValue({
      provider: "sub2api",
      success: false,
      message: "该兑换码已被使用。",
      creditedDetail: null,
    });
    await act(async () => {
      root.render(<RechargeDialog station={station} onClose={vi.fn()} onOpenUrl={vi.fn(async () => undefined)} />);
    });
    const input = document.querySelector<HTMLInputElement>('input[aria-label="兑换码"]');
    await act(async () => {
      if (input) {
        Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set?.call(input, "USED-FAKE-CODE");
        input.dispatchEvent(new Event("input", { bubbles: true }));
      }
    });
    await act(async () => button("兑换").click());

    const feedback = document.querySelector('[role="status"]');
    expect(feedback?.textContent).toContain("兑换失败");
    expect(feedback?.textContent).toContain("该兑换码已被使用。");
    expect(feedback?.className).toContain("bg-danger-surface");
  });

  it("renders the NewAPI redemption input without explanatory copy", async () => {
    await act(async () => {
      root.render(<RechargeDialog station={{ ...(station as object), stationType: "newapi" } as never} onClose={vi.fn()} onOpenUrl={vi.fn(async () => undefined)} />);
    });
    expect(document.querySelector('input[aria-label="兑换码"]')).not.toBeNull();
    expect(document.body.textContent).not.toContain("兑换到 NewAPI 钱包余额");
  });

  it("opens the manual entry form between recharge entries and redemption", async () => {
    await act(async () => {
      root.render(<RechargeDialog station={station} onClose={vi.fn()} onOpenUrl={vi.fn(async () => undefined)} />);
    });
    await act(async () => button("手动添加入口").click());

    const entriesHeading = [...document.querySelectorAll("h3")]
      .find((item) => item.textContent?.includes("充值入口"));
    const entriesSection = entriesHeading?.closest("section");
    const entryForm = [...document.querySelectorAll("form")]
      .find((item) => item.textContent?.includes("添加充值入口"));
    const redemptionForm = document.querySelector<HTMLFormElement>('form[aria-label="兑换码操作"]');
    expect(document.querySelector('button[aria-label="取消编辑"]')).toBeNull();
    expect(entriesSection && entryForm
      ? entriesSection.compareDocumentPosition(entryForm) & Node.DOCUMENT_POSITION_FOLLOWING
      : 0).toBeTruthy();
    expect(entryForm && redemptionForm
      ? entryForm.compareDocumentPosition(redemptionForm) & Node.DOCUMENT_POSITION_FOLLOWING
      : 0).toBeTruthy();
  });

  it("round-trips manually managed entries in station-scoped storage", () => {
    const entry = { url: "https://relay.example/topup", label: "余额充值", provider: "custom" as const, paymentMethods: [], source: "manual" as const };
    writeRechargeEntries("station-1", [entry]);
    expect(readRechargeEntries("station-1")).toEqual([entry]);
    expect(readRechargeEntries("station-2")).toEqual([]);
  });

  it("keeps scan results as candidates until the user confirms them", async () => {
    scanStationRecharge.mockResolvedValue(successfulScan());
    await act(async () => {
      root.render(<RechargeDialog station={station} onClose={vi.fn()} onOpenUrl={vi.fn(async () => undefined)} />);
    });
    await act(async () => button("扫描充值方式").click());

    expect(document.body.textContent).toContain("扫描结果（1）");
    expect(document.body.textContent).toContain("新发现");
    expect(document.body.textContent).toContain("忽略");
    expect(document.querySelector('button[aria-label^="忽略："]')).toBeNull();
    expect(document.body.textContent).toContain("充值入口（0）");

    await act(async () => button("确认添加").click());
    expect(document.body.textContent).toContain("充值入口（1）");
    expect(document.body.textContent).toContain("已存在");
  });

  it("places scan errors in the results section before redemption", async () => {
    scanStationRecharge.mockRejectedValue(new Error("timed_out"));
    await act(async () => {
      root.render(<RechargeDialog station={station} onClose={vi.fn()} onOpenUrl={vi.fn(async () => undefined)} />);
    });
    await act(async () => button("扫描充值方式").click());

    const resultsHeading = [...document.querySelectorAll("h3")].find((item) => item.textContent?.includes("扫描结果"));
    const resultsSection = resultsHeading?.closest("section");
    const redemptionForm = document.querySelector<HTMLFormElement>('form[aria-label="兑换码操作"]');
    expect(resultsSection?.textContent).toContain("扫描失败");
    expect(resultsSection?.textContent).toContain("重试");
    const relativePosition = resultsSection && redemptionForm
      ? resultsSection.compareDocumentPosition(redemptionForm)
      : 0;
    expect(relativePosition & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("renders the entry actions menu outside the dialog scroll container", async () => {
    writeRechargeEntries("station-1", [{
      url: "https://catfk.com/shop/pikaqiu",
      label: "兑换码购买",
      provider: "cloudcat",
      paymentMethods: [],
      source: "confirmed",
    }]);
    await act(async () => {
      root.render(<RechargeDialog station={station} onClose={vi.fn()} onOpenUrl={vi.fn(async () => undefined)} />);
    });

    const menuButton = document.querySelector<HTMLButtonElement>('button[aria-label^="更多操作"]');
    expect(menuButton).not.toBeNull();
    await act(async () => menuButton?.click());

    const editButton = [...document.querySelectorAll<HTMLButtonElement>("button")]
      .find((item) => item.textContent?.includes("编辑"));
    expect(editButton?.parentElement?.className).toContain("fixed");
  });

  it("opens the removal confirmation when the menu action is clicked", async () => {
    writeRechargeEntries("station-1", [{
      url: "https://catfk.com/shop/pikaqiu",
      label: "兑换码购买",
      provider: "cloudcat",
      paymentMethods: [],
      source: "confirmed",
    }]);
    await act(async () => {
      root.render(<RechargeDialog station={station} onClose={vi.fn()} onOpenUrl={vi.fn(async () => undefined)} />);
    });

    await act(async () => document.querySelector<HTMLButtonElement>('button[aria-label^="更多操作"]')?.click());
    const removeButton = [...document.querySelectorAll<HTMLButtonElement>("button")]
      .find((item) => item.textContent?.includes("移除"));
    expect(removeButton).not.toBeUndefined();
    await act(async () => removeButton?.click());

    expect(document.body.textContent).toContain("移除充值入口");
  });
});
