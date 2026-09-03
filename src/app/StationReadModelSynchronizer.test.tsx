// @vitest-environment jsdom

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { StationReadModelSynchronizer } from "./StationReadModelSynchronizer";

const mocks = vi.hoisted(() => ({
  reconcile: vi.fn(),
  subscribe: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({ isTauri: () => true }));
vi.mock("@/lib/query/stationCollectionQuerySynchronization", () => ({
  DOMAIN_REVISION_UPDATED_EVENT: "domain-revision-updated",
  reconcileStationReadModels: mocks.reconcile,
  subscribeToDomainRevisionUpdates: mocks.subscribe,
}));

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const originalVisibilityDescriptor = Object.getOwnPropertyDescriptor(document, "visibilityState");

describe("StationReadModelSynchronizer", () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    vi.useFakeTimers();
    host = document.createElement("div");
    document.body.appendChild(host);
    root = createRoot(host);
    Object.defineProperty(document, "visibilityState", { configurable: true, value: "visible" });
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
    vi.useRealTimers();
    vi.clearAllMocks();
    if (originalVisibilityDescriptor) {
      Object.defineProperty(document, "visibilityState", originalVisibilityDescriptor);
    }
  });

  it("subscribes before probing and reconciles on resume and the fallback interval", async () => {
    let finishSubscription: ((unlisten: () => void) => void) | undefined;
    let finishInitialProbe: (() => void) | undefined;
    const unlisten = vi.fn();
    mocks.subscribe.mockReturnValue(new Promise((resolve) => {
      finishSubscription = resolve;
    }));
    mocks.reconcile
      .mockReturnValueOnce(new Promise<void>((resolve) => {
        finishInitialProbe = resolve;
      }))
      .mockResolvedValue(undefined);

    act(() => {
      root.render(
        <QueryClientProvider client={new QueryClient()}>
          <StationReadModelSynchronizer />
        </QueryClientProvider>,
      );
    });

    expect(mocks.subscribe).toHaveBeenCalledTimes(1);
    expect(mocks.reconcile).not.toHaveBeenCalled();

    await act(async () => {
      finishSubscription?.(unlisten);
      await Promise.resolve();
    });
    expect(mocks.reconcile).toHaveBeenCalledTimes(1);

    act(() => {
      window.dispatchEvent(new Event("pageshow"));
      vi.advanceTimersByTime(30_000);
    });
    expect(mocks.reconcile).toHaveBeenCalledTimes(1);

    await act(async () => {
      finishInitialProbe?.();
      await Promise.resolve();
    });
    act(() => window.dispatchEvent(new Event("pageshow")));
    await act(async () => Promise.resolve());
    expect(mocks.reconcile).toHaveBeenCalledTimes(2);

    Object.defineProperty(document, "visibilityState", { configurable: true, value: "hidden" });
    act(() => document.dispatchEvent(new Event("visibilitychange")));
    expect(mocks.reconcile).toHaveBeenCalledTimes(2);

    Object.defineProperty(document, "visibilityState", { configurable: true, value: "visible" });
    act(() => document.dispatchEvent(new Event("visibilitychange")));
    await act(async () => Promise.resolve());
    expect(mocks.reconcile).toHaveBeenCalledTimes(3);

    act(() => vi.advanceTimersByTime(30_000));
    await act(async () => Promise.resolve());
    expect(mocks.reconcile).toHaveBeenCalledTimes(4);

    act(() => root.unmount());
    act(() => {
      window.dispatchEvent(new Event("pageshow"));
      vi.advanceTimersByTime(30_000);
    });
    expect(mocks.reconcile).toHaveBeenCalledTimes(4);
    expect(unlisten).toHaveBeenCalledTimes(1);
  });
});
