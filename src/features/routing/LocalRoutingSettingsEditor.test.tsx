// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ToastProvider } from "@/components/ui";
import type { RoutingPolicyConfigV1 } from "@/lib/types/routing";
import { LocalRoutingSettingsEditor } from "./LocalRoutingSettingsEditor";

const mocks = vi.hoisted(() => ({
  load: vi.fn(),
  apply: vi.fn(),
  refresh: vi.fn(),
}));

vi.mock("@/lib/api/routing", () => ({
  loadRoutingPolicy: mocks.load,
  applyRoutingPolicyDocument: mocks.apply,
}));

vi.mock("@/lib/query/routingQuerySynchronization", () => ({
  refreshRoutingQueries: mocks.refresh,
}));

vi.mock("@/lib/query/useActivityQuery", () => ({
  useActivityQuery: () => ({
    data: { collectorProxyMode: "system" },
    isPending: false,
    error: null,
  }),
}));

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

afterEach(() => {
  document.body.innerHTML = "";
  mocks.load.mockReset();
  mocks.apply.mockReset();
  mocks.refresh.mockReset();
  vi.restoreAllMocks();
});

describe("LocalRoutingSettingsEditor", () => {
  it("shows percentage presets and normalizes manual weight changes", async () => {
    const config = policyConfig();
    mocks.load.mockResolvedValue({ config, revision: 4 });
    mocks.apply.mockImplementation(async (input: { policy: RoutingPolicyConfigV1 }) => ({ config: input.policy, revision: 5 }));
    mocks.refresh.mockResolvedValue({ refreshed: true, errors: [] });
    const { host, root, queryClient } = renderEditor();

    await act(async () => Promise.resolve());
    expect(value(host, "可靠性百分比")).toBe("40");
    expect(value(host, "响应速度百分比")).toBe("25");
    expect(host.textContent).toContain("评分偏好");
    expect(host.textContent).not.toContain("权重合计");

    await act(async () => findButton(host, "稳定优先")?.click());
    expect(value(host, "可靠性百分比")).toBe("50");
    expect(value(host, "响应速度百分比")).toBe("25");
    expect(value(host, "成本百分比")).toBe("15");
    expect(value(host, "偏好百分比")).toBe("10");

    setInput(host, '[aria-label="可靠性百分比"]', "60");
    expect(value(host, "可靠性百分比")).toBe("60");
    await act(async () => findButton(host, "保存策略")?.click());
    const saved = mocks.apply.mock.calls[0][0].policy as RoutingPolicyConfigV1;
    expect(saved.reliabilityWeight + saved.responsivenessWeight + saved.costWeight + saved.preferenceWeight).toBe(10_000);
    expect(saved.reliabilityWeight).toBe(6_000);

    await act(async () => root.unmount());
    queryClient.clear();
  });

  it("converts exploration percentage and only shows affinity duration when enabled", async () => {
    const config = policyConfig();
    config.affinityEnabled = true;
    mocks.load.mockResolvedValue({ config, revision: 4 });
    mocks.apply.mockImplementation(async (input: { policy: RoutingPolicyConfigV1 }) => ({ config: input.policy, revision: 5 }));
    mocks.refresh.mockResolvedValue({ refreshed: true, errors: [] });
    const { host, root, queryClient } = renderEditor();

    await act(async () => Promise.resolve());
    expect(value(host, "探索比例")).toBe("5");
    expect(host.querySelector('[aria-label="亲和时长"]')).toBeTruthy();

    setInput(host, '[aria-label="探索比例"]', "7.25");
    await act(async () => findButton(host, "保存策略")?.click());
    const saved = mocks.apply.mock.calls[0][0].policy as RoutingPolicyConfigV1;
    expect(saved.explorationShareBasisPoints).toBe(725);

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

function policyConfig(): RoutingPolicyConfigV1 {
  return {
    version: 1,
    reliabilityWeight: 4_000,
    responsivenessWeight: 2_500,
    costWeight: 2_000,
    preferenceWeight: 1_500,
    maxCandidates: 64,
    explorationShareBasisPoints: 500,
    allowDepletedFallback: false,
    affinityEnabled: false,
    affinityTtlSeconds: 300,
    outboundProxyMode: "inherit",
    outboundProxyUrl: null,
    routingGroupFilter: "all_groups",
  };
}
