// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ToastProvider } from "@/components/ui";
import { BackendError } from "@/lib/bridge/errors";
import type { RoutingPolicyConfigV3 } from "@/lib/types/routing";
import { LocalRoutingSettingsEditor } from "./LocalRoutingSettingsEditor";
import { createDefaultRoutingPolicyConfig } from "./useRoutingPolicyDraft";

const mocks = vi.hoisted(() => ({
  policyData: null as unknown,
  protectionData: null as unknown,
  policyQuery: null as unknown,
  protectionQuery: null as unknown,
  apply: vi.fn(),
  publication: vi.fn(() => new Promise(() => undefined)),
  refresh: vi.fn(),
}));

vi.mock("@/lib/api/routing", () => ({
  loadRoutingPolicy: vi.fn(async () => mocks.policyData),
  getRoutingProtectionStatus: vi.fn(async () => mocks.protectionData),
  applyRoutingPolicyDocument: mocks.apply,
  getRoutingPolicyPublicationStatus: mocks.publication,
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
    return { data: { collectorProxyMode: "system" }, isPending: false, error: null, refetch: vi.fn() };
  },
}));

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

afterEach(() => {
  document.body.innerHTML = "";
  mocks.policyData = null;
  mocks.protectionData = null;
  mocks.policyQuery = null;
  mocks.protectionQuery = null;
  mocks.apply.mockReset();
  mocks.publication.mockReset();
  mocks.publication.mockImplementation(() => new Promise(() => undefined));
  mocks.refresh.mockReset();
  vi.restoreAllMocks();
});

describe("LocalRoutingSettingsEditor", () => {
  it("renders V3 defaults and removes legacy candidate/exploration and error-rate controls", async () => {
    const config = policyConfig();
    mocks.policyData = policySnapshot(config, 4);
    mocks.protectionData = availableProtection();
    const { host, root, queryClient } = renderEditor();

    await act(async () => Promise.resolve());
    expect(host.querySelector('[aria-label="真实流量与监控权重比例滑块"]')).not.toBeNull();
    expect(host.querySelector('[aria-label="编辑真实流量权重"]')).not.toBeNull();
    expect(host.querySelector('[aria-label="编辑监控权重"]')).not.toBeNull();
    expect(host.querySelector('[aria-label="真实流量权重（%）"]')).toBeNull();
    expect(host.querySelector('[aria-label="监控权重（%）"]')).toBeNull();
    expect(host.textContent).toContain("真实流量（%）");
    expect(host.querySelector('[aria-label="编辑真实流量权重"]')?.textContent?.trim()).toBe("70");
    expect(host.textContent).toContain("监控（%）");
    expect(host.querySelector('[aria-label="编辑监控权重"]')?.textContent?.trim()).toBe("30");
    expect(host.querySelector('[aria-label="编辑真实流量权重"]')?.className).toContain("font-normal");
    expect(host.querySelector('[aria-label="编辑监控权重"]')?.className).toContain("font-normal");
    expect(value(host, "历史最小样本数")).toBe("15");
    expect(value(host, "最近最小样本数")).toBe("5");
    expect(value(host, "乐观可靠性（%）")).toBe("95");
    expect(value(host, "乐观响应时间（毫秒）")).toBe("2500");
    expect(value(host, "最大重试次数（次）")).toBe("3");
    expect(value(host, "连续失败阈值（次）")).toBe("3");
    expect(value(host, "恢复成功阈值（次）")).toBe("2");
    expect(value(host, "恢复等待时间（秒）")).toBe("30");
    expect(host.textContent).toContain("质量来源权重");
    expect(host.textContent).toContain("熔断器设置");
    expect(host.textContent).toContain("重试设置");
    expect(host.textContent).toContain("首把密钥之外最多再尝试多少把密钥");
    expect(host.textContent).toContain("当前密钥失败后会继续重试");
    expect(host.textContent).toContain("连续失败达到该次数后熔断并尝试下一把密钥");
    expect(host.textContent).not.toContain("429 按当前密钥的普通故障处理并尝试下一候选");
    expect(host.textContent).not.toContain("候选与探索");
    expect(host.textContent).not.toContain("错误率保护参数");
    expect(host.querySelector('[aria-label="最大候选数"]')).toBeNull();
    expect(host.querySelector('[aria-label="探索比例（%）"]')).toBeNull();
    expect(host.querySelector('[aria-label="失败率阈值（%）"]')).toBeNull();
    expect(host.querySelector('[aria-label="允许跨容量域回退"]')).toBeNull();
    const scoreSection = host.querySelector("#routing-policy-weights-title")?.closest("section");
    expect(scoreSection?.querySelector('[aria-label="真实流量与监控权重比例滑块"]')).not.toBeNull();
    expect(scoreSection?.querySelector('[aria-label="乐观响应时间（毫秒）"]')).not.toBeNull();

    await act(async () => root.unmount());
    queryClient.clear();
  });

  it("shows only an unsaved tag while the draft is dirty", async () => {
    mocks.policyData = policySnapshot(policyConfig(), 4, "active");
    mocks.protectionData = availableProtection();
    const { host, root, queryClient } = renderEditor();

    await act(async () => Promise.resolve());
    expect(host.textContent).not.toContain("未保存");
    expect(host.textContent).not.toContain("已生效");
    expect(host.textContent).not.toContain("当前运行时正在使用此策略");

    setInput(host, '[aria-label="最大重试次数（次）"]', "1");
    expect(host.textContent).toContain("未保存");
    expect(host.textContent).not.toContain("存在未保存的修改");
    expect(host.textContent).not.toContain("已生效");
    expect(host.textContent).not.toContain("当前运行时正在使用此策略");

    await act(async () => root.unmount());
    queryClient.clear();
  });

  it("keeps source weights at 100 percent while editing", async () => {
    const config = policyConfig();
    mocks.policyData = policySnapshot(config, 4);
    mocks.protectionData = availableProtection();
    mocks.apply.mockImplementation(async (input: { policy: RoutingPolicyConfigV3 }) => policySnapshot(input.policy, 5));
    const { host, root, queryClient } = renderEditor();

    await act(async () => Promise.resolve());
    clickElement(host, '[aria-label="编辑真实流量权重"]');
    setInput(host, '[aria-label="真实流量权重（%）"]', "85");
    expect(value(host, "真实流量权重（%）")).toBe("85");
    expect(host.querySelector('[aria-label="编辑监控权重"]')?.textContent).toContain("15");
    await act(async () => findButton(host, "保存")?.click());
    const saved = mocks.apply.mock.calls[0][0].policy as RoutingPolicyConfigV3;
    expect(saved.reliabilitySourceWeights).toEqual({ realTrafficPercent: 85, monitoringPercent: 15 });

    await act(async () => root.unmount());
    queryClient.clear();
  });

  it("updates the other source weight when the monitoring side is edited", async () => {
    const config = policyConfig();
    mocks.policyData = policySnapshot(config, 4);
    mocks.protectionData = availableProtection();
    mocks.apply.mockImplementation(async (input: { policy: RoutingPolicyConfigV3 }) => policySnapshot(input.policy, 5));
    const { host, root, queryClient } = renderEditor();

    await act(async () => Promise.resolve());
    clickElement(host, '[aria-label="编辑监控权重"]');
    setInput(host, '[aria-label="监控权重（%）"]', "22");
    expect(value(host, "监控权重（%）")).toBe("22");
    expect(host.querySelector('[aria-label="编辑真实流量权重"]')?.textContent).toContain("78");

    await act(async () => findButton(host, "保存")?.click());
    const saved = mocks.apply.mock.calls[0][0].policy as RoutingPolicyConfigV3;
    expect(saved.reliabilitySourceWeights).toEqual({ realTrafficPercent: 78, monitoringPercent: 22 });

    await act(async () => root.unmount());
    queryClient.clear();
  });

  it("updates both displayed percentages when the slider moves", async () => {
    mocks.policyData = policySnapshot(policyConfig(), 4);
    mocks.protectionData = availableProtection();
    const { host, root, queryClient } = renderEditor();

    await act(async () => Promise.resolve());
    setInput(host, '[aria-label="真实流量与监控权重比例滑块"]', "85");
    expect(value(host, "真实流量与监控权重比例滑块")).toBe("85");
    expect(host.querySelector('[aria-label="编辑真实流量权重"]')?.textContent).toContain("85");
    expect(host.querySelector('[aria-label="编辑监控权重"]')?.textContent).toContain("15");

    await act(async () => root.unmount());
    queryClient.clear();
  });

  it("keeps controls locked while saving and reports staged policy honestly", async () => {
    const config = policyConfig();
    mocks.policyData = policySnapshot(config, 4);
    mocks.protectionData = availableProtection();
    let resolveApply: ((value: ReturnType<typeof policySnapshot>) => void) | null = null;
    mocks.apply.mockImplementation(() => new Promise((resolve) => {
      resolveApply = resolve;
    }));
    const { host, root, queryClient } = renderEditor();

    await act(async () => Promise.resolve());
    setInput(host, '[aria-label="最大重试次数（次）"]', "1");
    act(() => findButton(host, "保存")?.click());
    await act(async () => Promise.resolve());

    const retryInput = host.querySelector('[aria-label="最大重试次数（次）"]') as HTMLInputElement;
    const proxySelect = host.querySelector('[aria-label="本地路由出站代理"]') as HTMLButtonElement;
    expect(retryInput.matches(":disabled")).toBe(true);
    expect(proxySelect.matches(":disabled")).toBe(true);

    setInput(host, '[aria-label="最大重试次数（次）"]', "2");
    const stagedConfig = { ...config, retry: { ...config.retry, maxRetryCount: 1 } };
    await act(async () => {
      resolveApply?.(policySnapshot(stagedConfig, 5, "staged"));
      await Promise.resolve();
    });

    expect(value(host, "最大重试次数（次）")).toBe("1");
    expect(host.textContent).toContain("等待重建");
    expect(host.textContent).toContain("路由策略已提交");
    expect(host.textContent).not.toContain("路由策略已保存");

    await act(async () => root.unmount());
    queryClient.clear();
  });

  it.each([
    ["ready", "等待切换", "重建已完成"],
    ["failed", "重建失败", "当前运行策略未改变"],
  ])("shows the %s publication state", async (status, label, description) => {
    mocks.policyData = policySnapshot(policyConfig(), 4, status);
    mocks.protectionData = availableProtection();
    const { host, root, queryClient } = renderEditor();

    await act(async () => Promise.resolve());
    expect(host.textContent).toContain(label);
    expect(host.textContent).toContain(description);

    await act(async () => root.unmount());
    queryClient.clear();
  });

  it("does not silently clamp an invalid source weight", async () => {
    mocks.policyData = policySnapshot(policyConfig(), 4);
    mocks.protectionData = availableProtection();
    const { host, root, queryClient } = renderEditor();

    await act(async () => Promise.resolve());
    clickElement(host, '[aria-label="编辑真实流量权重"]');
    setInput(host, '[aria-label="真实流量权重（%）"]', "110");
    expect(value(host, "真实流量权重（%）")).toBe("110");
    expect(host.querySelector('[aria-label="编辑监控权重"]')?.textContent).toContain("-10");
    expect(host.textContent).toContain("必须是 0-100 的整数");
    expect(host.querySelector('[aria-label="真实流量权重（%）"]')?.getAttribute("aria-invalid")).toBe("true");

    await act(async () => root.unmount());
    queryClient.clear();
  });

  it("shows publication read failure without claiming the staged policy is active", async () => {
    const config = policyConfig();
    mocks.policyData = policySnapshot(config, 4);
    mocks.protectionData = availableProtection();
    mocks.apply.mockImplementation(async (input: { policy: RoutingPolicyConfigV3 }) =>
      policySnapshot(input.policy, 5, "staged"));
    mocks.publication.mockRejectedValue(new Error("publication unavailable"));
    const { host, root, queryClient } = renderEditor();

    await act(async () => Promise.resolve());
    setInput(host, '[aria-label="最大重试次数（次）"]', "1");
    await act(async () => {
      findButton(host, "保存")?.click();
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(host.textContent).toContain("发布状态不可用");
    expect(host.textContent).toContain("尚未确认此策略已生效");
    expect(host.textContent).not.toContain("当前运行时正在使用此策略");

    await act(async () => root.unmount());
    queryClient.clear();
  });

  it("shows independent explanations for every timeout field", async () => {
    mocks.policyData = policySnapshot(policyConfig(), 4);
    mocks.protectionData = availableProtection();
    const { host, root, queryClient } = renderEditor();

    await act(async () => Promise.resolve());
    const expected = [
      ["首字节超时（秒）", "等待上游开始返回内容的最长时间"],
      ["提交前超时（秒）", "输出提交给客户端前允许消耗的总预算"],
      ["缓冲执行超时（秒）", "非流式请求在完整响应返回前允许执行的最长时间"],
      ["流空闲超时（秒）", "两次输出之间允许的最长静默时间"],
    ] as const;
    for (const [label, explanation] of expected) {
      const input = host.querySelector(`[aria-label="${label}"]`);
      expect(input?.parentElement?.parentElement?.textContent).toContain(explanation);
      expect(input?.parentElement?.parentElement?.textContent).toContain("范围");
      expect(input?.parentElement?.parentElement?.textContent).toContain("默认");
    }
    const connectInput = host.querySelector('[aria-label="连接超时（秒）"]');
    expect(connectInput?.parentElement?.parentElement?.textContent).toContain("范围 1-120 秒，默认 10 秒。");
    expect(host.textContent).not.toContain("建立到中转站网络连接允许等待的最长时间");
    expect(host.textContent).not.toContain("保存后需要重启本地路由");

    await act(async () => root.unmount());
    queryClient.clear();
  });

  it("edits retry and circuit settings in their V3 namespaces", async () => {
    mocks.policyData = policySnapshot(policyConfig(), 4);
    mocks.protectionData = availableProtection();
    mocks.apply.mockImplementation(async (input: { policy: RoutingPolicyConfigV3 }) => policySnapshot(input.policy, 5));
    const { host, root, queryClient } = renderEditor();

    await act(async () => Promise.resolve());
    setInput(host, '[aria-label="最大重试次数（次）"]', "1");
    setInput(host, '[aria-label="连续失败阈值（次）"]', "4");
    setInput(host, '[aria-label="恢复成功阈值（次）"]', "3");
    setInput(host, '[aria-label="恢复等待时间（秒）"]', "60");
    await act(async () => findButton(host, "保存")?.click());
    const saved = mocks.apply.mock.calls[0][0].policy as RoutingPolicyConfigV3;
    expect(saved.retry.maxRetryCount).toBe(1);
    expect(saved.retry.consecutiveFailureThreshold).toBe(4);
    expect(saved.circuitBreaker.recoverySuccessThreshold).toBe(3);
    expect(saved.circuitBreaker.recoveryWaitSeconds).toBe(60);

    await act(async () => root.unmount());
    queryClient.clear();
  });

  it("maps V3 backend validation errors to their fields", async () => {
    mocks.policyData = policySnapshot(policyConfig(), 4);
    mocks.protectionData = availableProtection();
    mocks.apply.mockRejectedValue(new BackendError(
      "invalid_input",
      "策略验证失败",
      false,
      {
        kind: "validation",
        fields: [{ field: "circuitBreaker.recoveryWaitSeconds", code: "out_of_range", message: "恢复等待时间超出范围" }],
      },
    ));
    const { host, root, queryClient } = renderEditor();

    await act(async () => Promise.resolve());
    setInput(host, '[aria-label="恢复等待时间（秒）"]', "4");
    await act(async () => findButton(host, "保存")?.click());
    const field = host.querySelector('[aria-label="恢复等待时间（秒）"]') as HTMLInputElement;
    expect(field.getAttribute("aria-invalid")).toBe("true");
    expect(host.textContent).toContain("恢复等待时间超出范围");

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
        <ToastProvider><LocalRoutingSettingsEditor /></ToastProvider>
      </QueryClientProvider>,
    );
  });
  return { host, root, queryClient };
}

function findButton(host: HTMLElement, text: string): HTMLButtonElement | undefined {
  return Array.from(host.querySelectorAll("button")).find((button) => button.textContent?.includes(text)) as HTMLButtonElement | undefined;
}

function clickElement(host: HTMLElement, selector: string) {
  const element = host.querySelector(selector) as HTMLElement | null;
  if (!element) throw new Error(`Expected element ${selector}`);
  act(() => element.click());
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

function policyConfig(): RoutingPolicyConfigV3 {
  return createDefaultRoutingPolicyConfig();
}

function policySnapshot(config: RoutingPolicyConfigV3, revision: number, status = "active") {
  return { config, revision, policyVersion: "routing-policy-v3", systemVersion: "routing-system-v1", status, updatedAtMs: revision, documentSync: null };
}

function availableProtection() {
  return { statusVersion: "routing_protection_status_v1", generatedAtMs: 1, entries: [], readModelStatus: "available" as const };
}
