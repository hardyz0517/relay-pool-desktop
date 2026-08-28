// @vitest-environment jsdom

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { OfficialStatusTab } from "./OfficialStatusTab";

type MockController = { changePage: ReturnType<typeof vi.fn>; [key: string]: unknown };

const mocks = vi.hoisted(() => ({
  controller: null as MockController | null,
  stationQuery: { data: [{ id: "station-1", name: "示例站点" }] as Array<{ id: string; name: string }> | undefined, isError: false },
  useActivityQuery: vi.fn(),
}));

vi.mock("./useOfficialStatusController", () => ({
  OFFICIAL_STATUS_PAGE_SIZE_OPTIONS: [20, 50, 100],
  useOfficialStatusController: () => mocks.controller,
}));
vi.mock("@/lib/query/useActivityQuery", () => ({
  useActivityQuery: mocks.useActivityQuery,
}));

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let host: HTMLDivElement;
let root: Root;

function row(overrides: Record<string, unknown> = {}) {
  return {
    rowKey: "row-1", stationName: "示例站点", name: "OpenAI", provider: "openai", primaryModel: "gpt-test",
    groupName: "default", extraModels: [], currentLatencyMs: 1554, currentPingLatencyMs: 32,
    currentOutcome: "available", currentLabel: "正常", sourceState: "authorization_required", sourceStateLabel: "需要授权",
    stale: false, lastAttemptAtMs: 1700000000000,
    recentAvailabilityPercent: 98, availabilityLabel: "98.00%", lastCheckedLabel: "08/27 12:00", trend: [], ...overrides,
  };
}

function setupController(overrides: Record<string, unknown> = {}) {
  const refetch = vi.fn().mockResolvedValue({});
  mocks.controller = {
    filters: { search: "", stationId: "", outcome: "all", sourceState: "all" },
    setSearch: vi.fn(), setStationId: vi.fn(), setOutcome: vi.fn(), setSourceState: vi.fn(),
    page: 1, pageSize: 100,
    pageInfo: { currentPage: 1, totalPages: 2, startIndex: 1, endIndex: 1, total: 101 },
    changePage: vi.fn(), setPageSize: vi.fn(), paginationBusy: false, refresh: refetch,
    query: { data: { rows: [row()], page: { nextCursor: "cursor-2" }, summary: {}, readAtMs: 1 }, isPending: false, isFetching: false, error: null },
    view: { rows: [row()], nextCursor: "cursor-2", summary: {}, readAtMs: 1 },
    ...overrides,
  };
  mocks.useActivityQuery.mockImplementation(() => mocks.stationQuery);
  return refetch;
}

beforeEach(() => {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  mocks.stationQuery = { data: [{ id: "station-1", name: "示例站点" }], isError: false };
  setupController();
});

afterEach(() => {
  act(() => root.unmount());
  host.remove();
  vi.clearAllMocks();
});

function render() {
  act(() => root.render(<OfficialStatusTab />));
}

describe("OfficialStatusTab", () => {
  it("shows loading without claiming an empty result", () => {
    setupController({ query: { data: undefined, isPending: true, isFetching: true, error: null }, view: { rows: [], nextCursor: null, summary: {}, readAtMs: null } });
    render();
    expect(host.querySelector('[aria-label="加载官方状态"]')).not.toBeNull();
    expect(host.textContent).not.toContain("暂无官方状态");
    expect(host.querySelector("table")).toBeNull();
  });

  it("retains rows while showing a read error", () => {
    setupController({ query: { data: { rows: [row()], page: { nextCursor: null }, summary: {}, readAtMs: 1 }, isPending: false, isFetching: true, error: new Error("network") } });
    render();
    expect(host.textContent).toContain("官方状态读取失败");
    expect(host.textContent).toContain("示例站点");
    expect(host.querySelector('[aria-label="数据采集：需要授权"]')).not.toBeNull();
  });

  it("invokes pagination callbacks and keeps controls keyboard reachable", () => {
    render();
    const next = host.querySelector<HTMLButtonElement>('button[aria-label="下一页"]');
    expect(next?.disabled).toBe(false);
    act(() => next?.click());
    expect(mocks.controller?.changePage).toHaveBeenCalledWith(2);
    expect(host.querySelector<HTMLInputElement>('[aria-label="搜索官方状态"]')).not.toBeNull();
    expect(host.querySelector('nav[aria-label="官方状态分页"]')).not.toBeNull();
  });

  it("disables pagination after a failed page with no retained data", () => {
    setupController({
      query: { data: undefined, isPending: false, isFetching: false, error: new Error("page") },
      view: { rows: [], nextCursor: null, summary: {}, readAtMs: null },
      pageInfo: { currentPage: 1, totalPages: 1, startIndex: 0, endIndex: 0, total: 0 },
    });
    render();
    expect(host.textContent).toContain("官方状态读取失败");
    expect(host.querySelector('nav[aria-label="官方状态分页"]')).toBeNull();
  });

  it("refreshes only the overview read and does not expose collection controls", async () => {
    const refetch = setupController();
    render();
    const refresh = Array.from(host.querySelectorAll("button")).find((button) => button.textContent?.includes("刷新"));
    await act(async () => { refresh?.click(); });
    expect(refetch).toHaveBeenCalledTimes(1);
    expect(host.textContent).not.toContain("立即采集");
    expect(host.textContent).not.toContain("取消采集");
  });

  it("keeps monitor outcome and data collection filters unambiguous", () => {
    setupController();
    render();

    expect(host.querySelector<HTMLButtonElement>('button[aria-label="监控状态"]')?.textContent).toContain("全部监控状态");
    expect(host.querySelector<HTMLButtonElement>('button[aria-label="数据采集状态"]')?.textContent).toContain("全部数据采集");

    act(() => host.querySelector<HTMLButtonElement>('button[aria-label="监控状态"]')?.click());
    expect(document.querySelector('[role="listbox"][aria-label="监控状态"]')?.textContent).toContain("可用");
    expect(document.querySelector('[role="listbox"][aria-label="监控状态"]')?.textContent).not.toContain("采集正常");

    act(() => host.querySelector<HTMLButtonElement>('button[aria-label="数据采集状态"]')?.click());
    expect(document.querySelector('[role="listbox"][aria-label="数据采集状态"]')?.textContent).toContain("采集正常");
  });

  it("keeps collection issues secondary to the official status badge", () => {
    const partialRow = row({ sourceState: "degraded", sourceStateLabel: "部分解析" });
    setupController({
      query: { data: { rows: [partialRow], page: { nextCursor: null }, summary: {}, readAtMs: 1 }, isPending: false, isFetching: false, error: null },
      view: { rows: [partialRow], nextCursor: null, summary: {}, readAtMs: 1 },
    });
    render();

    expect(host.querySelector('[aria-label="数据采集：部分解析，已保留有效结果"]')).not.toBeNull();
    expect(host.textContent).not.toContain("采集部分解析");
    expect(host.querySelectorAll("tbody tr:first-child td:nth-child(3) [class*='rounded-full']")).toHaveLength(1);
  });

  it("uses the shared searchable select for station filtering", () => {
    mocks.stationQuery = {
      data: [
        { id: "station-1", name: "示例站点" },
        { id: "station-2", name: "备用站点" },
      ],
      isError: false,
    };
    render();

    act(() => host.querySelector<HTMLButtonElement>('button[aria-label="站点"]')?.click());

    const search = document.querySelector<HTMLInputElement>('input[aria-label="站点 搜索"]');
    expect(search).not.toBeNull();
    expect(search?.placeholder).toBe("搜索站点");
  });

  it("reports station catalog failure without hiding official rows", () => {
    mocks.stationQuery = { data: undefined, isError: true };
    render();
    expect(host.textContent).toContain("站点目录读取失败");
    expect(host.textContent).toContain("示例站点");
  });

  it("matches the local status table presentation", () => {
    render();

    const table = host.querySelector("table");
    expect(table?.className).toContain("table-fixed");
    expect(table?.className).toContain("min-w-[1040px]");
    expect(host.textContent).toContain("监控 / 站点");
    expect(host.textContent).toContain("最近检查");
    expect(host.textContent).toContain("趋势");
    expect(host.querySelector("[title='OpenAI · default'] svg")).not.toBeNull();
    expect(host.querySelector(".text-channel-availability")?.textContent).toBe("98.00%");
    expect(host.querySelector("[aria-label='官方状态：正常']")).not.toBeNull();
    expect(host.querySelector("[aria-label='OpenAI 的站点发布最近 60 次状态记录']")).not.toBeNull();
  });
});
