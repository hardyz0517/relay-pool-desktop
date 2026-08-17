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
});

async function render({
  workspace,
  isLoading = false,
  isError = false,
  isRefreshing = false,
  isRefreshError = false,
  onRefresh = vi.fn().mockResolvedValue(undefined),
  onRetryWorkspace = vi.fn().mockResolvedValue(undefined),
}: {
  workspace?: StationPublishedStatusWorkspace;
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
