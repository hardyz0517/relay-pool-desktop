// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ToastProvider } from "@/components/ui";
import { BackendError } from "@/lib/bridge/errors";
import type { RoutingPolicyConfigV2 } from "@/lib/types/routing";
import { LocalRoutingSettingsEditor } from "./LocalRoutingSettingsEditor";
import { createDefaultRoutingPolicyConfig } from "./useRoutingPolicyDraft";

const mocks = vi.hoisted(() => ({
  load: vi.fn(),
  protection: vi.fn(),
  policyData: null as unknown,
  protectionData: null as unknown,
  policyQuery: null as unknown,
  protectionQuery: null as unknown,
  apply: vi.fn(),
  refresh: vi.fn(),
}));

vi.mock("@/lib/api/routing", () => ({
  loadRoutingPolicy: mocks.load,
  getRoutingProtectionStatus: mocks.protection,
  applyRoutingPolicyDocument: mocks.apply,
}));

vi.mock("@/lib/query/routingQuerySynchronization", () => ({
  refreshRoutingQueries: mocks.refresh,
}));

vi.mock("@/lib/query/useActivityQuery", () => ({
  useActivityQuery: (options: { queryKey?: readonly unknown[] }) => {
    if (options.queryKey?.[1] === "policy") {
      return mocks.policyQuery ?? { data: mocks.policyData, isPending: false, error: null, refetch: vi.fn() };
    }
    if (options.queryKey?.[1] === "protectionStatus") {
      return mocks.protectionQuery ?? { data: mocks.protectionData, isPending: false, error: null, refetch: vi.fn() };
    }
    return {
      data: { collectorProxyMode: "system" },
    isPending: false,
    error: null,
    refetch: vi.fn(),
    };
  },
}));

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

afterEach(() => {
  document.body.innerHTML = "";
  mocks.load.mockReset();
  mocks.protection.mockReset();
  mocks.policyData = null;
  mocks.protectionData = null;
  mocks.policyQuery = null;
  mocks.protectionQuery = null;
  mocks.apply.mockReset();
  mocks.refresh.mockReset();
  vi.restoreAllMocks();
});

describe("LocalRoutingSettingsEditor", () => {
  it("shows percentage presets and normalizes manual weight changes", async () => {
    const config = policyConfig();
    mocks.load.mockResolvedValue({ config, revision: 4 });
    mocks.protection.mockResolvedValue({ statusVersion: "routing_protection_status_v1", generatedAtMs: 1, entries: [], readModelStatus: "available" });
    mocks.policyData = { config, revision: 4, policyVersion: "routing-policy-v2", systemVersion: "routing-system-v1", status: "active", updatedAtMs: 1 };
    mocks.protectionData = await mocks.protection();
    mocks.apply.mockImplementation(async (input: { policy: RoutingPolicyConfigV2 }) => ({ config: input.policy, revision: 5 }));
    mocks.refresh.mockResolvedValue({ refreshed: true, errors: [] });
    const { host, root, queryClient } = renderEditor();

    await act(async () => Promise.resolve());
    expect(value(host, "可靠性（%）")).toBe("40");
    expect(host.querySelector('[aria-label="可靠性（%）"]')?.className).toContain("text-center");
    expect(value(host, "响应速度（%）")).toBe("25");
    expect(host.textContent).toContain("评分偏好");
    expect(host.textContent).not.toContain("权重合计");

    await act(async () => findButton(host, "稳定优先")?.click());
    expect(value(host, "可靠性（%）")).toBe("50");
    expect(value(host, "响应速度（%）")).toBe("25");
    expect(value(host, "成本（%）")).toBe("15");
    expect(value(host, "偏好（%）")).toBe("10");

    setInput(host, '[aria-label="可靠性（%）"]', "60");
    expect(value(host, "可靠性（%）")).toBe("60");
    await act(async () => findButton(host, "保存")?.click());
    const saved = mocks.apply.mock.calls[0][0].policy as RoutingPolicyConfigV2;
    expect(saved.reliabilityWeight + saved.responsivenessWeight + saved.costWeight + saved.preferenceWeight).toBe(10_000);
    expect(saved.reliabilityWeight).toBe(6_000);

    await act(async () => root.unmount());
    queryClient.clear();
  });

  it("places measurement units in field labels instead of input suffixes", async () => {
    const config = policyConfig();
    config.affinityEnabled = true;
    mocks.load.mockResolvedValue({ config, revision: 4 });
    mocks.protection.mockResolvedValue({ statusVersion: "routing_protection_status_v1", generatedAtMs: 1, entries: [], readModelStatus: "available" });
    mocks.policyData = policySnapshot(config, 4);
    mocks.protectionData = await mocks.protection();
    const { host, root, queryClient } = renderEditor();

    await act(async () => Promise.resolve());
    for (const label of [
      "倍率上限（倍）",
      "可靠性（%）",
      "探索比例（%）",
      "连接超时（秒）",
      "统计窗口时长（秒）",
      "失败率阈值（%）",
      "亲和时长（秒）",
      "容量重试总等待预算（秒）",
    ]) {
      expect(host.textContent).toContain(label);
    }
    expect(host.querySelector('[aria-label="连接超时（秒）"]')?.parentElement?.textContent).toBe("");
    expect(host.querySelector('[aria-label="失败率阈值（%）"]')?.parentElement?.textContent).toBe("");

    await act(async () => root.unmount());
    queryClient.clear();
  });

  it("groups strategy sections with subtle surfaces instead of dividers", async () => {
    const config = policyConfig();
    mocks.policyData = policySnapshot(config, 4);
    mocks.protectionData = availableProtection();
    const { host, root, queryClient } = renderEditor();

    await act(async () => Promise.resolve());
    const strategyCard = Array.from(host.querySelectorAll("section")).find(
      (section) => section.querySelector("h2")?.textContent === "策略配置",
    );
    const blocks = Array.from(strategyCard?.querySelectorAll('section[aria-labelledby^="routing-policy-"]') ?? []);

    expect(strategyCard?.querySelector(".divide-y")).toBeNull();
    expect(blocks.length).toBeGreaterThan(0);
    expect(blocks.every((block) => block.className.includes("bg-surface-subtle") && block.className.includes("rounded-[var(--surface-radius)]"))).toBe(true);

    await act(async () => root.unmount());
    queryClient.clear();
  });

  it("converts exploration percentage and only shows affinity duration when enabled", async () => {
    const config = policyConfig();
    config.affinityEnabled = true;
    mocks.load.mockResolvedValue({ config, revision: 4 });
    mocks.protection.mockResolvedValue({ statusVersion: "routing_protection_status_v1", generatedAtMs: 1, entries: [], readModelStatus: "available" });
    mocks.policyData = { config, revision: 4, policyVersion: "routing-policy-v2", systemVersion: "routing-system-v1", status: "active", updatedAtMs: 1 };
    mocks.protectionData = await mocks.protection();
    mocks.apply.mockImplementation(async (input: { policy: RoutingPolicyConfigV2 }) => ({ config: input.policy, revision: 5 }));
    mocks.refresh.mockResolvedValue({ refreshed: true, errors: [] });
    const { host, root, queryClient } = renderEditor();

    await act(async () => Promise.resolve());
    expect(value(host, "探索比例（%）")).toBe("5");
    expect(host.querySelector('[aria-label="亲和时长（秒）"]')).toBeTruthy();

    setInput(host, '[aria-label="探索比例（%）"]', "7.25");
    await act(async () => findButton(host, "保存")?.click());
    const saved = mocks.apply.mock.calls[0][0].policy as RoutingPolicyConfigV2;
    expect(saved.explorationShareBasisPoints).toBe(725);

    await act(async () => root.unmount());
    queryClient.clear();
  });

  it("edits the four retry/failover fields as one policy document", async () => {
    const config = policyConfig();
    mocks.load.mockResolvedValue({ config, revision: 4 });
    mocks.protectionData = { statusVersion: "routing_protection_status_v1", generatedAtMs: 1, entries: [], readModelStatus: "available" };
    mocks.policyData = { config, revision: 4, policyVersion: "routing-policy-v2", systemVersion: "routing-system-v1", status: "active", updatedAtMs: 1 };
    mocks.apply.mockImplementation(async (input: { policy: RoutingPolicyConfigV2 }) => ({ config: input.policy, revision: 5, policyVersion: "routing-policy-v2", systemVersion: "routing-system-v1", status: "active", updatedAtMs: 2 }));
    const { host, root, queryClient } = renderEditor();

    await act(async () => Promise.resolve());
    setInput(host, '[aria-label="单个请求最大尝试次数"]', "3");
    setInput(host, '[aria-label="同目标容量重试次数"]', "1");
    setInput(host, '[aria-label="容量重试总等待预算（秒）"]', "0.75");
    await act(async () => (host.querySelector('[aria-label="允许跨容量域回退"]') as HTMLButtonElement)?.click());
    await act(async () => findButton(host, "保存")?.click());

    const saved = mocks.apply.mock.calls[0][0].policy as RoutingPolicyConfigV2;
    expect(saved.retryFailover).toEqual({
      version: 2,
      maxTotalAttempts: 3,
      maxSameTargetCapacityRetries: 1,
      capacityRetryWaitBudgetSeconds: 0.75,
      allowCrossCapacityDomainFallback: false,
    });

    await act(async () => root.unmount());
    queryClient.clear();
  });

  it("edits proxy timeout fields as part of the routing policy document", async () => {
    const config = policyConfig();
    mocks.policyData = policySnapshot(config, 4);
    mocks.protectionData = availableProtection();
    mocks.apply.mockImplementation(async (input: { policy: RoutingPolicyConfigV2 }) => ({ config: input.policy, revision: 5, policyVersion: "routing-policy-v2", systemVersion: "routing-system-v1", status: "active", updatedAtMs: 2 }));
    const { host, root, queryClient } = renderEditor();

    await act(async () => Promise.resolve());
    setInput(host, '[aria-label="连接超时（秒）"]', "15");
    setInput(host, '[aria-label="缓冲执行超时（秒）"]', "600");
    await act(async () => findButton(host, "保存")?.click());

    const saved = mocks.apply.mock.calls[0][0].policy as RoutingPolicyConfigV2;
    expect(saved.timeoutPolicy).toEqual({
      version: 2,
      connectSeconds: 15,
      firstByteSeconds: 30,
      precommitSeconds: 60,
      bufferedExecutionSeconds: 600,
      streamIdleSeconds: 90,
    });

    await act(async () => root.unmount());
    queryClient.clear();
  });

  it("does not expose runtime protection status in policy settings", async () => {
    const config = policyConfig();
    mocks.load.mockResolvedValue({ config, revision: 4 });
    mocks.policyData = { config, revision: 4, policyVersion: "routing-policy-v2", systemVersion: "routing-system-v1", status: "active", updatedAtMs: 1 };
    mocks.protectionData = {
      statusVersion: "routing_protection_status_v1",
      generatedAtMs: 1,
      readModelStatus: "available",
      timeouts: {
        connectSeconds: 1,
        firstByteSeconds: 2,
        precommitSeconds: 3,
        bufferedExecutionSeconds: 4,
        streamIdleSeconds: 5,
        owner: "proxy.runtime.v1",
      },
      entries: [
        { scope: "key-a", scopeKind: "station_key", state: "cooldown", explanationKey: "routing.protection.cooldown", persistenceKind: "durable", cooldownUntilMs: 2_000, cooldownRemainingMs: 1_000, recentFailureCode: "upstream_overloaded", updatedAtMs: 1_000, detailAvailable: true },
        { scope: "capacity-domain-a", scopeKind: "capacity_domain", state: "half_open", explanationKey: "routing.protection.half_open", persistenceKind: "runtime_capacity", cooldownUntilMs: null, cooldownRemainingMs: null, recentFailureCode: "capacity_exhausted", updatedAtMs: 1_000, detailAvailable: true },
      ],
    };
    const { host, root, queryClient } = renderEditor();

    await act(async () => Promise.resolve());
    expect(host.textContent).not.toContain("故障保护");
    expect(host.textContent).not.toContain("持久化健康状态");
    expect(host.textContent).not.toContain("进程内容量状态");
    expect(host.textContent).not.toContain("容量探测中");
    expect(host.textContent).not.toContain("上游过载");
    expect(host.textContent).toContain("错误率保护参数");
    expect(host.textContent).not.toContain("当前运行实例仍为");
    expect(host.textContent).not.toContain("proxy.runtime.v1");

    await act(async () => root.unmount());
    queryClient.clear();
  });

  it("does not expose legacy compatibility snapshots in the protection settings", async () => {
    const config = policyConfig();
    mocks.policyData = policySnapshot(config, 4);
    mocks.protectionData = {
      statusVersion: "routing_protection_status_v1",
      generatedAtMs: 1,
      readModelStatus: "available",
      entries: [
        { scope: "legacy_station_key:v1:hash", scopeKind: "legacy_station_key", state: "degraded", explanationKey: "routing.protection.legacy_degraded", persistenceKind: "legacy_compatibility", cooldownUntilMs: null, cooldownRemainingMs: null, recentFailureCode: "legacy_failure", updatedAtMs: 1_000, detailAvailable: true },
      ],
    };
    const { host, root, queryClient } = renderEditor();

    await act(async () => Promise.resolve());
    expect(host.textContent).not.toContain("故障保护");
    expect(host.textContent).not.toContain("legacy_station_key");
    expect(host.textContent).not.toContain("兼容健康状态");
    expect(host.textContent).not.toContain("legacy_failure");

    await act(async () => root.unmount());
    queryClient.clear();
  });

  it("does not expose unavailable runtime capacity placeholders", async () => {
    const config = policyConfig();
    mocks.policyData = policySnapshot(config, 4);
    mocks.protectionData = {
      statusVersion: "routing_protection_status_v1",
      generatedAtMs: 1,
      readModelStatus: "available",
      entries: [
        { scope: "runtime_capacity", scopeKind: "capacity_domain", state: "unavailable", explanationKey: "routing.protection.unavailable", persistenceKind: "runtime_capacity", cooldownUntilMs: null, cooldownRemainingMs: null, recentFailureCode: null, updatedAtMs: null, detailAvailable: false },
      ],
    };
    const { host, root, queryClient } = renderEditor();

    await act(async () => Promise.resolve());
    expect(host.textContent).not.toContain("故障保护");
    expect(host.textContent).not.toContain("runtime_capacity");
    expect(host.textContent).not.toContain("进程内容量状态");
    expect(host.textContent).not.toContain("明细不可用");

    await act(async () => root.unmount());
    queryClient.clear();
  });

  it("explains closed monitoring separately from an empty protection view", async () => {
    const config = policyConfig();
    mocks.policyData = policySnapshot(config, 4);
    mocks.protectionData = {
      statusVersion: "routing_protection_status_v1",
      generatedAtMs: 1,
      readModelStatus: "available",
      entries: [
        { scope: "endpoint-a", scopeKind: "endpoint", state: "degraded", explanationKey: "routing.protection.closed_monitoring", persistenceKind: "durable", cooldownUntilMs: null, cooldownRemainingMs: null, recentFailureCode: "upstream_5xx", updatedAtMs: 1_000, detailAvailable: true },
        { scope: "routing", scopeKind: null, state: "no_protection", explanationKey: "routing.protection.none_active", persistenceKind: null, cooldownUntilMs: null, cooldownRemainingMs: null, recentFailureCode: null, updatedAtMs: 1_000, detailAvailable: true },
      ],
    };
    const { host, root, queryClient } = renderEditor();

    await act(async () => Promise.resolve());
    expect(host.textContent).not.toContain("故障保护");
    expect(host.textContent).not.toContain("当前未打开保护、仍在监控");
    expect(host.textContent).not.toContain("保护已打开，暂时抑制候选");

    await act(async () => root.unmount());
    queryClient.clear();
  });

  it("renders typed backend validation errors on the matching retry field", async () => {
    const config = policyConfig();
    mocks.policyData = policySnapshot(config, 4);
    mocks.protectionData = availableProtection();
    mocks.apply.mockRejectedValue(new BackendError(
      "invalid_input",
      "策略验证失败",
      false,
      {
        kind: "validation",
        fields: [{
          field: "retryFailover.capacityRetryWaitBudgetSeconds",
          code: "out_of_range",
          message: "等待预算超出允许范围",
        }],
      },
    ));
    const { host, root, queryClient } = renderEditor();

    await act(async () => Promise.resolve());
    setInput(host, '[aria-label="容量重试总等待预算（秒）"]', "2500");
    await act(async () => findButton(host, "保存")?.click());

    const field = host.querySelector('[aria-label="容量重试总等待预算（秒）"]') as HTMLInputElement;
    expect(field.getAttribute("aria-invalid")).toBe("true");
    expect(field.getAttribute("aria-describedby")).toBe("routing-error-wait-budget");
    expect(host.querySelector("#routing-error-wait-budget")?.textContent).toContain("等待预算超出允许范围");
    expect(host.textContent).toContain("策略验证失败");

    await act(async () => root.unmount());
    queryClient.clear();
  });

  it("renders typed backend validation errors on protection profile fields", async () => {
    const config = policyConfig();
    mocks.policyData = policySnapshot(config, 4);
    mocks.protectionData = availableProtection();
    mocks.apply.mockRejectedValue(new BackendError(
      "invalid_input",
      "保护参数验证失败",
      false,
      {
        kind: "validation",
        fields: [{
          field: "protectionProfile.failureThresholdPercent",
          code: "out_of_range",
          message: "失败率阈值必须在 1 到 100 之间",
        }],
      },
    ));
    const { host, root, queryClient } = renderEditor();

    await act(async () => Promise.resolve());
    setInput(host, '[aria-label="失败率阈值（%）"]', "101");
    await act(async () => findButton(host, "保存")?.click());

    const field = host.querySelector('[aria-label="失败率阈值（%）"]') as HTMLInputElement;
    expect(field.getAttribute("aria-invalid")).toBe("true");
    expect(field.getAttribute("aria-describedby")).toBe("routing-error-protection-failure-threshold");
    expect(host.querySelector("#routing-error-protection-failure-threshold")?.textContent).toContain("失败率阈值必须在 1 到 100 之间");

    await act(async () => root.unmount());
    queryClient.clear();
  });

  it("keeps the local draft visible and offers explicit choices after a CAS conflict", async () => {
    const config = policyConfig();
    mocks.policyData = policySnapshot(config, 4);
    mocks.protectionData = availableProtection();
    const refetch = vi.fn().mockResolvedValue(undefined);
    mocks.policyQuery = { data: mocks.policyData, isPending: false, error: null, refetch };
    mocks.apply.mockRejectedValue(new BackendError(
      "conflict",
      "策略已被更新",
      false,
      { kind: "conflict", resource: "routing_policy", currentRevision: "5" },
    ));
    const { host, root, queryClient } = renderEditor();

    await act(async () => Promise.resolve());
    setInput(host, '[aria-label="最大候选数"]', "32");
    await act(async () => findButton(host, "保存")?.click());

    expect(refetch).toHaveBeenCalled();
    expect(host.textContent).toContain("策略保存冲突");
    expect(findButton(host, "重新加载")).toBeTruthy();
    expect(findButton(host, "合并远端")).toBeTruthy();
    expect(findButton(host, "覆盖远端")).toBeTruthy();
    expect(value(host, "最大候选数")).toBe("32");

    await act(async () => root.unmount());
    queryClient.clear();
  });

  it("detects external document changes and merges only untouched fields", async () => {
    const config = policyConfig();
    mocks.policyData = policySnapshot(config, 4);
    mocks.protectionData = availableProtection();
    const { host, root, queryClient } = renderEditor();

    await act(async () => Promise.resolve());
    setInput(host, '[aria-label="最大候选数"]', "32");
    mocks.policyData = policySnapshot({ ...config, maxCandidates: 128, explorationShareBasisPoints: 900 }, 5);
    await act(async () => {
      root.render(
        <QueryClientProvider client={queryClient}>
          <ToastProvider><LocalRoutingSettingsEditor /></ToastProvider>
        </QueryClientProvider>,
      );
      await Promise.resolve();
    });

    expect(host.textContent).toContain("策略已被其他操作更新");
    expect(value(host, "最大候选数")).toBe("32");
    await act(async () => findButton(host, "合并远端")?.click());
    expect(value(host, "最大候选数")).toBe("32");
    expect(value(host, "探索比例（%）")).toBe("9");
    expect(findButton(host, "保存")?.disabled).toBe(false);

    await act(async () => root.unmount());
    queryClient.clear();
  });

  it("fails closed for an invalid policy document and exposes reload", async () => {
    const refetch = vi.fn().mockResolvedValue(undefined);
    mocks.policyQuery = {
      data: null,
      isPending: false,
      error: new BackendError("invalid_input", "受管策略文档无效"),
      refetch,
    };
    const { host, root, queryClient } = renderEditor();

    await act(async () => Promise.resolve());
    expect(host.textContent).toContain("受管策略文档无效");
    const reload = findButton(host, "重新加载");
    expect(reload?.disabled).toBe(false);
    await act(async () => reload?.click());
    expect(refetch).toHaveBeenCalled();

    await act(async () => root.unmount());
    queryClient.clear();
  });

  it("keeps policy editing available while protection facts are unavailable", async () => {
    const config = policyConfig();
    mocks.policyData = policySnapshot(config, 4);
    mocks.protectionData = {
      statusVersion: "routing_protection_status_v1",
      generatedAtMs: 1,
      entries: [],
      readModelStatus: "unavailable" as const,
    };
    const { host, root, queryClient } = renderEditor();

    await act(async () => Promise.resolve());
    expect(host.textContent).not.toContain("保护状态资料不可用");
    expect(host.textContent).toContain("运行时超时事实暂不可用");
    expect(findButton(host, "保存")?.disabled).toBe(true);
    setInput(host, '[aria-label="最大候选数"]', "32");
    expect(findButton(host, "保存")?.disabled).toBe(false);

    await act(async () => root.unmount());
    queryClient.clear();
  });

  it("resets the draft to the default policy", async () => {
    const config = { ...policyConfig(), maxCandidates: 128, affinityEnabled: true, outboundProxyMode: "manual" as const, outboundProxyUrl: "http://127.0.0.1:7890" };
    config.retryFailover = { ...config.retryFailover, maxTotalAttempts: 2, capacityRetryWaitBudgetSeconds: 0.5, allowCrossCapacityDomainFallback: false };
    mocks.policyData = policySnapshot(config, 4);
    mocks.protectionData = availableProtection();
    const { host, root, queryClient } = renderEditor();

    await act(async () => Promise.resolve());
    await act(async () => findButton(host, "重置")?.click());
    expect(value(host, "最大候选数")).toBe("64");
    expect(value(host, "单个请求最大尝试次数")).toBe("4");
    expect(value(host, "容量重试总等待预算（秒）")).toBe("2");
    expect(host.querySelector('[aria-label="本地路由手动代理地址"]')).toBeNull();
    expect(findButton(host, "保存")?.disabled).toBe(false);

    await act(async () => root.unmount());
    queryClient.clear();
  });

  it("keeps outbound proxy settings in their own module", async () => {
    const config = policyConfig();
    mocks.policyData = policySnapshot(config, 4);
    mocks.protectionData = availableProtection();
    const { host, root, queryClient } = renderEditor();

    await act(async () => Promise.resolve());
    const strategyCard = Array.from(host.querySelectorAll("section")).find(
      (section) => section.querySelector("h2")?.textContent === "策略配置",
    );
    const proxyCard = Array.from(host.querySelectorAll("section")).find(
      (section) => section.querySelector("h2")?.textContent === "出站代理",
    );

    expect(strategyCard?.querySelector('[aria-label="本地路由出站代理"]')).toBeNull();
    const proxySelect = proxyCard?.querySelector('[aria-label="本地路由出站代理"]');
    expect(proxySelect).toBeTruthy();
    expect(proxySelect?.textContent).toContain("继承全局设置（使用系统代理）");
    expect(proxyCard?.textContent).not.toContain("当前使用：");
    expect(proxyCard?.querySelector("h2")?.textContent).toBe("出站代理");

    await act(async () => root.unmount());
    queryClient.clear();
  });

  it("keeps clean actions disabled and retry controls keyboard-focusable in a narrow layout", async () => {
    const config = policyConfig();
    mocks.policyData = policySnapshot(config, 4);
    mocks.protectionData = availableProtection();
    const { host, root, queryClient } = renderEditor();

    await act(async () => Promise.resolve());
    expect(findButton(host, "保存")?.disabled).toBe(true);
    expect(findButton(host, "撤销")).toBeUndefined();
    expect(host.textContent).not.toContain("当前生效 revision");
    expect(host.textContent).not.toContain("只影响后续请求");
    expect(Array.from(host.querySelectorAll("footer button")).map((button) => button.textContent?.trim())).toEqual(["重置", "保存"]);
    const retryInput = host.querySelector('[aria-label="单个请求最大尝试次数"]') as HTMLInputElement;
    const fallbackSwitch = host.querySelector('[aria-label="允许跨容量域回退"]') as HTMLButtonElement;
    retryInput.focus();
    expect(document.activeElement).toBe(retryInput);
    fallbackSwitch.focus();
    expect(document.activeElement).toBe(fallbackSwitch);
    expect(host.querySelector(".sm\\:grid-cols-2")).toBeTruthy();

    await act(async () => root.unmount());
    queryClient.clear();
  });

  it("does not create a revision-worthy draft when defaults are already active", async () => {
    const config = createDefaultRoutingPolicyConfig();
    mocks.policyData = policySnapshot(config, 4);
    mocks.protectionData = availableProtection();
    const { host, root, queryClient } = renderEditor();

    await act(async () => Promise.resolve());
    await act(async () => findButton(host, "重置")?.click());
    expect(findButton(host, "保存")?.disabled).toBe(true);
    expect(findButton(host, "撤销")).toBeUndefined();
    expect(mocks.apply).not.toHaveBeenCalled();

    await act(async () => root.unmount());
    queryClient.clear();
  });
});

function renderEditor() {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const host = document.createElement("div");
  document.body.append(host);
  const root = createRoot(host);
  act(() => {
    root.render(
      <QueryClientProvider client={queryClient}>
        <ToastProvider>
          <LocalRoutingSettingsEditor />
        </ToastProvider>
      </QueryClientProvider>,
    );
  });
  return { host, root, queryClient };
}

function findButton(host: HTMLElement, text: string): HTMLButtonElement | undefined {
  return Array.from(host.querySelectorAll("button")).find((button) => button.textContent?.includes(text)) as HTMLButtonElement | undefined;
}

function value(host: HTMLElement, label: string): string {
  return (host.querySelector(`[aria-label="${label}"]`) as HTMLInputElement).value;
}

function setInput(host: HTMLElement, selector: string, nextValue: string) {
  const input = host.querySelector(selector) as HTMLInputElement;
  act(() => {
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set;
    setter?.call(input, nextValue);
    input.dispatchEvent(new Event("input", { bubbles: true }));
    input.dispatchEvent(new Event("change", { bubbles: true }));
  });
}

function policyConfig(): RoutingPolicyConfigV2 {
  return {
    version: 2,
    reliabilityWeight: 4_000,
    responsivenessWeight: 2_500,
    costWeight: 2_000,
    preferenceWeight: 1_500,
    maxCandidates: 64,
    explorationShareBasisPoints: 500,
    allowDepletedFallback: false,
    affinityEnabled: false,
    affinityTtlSeconds: 300,
    maxRateMultiplier: null,
    outboundProxyMode: "inherit",
    outboundProxyUrl: null,
    routingGroupFilter: "all_groups",
    retryFailover: {
      version: 2,
      maxTotalAttempts: 4,
      maxSameTargetCapacityRetries: 2,
      capacityRetryWaitBudgetSeconds: 2,
      allowCrossCapacityDomainFallback: true,
    },
    protectionProfile: {
      version: 2,
      enabled: false,
      windowMaxSamples: 64,
      windowSeconds: 300,
      minSamples: 5,
      failureThresholdPercent: 60,
      halfOpenSuccessesToClose: 2,
    },
    timeoutPolicy: {
      version: 2,
      connectSeconds: 10,
      firstByteSeconds: 30,
      precommitSeconds: 60,
      bufferedExecutionSeconds: 300,
      streamIdleSeconds: 90,
    },
  };
}

function policySnapshot(config: RoutingPolicyConfigV2, revision: number) {
  return { config, revision, policyVersion: "routing-policy-v2", systemVersion: "routing-system-v1", status: "active" as const, updatedAtMs: revision };
}

function availableProtection() {
  return { statusVersion: "routing_protection_status_v1", generatedAtMs: 1, entries: [], readModelStatus: "available" as const };
}
