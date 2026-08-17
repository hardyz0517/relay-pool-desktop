// @vitest-environment jsdom

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AppSettings } from "@/lib/types/settings";
import { CollectorAdvancedSettings } from "./CollectorAdvancedSettings";

const mocks = vi.hoisted(() => ({
  getSettings: vi.fn(),
  toastError: vi.fn(),
  toastSuccess: vi.fn(),
  updateSettings: vi.fn(),
}));

vi.mock("@/lib/api/settings", () => ({
  getSettings: mocks.getSettings,
  updateSettings: mocks.updateSettings,
}));

vi.mock("@/components/ui", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/components/ui")>();
  return {
    ...actual,
    useToast: () => ({
      error: mocks.toastError,
      success: mocks.toastSuccess,
    }),
  };
});

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let host: HTMLDivElement;
let root: Root;
let queryClient: QueryClient;

beforeEach(() => {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  queryClient = new QueryClient({
    defaultOptions: { mutations: { retry: false }, queries: { retry: false } },
  });
  mocks.getSettings.mockReset().mockResolvedValue(settings());
  mocks.updateSettings.mockReset().mockResolvedValue(settings());
  mocks.toastError.mockReset();
  mocks.toastSuccess.mockReset();
});

afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  queryClient.clear();
});

describe("CollectorAdvancedSettings", () => {
  it("recovers a failed settings read and restores the published-status default from the retry result", async () => {
    mocks.getSettings
      .mockRejectedValueOnce(new Error("fixture settings read failed"))
      .mockResolvedValueOnce(settings({ publishedStatusIntervalMinutes: 5 }));

    await renderSettings();
    await waitForText("fixture settings read failed");

    await act(async () => textButton("重试").click());
    await waitForDraft();

    expect(mocks.getSettings).toHaveBeenCalledTimes(2);
    expect(publishedStatusInput().value).toBe("5");
    expect(publishedStatusInput().min).toBe("1");
    expect(publishedStatusInput().max).toBe("1440");
    expect(publishedStatusInput().step).toBe("1");
  });

  it("blocks an out-of-range published-status interval, then saves the repaired draft without losing fields", async () => {
    await renderSettings();
    await waitForDraft();

    await act(async () => setInputValue(publishedStatusInput(), "1441"));
    await act(async () => textButton("保存采集设置").click());

    expect(mocks.updateSettings).not.toHaveBeenCalled();
    expect(publishedStatusInput().getAttribute("aria-invalid")).toBe("true");
    expect(host.textContent).toContain("请输入 1 到 1440 的整数");
    expect(mocks.toastError).toHaveBeenCalledWith("保存采集设置失败", "请修正标记的参数");

    mocks.updateSettings.mockResolvedValueOnce(settings({ publishedStatusIntervalMinutes: 1 }));
    await act(async () => setInputValue(publishedStatusInput(), "1"));
    await act(async () => {
      textButton("保存采集设置").click();
      await Promise.resolve();
    });
    await waitFor(() => mocks.updateSettings.mock.calls.length === 1);

    expect(mocks.updateSettings).toHaveBeenCalledWith(expect.objectContaining({
      localProxyPort: 8787,
      balanceIntervalMinutes: 5,
      groupRateIntervalMinutes: 20,
      publishedStatusIntervalMinutes: 1,
      pricingRefreshIntervalMinutes: 60,
      collectorTimeoutSeconds: 15,
      collectorMaxConcurrency: 3,
    }));
    await waitFor(() => publishedStatusInput().getAttribute("aria-invalid") === "false");
    expect(publishedStatusInput().value).toBe("1");
    expect(publishedStatusInput().getAttribute("aria-invalid")).toBe("false");
  });

  it("includes published-status cadence in the recommended recovery draft", async () => {
    await renderSettings();
    await waitForDraft();

    await act(async () => textButton("恢复推荐值").click());
    expect(publishedStatusInput().value).toBe("5");
    expect(host.querySelector<HTMLInputElement>("#collector-setting-balanceIntervalMinutes")?.value).toBe("5");
    expect(host.querySelector<HTMLInputElement>("#collector-setting-groupRateIntervalMinutes")?.value).toBe("20");
    expect(host.querySelector<HTMLInputElement>("#collector-setting-pricingRefreshIntervalMinutes")?.value).toBe("60");
  });
});

async function renderSettings() {
  await act(async () => {
    root.render(
      <QueryClientProvider client={queryClient}>
        <CollectorAdvancedSettings />
      </QueryClientProvider>,
    );
  });
}

function settings(overrides: Partial<AppSettings> = {}): AppSettings {
  return {
    localProxyPort: 8787,
    localProxyStartOnLaunch: false,
    localKeyMasked: "sk-local-fixture",
    defaultRoutingStrategy: "automatic_balanced",
    collectorProxyMode: "direct",
    collectorProxyUrl: null,
    maxRateMultiplier: null,
    defaultRoutingGroupFilter: "all_groups",
    schedulerAdvancedSettings: {
      topK: 7,
      multiplier: 1,
      priority: 1,
      load: 1,
      queue: 0.7,
      errorRate: 0.8,
      ttft: 0.5,
      quotaHeadroom: 0,
      previousResponse: 5,
      sessionSticky: 3,
      multiplierMinConfidence: 0.8,
      stickyWeighted: false,
      stickyEscape: true,
      stickyEscapeTtftMs: 15_000,
      stickyEscapeErrorRate: 0.5,
      stickySessionTtlSeconds: 3_600,
      stickyResponseTtlSeconds: 3_600,
      stickyMaxWaiting: 3,
      stickyWaitTimeoutSeconds: 120,
      fallbackMaxWaiting: 100,
      fallbackWaitTimeoutSeconds: 30,
    },
    lowBalanceThresholdCny: 15,
    collectorIntervalMinutes: 30,
    balanceIntervalMinutes: 5,
    groupRateIntervalMinutes: 20,
    publishedStatusIntervalMinutes: 5,
    pricingRefreshIntervalMinutes: 60,
    collectorTimeoutSeconds: 15,
    collectorMaxConcurrency: 3,
    allowDepletedFallback: false,
    developerModeEnabled: false,
    dataDir: "fixture-data-dir",
    pendingDataDir: null,
    dataDirChangeRequiresRestart: false,
    ...overrides,
  };
}

function publishedStatusInput() {
  return host.querySelector<HTMLInputElement>("#collector-setting-publishedStatusIntervalMinutes")!;
}

function textButton(label: string) {
  return [...host.querySelectorAll<HTMLButtonElement>("button")]
    .find((button) => button.textContent?.includes(label))!;
}

function setInputValue(input: HTMLInputElement, value: string) {
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
  setter.call(input, value);
  input.dispatchEvent(new Event("input", { bubbles: true }));
}

async function waitForDraft() {
  await waitFor(() => Boolean(host.querySelector("#collector-setting-publishedStatusIntervalMinutes")));
}

async function waitForText(value: string) {
  await waitFor(() => host.textContent?.includes(value) ?? false);
}

async function waitFor(predicate: () => boolean) {
  for (let attempt = 0; attempt < 20; attempt += 1) {
    if (predicate()) return;
    await act(async () => {
      await new Promise<void>((resolve) => setTimeout(resolve, 0));
    });
  }
  expect(predicate()).toBe(true);
}
