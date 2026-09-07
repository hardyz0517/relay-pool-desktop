// @vitest-environment jsdom

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { StationPublishedStatusSection } from "./StationPublishedStatusSection";
import type { StationPublishedStatusWorkspace } from "@/lib/types/stationPublishedStatus";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let host: HTMLDivElement;
let root: Root;

beforeEach(() => {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});

afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
});

describe("StationPublishedStatusSection", () => {
  it("keeps a stable loading region while the independent workspace is loading", async () => {
    await render({ workspace: undefined, isLoading: true });

    expect(host.textContent).toContain("官方渠道状态");
    expect(host.querySelector("[aria-label='正在读取站点发布的渠道状态']")).not.toBeNull();
    expect(host.querySelector("table")).toBeNull();
  });

  it.each([
    ["never_collected", "尚未采集官方渠道状态"],
    ["empty", "站点未发布监控"],
    ["unsupported", "当前站点不支持官方渠道状态"],
    ["authorization_required", "需要重新授权"],
    ["failed", "官方状态采集失败"],
  ] as const)("renders the %s source state without affecting other detail sections", async (sourceState, label) => {
    await render({ workspace: createWorkspace(sourceState) });

    expect(host.textContent).toContain(label);
    expect(host.textContent).toContain("官方渠道状态");
    expect(host.textContent).not.toContain("本地主动探针");
  });

  it("shows retained partial and stale rows with only official-source labels", async () => {
    await render({
      workspace: createWorkspace("degraded", {
        completeness: "partial",
        stale: true,
        rows: [createRow()],
      }),
    });

    expect(host.textContent).toContain("部分站点发布的监控记录未能解析");
    expect(host.textContent).toContain("Synthetic official monitor");
    expect(host.textContent).toContain("最近可用性");
    expect(host.textContent).toContain("99.50%");
    expect(host.textContent).not.toContain("7 日可用率");
    expect(host.textContent).toContain("官方更新时间：");
    expect(host.textContent).toContain("最近 60 次");
    expect(host.textContent).toContain("default");
    expect(host.textContent).not.toContain("openai");
    expect(host.querySelectorAll("[aria-label*='来源：站点发布']").length).toBeGreaterThan(0);
    const table = host.querySelector("table");
    expect(table?.className).toContain("min-w-[1000px]");
    expect(table?.parentElement?.className).toContain("overflow-x-auto");
    expect(Array.from(table?.querySelectorAll("th") ?? []).map((cell) => cell.textContent)).not.toContain("官方更新时间");
    expect(host.textContent).not.toContain("最近探测");
    expect(host.querySelector("[title='监控类型：OpenAI · openai']")).not.toBeNull();
  });

  it("uses the collected monitor provider before model text to select the platform icon", async () => {
    await render({
      workspace: createWorkspace("available", {
        rows: [createRow({ provider: "anthropic", primaryModel: "gpt-4o-mini" })],
      }),
    });

    expect(host.querySelector("[title='监控类型：Claude · anthropic']")).not.toBeNull();
    expect(host.querySelector("[title='监控类型：OpenAI · anthropic']")).toBeNull();
  });

  it("prioritizes an unsupported source state over retained historical rows", async () => {
    await render({ workspace: createWorkspace("unsupported", { rows: [createRow()] }) });

    expect(host.textContent).toContain("当前站点不支持官方渠道状态");
    expect(host.querySelector("table")).toBeNull();
    expect(host.textContent).not.toContain("Synthetic official monitor");
  });

  it("labels unavailable official outcomes as errors", async () => {
    await render({
      workspace: createWorkspace("available", {
        rows: [createRow({ currentOutcome: "unavailable" })],
      }),
    });

    expect(host.textContent).toContain("错误");
    expect(host.textContent).not.toContain("不可用");
  });

  it("keeps a workspace request failure local to this section and supports retry", async () => {
    const onRetryWorkspace = vi.fn().mockResolvedValue(undefined);
    const onRefresh = vi.fn().mockResolvedValue(undefined);
    await render({ workspace: undefined, isError: true, onRefresh, onRetryWorkspace });

    expect(host.textContent).toContain("暂时无法读取官方渠道状态");
    expect(host.textContent).toContain("详情页其他信息不受影响");
    const retry = Array.from(host.querySelectorAll("button")).find((button) => button.textContent?.includes("重试"));
    expect(retry).toBeDefined();
    await act(async () => retry!.dispatchEvent(new MouseEvent("click", { bubbles: true })));
    expect(onRetryWorkspace).toHaveBeenCalledTimes(1);
    expect(onRefresh).not.toHaveBeenCalled();
  });

  it("reports a failed latest workspace read while retaining cached official rows", async () => {
    await render({ workspace: createWorkspace("available", { rows: [createRow()] }), isError: true });

    expect(host.textContent).toContain("最新官方状态读取失败；正在显示上次读取的结果。");
    expect(host.textContent).toContain("Synthetic official monitor");
  });

  it("uses one expandable component for NewAPI model and group projections", async () => {
    const first = createRow({
      rowKey: "newapi-model-group-a",
      provider: "newapi",
      name: "gpt-5.5 · default",
      groupName: "default",
      primaryModel: "gpt-5.5",
      recentAvailabilityPercent: 100,
      currentLatencyMs: 22_688.13,
      currentTtftMs: 10_490,
      currentTps: 31.7,
    });
    const second = createRow({
      rowKey: "newapi-model-group-b",
      provider: "newapi",
      name: "gpt-5.5 · cheap",
      groupName: "cheap",
      primaryModel: "gpt-5.5",
      currentOutcome: "degraded",
      recentAvailabilityPercent: 83,
      currentLatencyMs: 587,
      currentTtftMs: null,
      currentTps: 1.08,
    });
    await render({ stationType: "newapi", workspace: createWorkspace("available", { rows: [first, second] }) });

    expect(host.querySelector("h2")?.textContent).toBe("官方渠道状态");
    expect(host.textContent).toContain("按模型");
    expect(host.textContent).toContain("按分组");
    const segmented = host.querySelector('[role="radiogroup"][aria-label="NewAPI 状态视图"]');
    const refreshButton = Array.from(host.querySelectorAll("button")).find((button) => button.textContent?.includes("重新采集"));
    expect(segmented).not.toBeNull();
    expect(refreshButton).toBeDefined();
    expect(segmented?.parentElement).toBe(refreshButton?.parentElement);
    expect(Boolean(segmented && refreshButton && (segmented.compareDocumentPosition(refreshButton) & Node.DOCUMENT_POSITION_FOLLOWING))).toBe(true);
    expect(host.textContent).toContain("gpt-5.5");
    expect(host.textContent).toContain("2 个分组");
    expect(host.textContent).toContain("1 正常 · 1 欠佳");
    expect(host.textContent).toContain("91.50%");
    expect(host.textContent).toContain("10.5 s");
    expect(host.textContent).toContain("16.4");
    expect(host.textContent).not.toContain("NewAPI 请求性能");
    expect(Array.from(host.querySelectorAll("thead th")).map((cell) => cell.textContent)).toEqual([
      "模型", "状态概览", "最近可用性", "首响 / TPS", "最近 60 次",
    ]);
    const modelTrend = host.querySelector('[aria-label="gpt-5.5 最近 60 个性能 bucket"]');
    expect(modelTrend?.className).toContain("grid w-full");
    expect(host.textContent).not.toContain("gpt-5.5 · default");
    const modelRow = Array.from(host.querySelectorAll("tr[aria-expanded]"))
      .find((row) => row.textContent?.includes("gpt-5.5") && row.textContent?.includes("2 个分组"));
    expect(modelRow).toBeDefined();
    const modelCells = modelRow!.querySelectorAll("td");
    expect(modelCells).toHaveLength(5);
    expect(modelCells[3]?.textContent).toContain("10.5 s");
    expect(modelCells[3]?.textContent).toContain("TPS 16.4");
    await act(async () => modelRow!.dispatchEvent(new MouseEvent("click", { bubbles: true })));
    expect(host.textContent).toContain("default");
    expect(host.textContent).not.toContain("22.7 s");
    expect(host.textContent).not.toContain("587 ms");
    expect(host.textContent).toContain("1.08");
    expect(host.querySelectorAll("tbody tr")).toHaveLength(3);
    const childMetricCells = Array.from(host.querySelectorAll("tbody tr:not([aria-expanded])"))
      .map((row) => row.querySelectorAll("td")[3]);
    expect(childMetricCells.some((cell) => cell?.textContent?.includes("TPS 1.08"))).toBe(true);
    expect(childMetricCells.some((cell) => cell?.textContent?.includes("10.5 s") && cell.textContent.includes("TPS 31.7"))).toBe(true);
    const groupTab = Array.from(host.querySelectorAll("button")).find((button) => button.textContent === "按分组");
    await act(async () => groupTab!.dispatchEvent(new MouseEvent("click", { bubbles: true })));
    expect(Array.from(host.querySelectorAll("thead th")).map((cell) => cell.textContent)?.[0]).toBe("分组");
    expect(host.textContent).toContain("default");
    expect(host.textContent).toContain("cheap");
  });

  it("keeps long NewAPI child names bounded and formats missing performance metrics", async () => {
    const longGroupName = "某某专线/开发者专用/全模型高倍率渠道/名称很长但不应挤压指标列";
    await render({
      stationType: "newapi",
      workspace: createWorkspace("available", {
        rows: [createRow({
          rowKey: "newapi-long-group",
          provider: "newapi",
          name: `gpt-6-astra · ${longGroupName}`,
          groupName: longGroupName,
          primaryModel: "gpt-6-astra",
          currentOutcome: "unavailable",
          recentAvailabilityPercent: 0,
          currentLatencyMs: 2_000,
          currentTtftMs: null,
          currentTps: null,
        })],
      }),
    });

    const parentRow = host.querySelector("tbody tr[aria-expanded]");
    await act(async () => parentRow!.dispatchEvent(new MouseEvent("click", { bubbles: true })));
    const childRow = host.querySelectorAll("tbody tr")[1];
    expect(childRow?.textContent).toContain("0.00%");
    expect(childRow?.textContent).not.toContain("2.0 s");
    expect(childRow?.textContent?.match(/—/g)).toHaveLength(2);
    expect(childRow?.querySelector(`[title="${longGroupName}"]`)).not.toBeNull();
    expect(childRow?.querySelector(".truncate")).not.toBeNull();
  });

  it("keeps the existing Sub2API table structure and hides the NewAPI projection control", async () => {
    await render({
      workspace: createWorkspace("available", {
        rows: [createRow({ currentLatencyMs: 1_160, currentPingLatencyMs: 6 })],
      }),
    });

    expect(host.querySelector('[aria-label="NewAPI 状态视图"]')).toBeNull();
    expect(Array.from(host.querySelectorAll("thead th")).map((cell) => cell.textContent)).toEqual([
      "监控 / 分组", "模型", "当前状态", "最近可用性", "首响 / Ping", "最近 60 次",
    ]);
    const metricCell = host.querySelectorAll("tbody td")[4];
    expect(metricCell?.textContent).toContain("1.2 s");
    expect(metricCell?.textContent).toContain("Ping 6 ms");
    expect(host.textContent).not.toContain("延迟 / Ping");
  });
});

async function render({
  workspace,
  stationType = "sub2api",
  isLoading = false,
  isError = false,
  isRefreshing = false,
  isRefreshError = false,
  onRefresh = vi.fn().mockResolvedValue(undefined),
  onRetryWorkspace = vi.fn().mockResolvedValue(undefined),
}: {
  workspace?: StationPublishedStatusWorkspace;
  stationType?: string;
  isLoading?: boolean;
  isError?: boolean;
  isRefreshing?: boolean;
  isRefreshError?: boolean;
  onRefresh?: () => Promise<void>;
  onRetryWorkspace?: () => Promise<void>;
}) {
  await act(async () => {
    root.render(
      <StationPublishedStatusSection
        stationName="Fixture Station"
        stationType={stationType}
        workspace={workspace}
        isLoading={isLoading}
        isError={isError}
        isRefreshing={isRefreshing}
          isRefreshError={isRefreshError}
          onRefresh={onRefresh}
          onRetryWorkspace={onRetryWorkspace}
      />,
    );
  });
}

function createWorkspace(
  sourceState: StationPublishedStatusWorkspace["sourceState"],
  overrides: Partial<StationPublishedStatusWorkspace> = {},
): StationPublishedStatusWorkspace {
  return {
    stationId: "station-1",
    endpointRevision: 1,
    supported: true,
    sourceState,
    completeness: "complete",
    lastAttemptAtMs: 1_700_000_000_000,
    lastSuccessAtMs: 1_700_000_000_000,
    lastCompleteAtMs: 1_700_000_000_000,
    monitorCount: 0,
    stale: false,
    safeErrorKind: null,
    rows: [],
    ...overrides,
  };
}

function createRow(
  overrides: Partial<StationPublishedStatusWorkspace["rows"][number]> = {},
): StationPublishedStatusWorkspace["rows"][number] {
  return {
    rowKey: "row-1",
    upstreamMonitorId: "monitor-1",
    identityKind: "upstream_id",
    name: "Synthetic official monitor",
    provider: "openai",
    groupName: "default",
    primaryModel: "gpt-4o-mini",
    extraModels: ["gpt-4.1-mini"],
    currentOutcome: "available",
    currentLatencyMs: 120,
    currentPingLatencyMs: 18,
    recentAvailabilityPercent: 99.5,
    upstreamCheckedAtMs: 1_700_000_000_000,
    recentSamples: [
      {
        id: "sample-1",
        model: "gpt-4o-mini",
        outcome: "available",
        latencyMs: 120,
        pingLatencyMs: 18,
        checkedAtMs: 1_700_000_000_000,
      },
    ],
    ...overrides,
  };
}
