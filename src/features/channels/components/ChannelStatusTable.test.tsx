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
});

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
