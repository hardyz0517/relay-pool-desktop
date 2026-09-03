// @vitest-environment jsdom

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ChannelStatusRowView } from "../channelStatusViewModel";
import { ChannelStatusTable } from "./ChannelStatusTable";

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

describe("ChannelStatusTable", () => {
  it("allows a disabled monitor to be run manually", async () => {
    const onRunNow = vi.fn();
    const row = disabledMonitorRow();

    await act(async () => {
      root.render(
        <ChannelStatusTable
          rows={[row]}
          loading={false}
          actionPending={false}
          onRunNow={onRunNow}
          onCancel={vi.fn()}
          onOpenExecution={vi.fn()}
        />,
      );
    });

    const runButton = host.querySelector('button[aria-label="立即运行"]') as HTMLButtonElement;
    expect(runButton).toBeDefined();
    expect(runButton.disabled).toBe(false);

    await act(async () => {
      runButton.click();
    });
    expect(onRunNow).toHaveBeenCalledWith(row);
  });

  it("keeps long latency values and their units on one line", async () => {
    const row = {
      ...disabledMonitorRow(),
      latencyLabel: "45013 ms",
    };

    await act(async () => {
      root.render(
        <ChannelStatusTable
          rows={[row]}
          loading={false}
          actionPending={false}
          onRunNow={vi.fn()}
          onCancel={vi.fn()}
          onOpenExecution={vi.fn()}
        />,
      );
    });

    const latency = Array.from(host.querySelectorAll("tbody tr td div"))
      .find((element) => element.textContent === "45013 ms");
    expect(latency?.className).toContain("whitespace-nowrap");
    expect(latency?.className).toContain("tabular-nums");
  });

  it("uses the shell page as the vertical virtualizer viewport", async () => {
    host.setAttribute("data-shell-page-scroll-container", "");
    vi.spyOn(host, "getBoundingClientRect").mockReturnValue(viewportRect());
    vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(72);
    const rows = Array.from({ length: 100 }, (_, index) => ({
      ...disabledMonitorRow(),
      rowKey: `monitor-${index}:key-${index}`,
      monitorId: `monitor-${index}`,
      targetName: `密钥 ${index}`,
    }));

    await act(async () => {
      root.render(
        <ChannelStatusTable
          rows={rows}
          loading={false}
          actionPending={false}
          onRunNow={vi.fn()}
          onCancel={vi.fn()}
          onOpenExecution={vi.fn()}
        />,
      );
    });

    const horizontalScroll = host.querySelector<HTMLElement>(
      "[data-channel-status-horizontal-scroll]",
    );
    expect(horizontalScroll?.className).toContain("overflow-x-auto");
    expect(horizontalScroll?.className).not.toContain("overflow-auto");
    expect(horizontalScroll?.className).not.toContain("max-h-");

    const initialIndexes = renderedIndexes();
    expect(initialIndexes.length).toBeGreaterThan(0);
    expect(initialIndexes.length).toBeLessThan(rows.length);

    await act(async () => {
      host.scrollTop = 2_880;
      host.dispatchEvent(new Event("scroll"));
    });

    expect(Math.min(...renderedIndexes())).toBeGreaterThan(0);
  });
});

function renderedIndexes(): number[] {
  return Array.from(host.querySelectorAll<HTMLElement>("tbody tr[data-index]"))
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

function disabledMonitorRow(): ChannelStatusRowView {
  return {
    rowKey: "monitor-1:key-1",
    monitorId: "monitor-1",
    monitorName: "停用的监控",
    stationId: "station-1",
    enabled: false,
    balancePaused: false,
    targetName: "密钥 A",
    stationName: "Station A",
    stationKeyId: "key-1",
    keyName: "密钥 A",
    modelLabel: "gpt-4.1-mini",
    currentTone: "disabled",
    currentLabel: "停用",
    currentReason: null,
    latestProbeTone: "missing",
    availabilityPercent: null,
    availabilityLabel: "--",
    latencyMs: null,
    latencyLabel: "--",
    ttfbMs: null,
    ttfbLabel: "--",
    firstContentMs: null,
    firstContentLabel: "--",
    endpointPingMs: null,
    lastCheckedLabel: "尚未检查",
    lastCheckedAtMs: null,
    trend: [],
    recentTrend: [],
    runningExecutionId: null,
    latestExecutionId: null,
    dirty: false,
    corrupt: false,
    groupName: null,
    visualPlatform: "openai",
    visualPlatformLabel: "OpenAI",
    endpointPingLabel: "--",
  };
}
