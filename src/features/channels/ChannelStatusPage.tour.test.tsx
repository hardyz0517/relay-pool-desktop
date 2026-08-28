// @vitest-environment jsdom

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ChannelViewPreparationPort } from "./channelViewPreparation";
import { ChannelStatusPage } from "./ChannelStatusPage";

vi.mock("./ChannelStatusTab", () => ({
  ChannelStatusTab: () => (
    <div>
      <div data-tour="channels-local-toolbar" />
      <div data-tour="channels-local-results" />
    </div>
  ),
}));

vi.mock("./OfficialStatusTab", () => ({
  OfficialStatusTab: () => (
    <div>
      <div data-tour="channels-official-summary" />
      <div data-tour="channels-official-results" />
    </div>
  ),
}));

vi.mock("./ChannelMonitoringTab", () => ({
  ChannelMonitoringTab: () => <div>探针内容</div>,
}));

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

describe("ChannelStatusPage tour preparation", () => {
  let host: HTMLDivElement;
  let root: Root;
  let queryClient: QueryClient;
  let viewPort: ChannelViewPreparationPort | null;

  beforeEach(() => {
    host = document.createElement("div");
    document.body.appendChild(host);
    root = createRoot(host);
    queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    viewPort = null;
    act(() => {
      root.render(
        <QueryClientProvider client={queryClient}>
          <ChannelStatusPage onViewPreparationPort={(port) => { viewPort = port; }} />
        </QueryClientProvider>,
      );
    });
  });

  afterEach(() => {
    act(() => root.unmount());
    queryClient.clear();
    host.remove();
  });

  it("switches among stable tab anchors and restores the previous view once", () => {
    expect(host.querySelector('[data-tour="channels-tabs"]')).not.toBeNull();
    expect(host.querySelector('[data-tour="channels-local-results"]')).not.toBeNull();

    const officialButton = Array.from(host.querySelectorAll<HTMLButtonElement>('[role="radio"]'))
      .find((button) => button.textContent === "官方状态");
    act(() => officialButton?.click());
    expect(host.querySelector('[data-tour="channels-official-results"]')).not.toBeNull();

    let restore: () => void = () => undefined;
    act(() => { restore = viewPort?.showMonitoringView() ?? restore; });
    expect(host.querySelector('[data-tour="channels-monitoring-list"]')).not.toBeNull();

    act(() => restore());
    expect(host.querySelector('[data-tour="channels-official-results"]')).not.toBeNull();

    act(() => restore());
    expect(host.querySelector('[data-tour="channels-official-results"]')).not.toBeNull();
  });

  it("clears the preparation port when the page unmounts", () => {
    expect(viewPort).not.toBeNull();
    act(() => root.unmount());
    expect(viewPort).toBeNull();
    root = createRoot(host);
  });

  it("captures the latest prepared view during rapid switches", () => {
    let restoreOfficial: () => void = () => undefined;
    let restoreMonitoring: () => void = () => undefined;
    act(() => {
      restoreOfficial = viewPort?.showOfficialView() ?? restoreOfficial;
      restoreMonitoring = viewPort?.showMonitoringView() ?? restoreMonitoring;
    });
    expect(host.querySelector('[data-tour="channels-monitoring-list"]')).not.toBeNull();

    act(() => restoreMonitoring());
    expect(host.querySelector('[data-tour="channels-official-results"]')).not.toBeNull();

    act(() => restoreOfficial());
    expect(host.querySelector('[data-tour="channels-local-results"]')).not.toBeNull();
  });
});
