// @vitest-environment jsdom

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ChannelStatusRowView } from "../channelStatusViewModel";
import { ChannelStatusCardGrid } from "./ChannelStatusCardGrid";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

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
  vi.restoreAllMocks();
});

describe("ChannelStatusCardGrid", () => {
  it("keeps hook order valid when data arrives after an empty state", async () => {
    await act(async () => {
      root.render(<ChannelStatusCardGrid rows={[]} loading />);
    });
    expect(host.textContent).toContain("正在读取状态监控");

    await act(async () => {
      root.render(<ChannelStatusCardGrid rows={[fixtureRow()]} loading={false} />);
    });

    expect(host.textContent).toContain("模型延迟");
    expect(host.textContent).toContain("端点 Ping");
    const grid = host.querySelector<HTMLElement>("[data-channel-status-card-grid]");
    expect(grid?.className).not.toContain("overflow-auto");
    expect(grid?.className).not.toContain("max-h-");
  });

  it("virtualizes long card grids against the shell page scroll surface", async () => {
    host.setAttribute("data-shell-page-scroll-container", "");
    vi.spyOn(host, "getBoundingClientRect").mockReturnValue(viewportRect());
    vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(250);
    const rows = Array.from({ length: 30 }, (_, index) => ({
      ...fixtureRow(),
      rowKey: `monitor-${index}|key-${index}`,
      monitorId: `monitor-${index}`,
      targetName: `密钥 ${index}`,
    }));

    await act(async () => {
      root.render(<ChannelStatusCardGrid rows={rows} loading={false} />);
    });

    const initialIndexes = renderedIndexes();
    expect(initialIndexes.length).toBeGreaterThan(0);
    expect(initialIndexes.length).toBeLessThan(rows.length);

    await act(async () => {
      host.scrollTop = 3_000;
      host.dispatchEvent(new Event("scroll"));
    });

    expect(Math.min(...renderedIndexes())).toBeGreaterThan(0);
  });
});

function renderedIndexes(): number[] {
  return Array.from(host.querySelectorAll<HTMLElement>("[data-index]"))
    .map((row) => Number(row.dataset.index));
}

function viewportRect(): DOMRect {
  return {
    x: 0,
    y: 0,
    top: 0,
    right: 1_200,
    bottom: 600,
    left: 0,
    width: 1_200,
    height: 600,
    toJSON: () => ({}),
  };
}

function fixtureRow(): ChannelStatusRowView {
  return {
    rowKey: "monitor-1|key-1",
    monitorId: "monitor-1",
    monitorName: "监控",
    stationId: "station-1",
    stationKeyId: "key-1",
    targetName: "密钥",
    stationName: "站点",
    keyName: "密钥",
    groupName: null,
    visualPlatform: "openai",
    visualPlatformLabel: "OpenAI",
    enabled: true,
    balancePaused: false,
    modelLabel: "gpt-test",
    currentTone: "available",
    latestProbeTone: "available",
    currentLabel: "正常",
    currentReason: null,
    runningExecutionId: null,
    latestExecutionId: null,
    availabilityPercent: 100,
    availabilityLabel: "100.00%",
    latencyMs: 100,
    latencyLabel: "100 ms",
    ttfbMs: 40,
    ttfbLabel: "40 ms",
    firstContentMs: 50,
    firstContentLabel: "50 ms",
    endpointPingMs: 20,
    endpointPingLabel: "20 ms",
    lastCheckedAtMs: 1,
    lastCheckedLabel: "01/01 00:00",
    recentTrend: [],
    trend: [],
    dirty: false,
    corrupt: false,
  };
}
