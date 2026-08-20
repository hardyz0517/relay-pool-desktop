// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { RechargeDialog } from "./RechargeDialog";
import { readRechargeEntries, writeRechargeEntries } from "./rechargeEntriesStorage";

const { getLatestCollectorSnapshot, scanStationRecharge } = vi.hoisted(() => ({
  getLatestCollectorSnapshot: vi.fn(),
  scanStationRecharge: vi.fn(),
}));

vi.mock("@/lib/api/collector", () => ({ getLatestCollectorSnapshot, scanStationRecharge }));

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const station = {
  id: "station-1",
  name: "Grox",
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
    expect(document.body.textContent).toContain("扫描登录页");
    expect([...document.querySelectorAll<HTMLButtonElement>("button")].filter((item) => item.textContent?.includes("扫描登录页"))).toHaveLength(1);
    expect(document.body.textContent).not.toContain("忽略");
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
    await act(async () => button("扫描登录页").click());

    expect(document.body.textContent).toContain("扫描结果（1）");
    expect(document.body.textContent).toContain("新发现");
    expect(document.body.textContent).toContain("忽略");
    expect(document.querySelector('button[aria-label^="忽略："]')).toBeNull();
    expect(document.body.textContent).toContain("已确认入口（0）");

    await act(async () => button("确认添加").click());
    expect(document.body.textContent).toContain("已确认入口（1）");
    expect(document.body.textContent).toContain("已存在");
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
