// @vitest-environment jsdom

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ChannelStatusTab } from "./ChannelStatusTab";

const useChannelStatusControllerMock = vi.hoisted(() => vi.fn());

vi.mock("./useChannelStatusController", () => ({
  useChannelStatusController: useChannelStatusControllerMock,
}));
vi.mock("./components/ChannelStatusToolbar", () => ({
  ChannelStatusToolbar: () => <div>toolbar</div>,
}));
vi.mock("./components/ChannelStatusTable", () => ({
  ChannelStatusTable: () => <div>results</div>,
}));
vi.mock("./components/ChannelStatusCardGrid", () => ({
  ChannelStatusCardGrid: () => <div>cards</div>,
}));
vi.mock("./components/MonitorExecutionDrawer", () => ({
  MonitorExecutionDrawer: () => null,
}));
vi.mock("@/lib/monitoringPerformance", () => ({
  recordMonitoringPerformance: vi.fn(),
}));

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean })
  .IS_REACT_ACT_ENVIRONMENT = true;

let host: HTMLDivElement;
let root: Root;

beforeEach(() => {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});

afterEach(() => {
  act(() => root.unmount());
  host.remove();
  vi.clearAllMocks();
});

describe("ChannelStatusTab", () => {
  it("keeps a successful background refresh silent", async () => {
    useChannelStatusControllerMock.mockReturnValue(controllerFixture({
      isFetching: true,
      dataUpdatedAt: Date.now(),
    }));

    await act(async () => {
      root.render(<ChannelStatusTab />);
    });

    expect(host.textContent).not.toContain("正在更新状态数据");
    expect(host.querySelector('[role="status"]')).toBeNull();
    expect(host.textContent).toContain("results");
  });
});

function controllerFixture({
  isFetching,
  dataUpdatedAt,
}: {
  isFetching: boolean;
  dataUpdatedAt: number;
}) {
  return {
    statusQuery: {
      data: {},
      dataUpdatedAt,
      error: null,
      isFetching,
      isPending: false,
    },
    workspaceView: { rows: [] },
    selectedExecutionId: null,
    setSelectedExecutionId: vi.fn(),
    isRunningAction: false,
    runNow: vi.fn(),
    cancel: vi.fn(),
    refresh: vi.fn(),
  };
}
