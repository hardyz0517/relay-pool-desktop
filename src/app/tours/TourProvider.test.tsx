// @vitest-environment jsdom

import { StrictMode } from "react";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ToastProvider } from "@/components/ui";
import { TourProvider } from "./TourProvider";
import type { TourManagerApi, TourManagerSnapshot } from "./tourTypes";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function createManager(initialSnapshot?: TourManagerSnapshot) {
  let snapshot: TourManagerSnapshot = initialSnapshot ?? {
    phase: "idle",
    tourId: null,
    stepIndex: 0,
    stepCount: 0,
    source: null,
    message: null,
  };
  const listeners = new Set<(value: TourManagerSnapshot) => void>();
  const manager: TourManagerApi = {
    start: vi.fn(() => true),
    next: vi.fn(),
    previous: vi.fn(),
    retry: vi.fn(),
    skip: vi.fn(),
    close: vi.fn(),
    resetProgress: vi.fn(),
    getSnapshot: () => snapshot,
    subscribe: (listener) => {
      listeners.add(listener);
      listener(snapshot);
      return () => listeners.delete(listener);
    },
    dispose: vi.fn(),
  };
  return {
    manager,
    listeners,
    setSnapshot(next: TourManagerSnapshot) {
      snapshot = next;
      listeners.forEach((listener) => listener(snapshot));
    },
  };
}

describe("TourProvider lifecycle", () => {
  let root: Root | null = null;

  afterEach(() => {
    vi.useRealTimers();
    if (root) {
      act(() => root?.unmount());
      root = null;
    }
    document.body.replaceChildren();
  });

  function tree(manager: TourManagerApi, strict = false) {
    const content = (
      <ToastProvider>
        <TourProvider manager={manager}>
          <div>content</div>
        </TourProvider>
      </ToastProvider>
    );
    return strict ? <StrictMode>{content}</StrictMode> : content;
  }

  function render(manager: TourManagerApi, strict = false) {
    const container = document.createElement("div");
    document.body.append(container);
    root = createRoot(container);
    act(() => root?.render(tree(manager, strict)));
  }

  it("disposes after a normal unmount", () => {
    vi.useFakeTimers();
    const { manager } = createManager();
    render(manager);

    act(() => root?.unmount());
    root = null;
    expect(manager.dispose).not.toHaveBeenCalled();
    act(() => vi.runOnlyPendingTimers());
    expect(manager.dispose).toHaveBeenCalledOnce();
  });

  it("does not dispose during a StrictMode effect probe", () => {
    vi.useFakeTimers();
    const { manager } = createManager();
    render(manager, true);

    act(() => vi.runOnlyPendingTimers());
    expect(manager.dispose).not.toHaveBeenCalled();

    act(() => root?.unmount());
    root = null;
    act(() => vi.runOnlyPendingTimers());
    expect(manager.dispose).toHaveBeenCalledOnce();
  });

  it("disposes a replaced manager without confusing it with a StrictMode probe", () => {
    vi.useFakeTimers();
    const first = createManager();
    const second = createManager();
    render(first.manager);

    act(() => root?.render(tree(second.manager)));
    expect(first.manager.dispose).not.toHaveBeenCalled();
    expect(second.manager.dispose).not.toHaveBeenCalled();

    act(() => vi.runOnlyPendingTimers());
    expect(first.manager.dispose).toHaveBeenCalledOnce();
    expect(second.manager.dispose).not.toHaveBeenCalled();

    act(() => root?.unmount());
    root = null;
    act(() => vi.runOnlyPendingTimers());
    expect(second.manager.dispose).toHaveBeenCalledOnce();
  });

  it("disposes immediately on pagehide and cancels delayed cleanup", () => {
    vi.useFakeTimers();
    const { manager } = createManager();
    render(manager);

    act(() => window.dispatchEvent(new Event("pagehide")));
    expect(manager.dispose).toHaveBeenCalledOnce();

    act(() => root?.unmount());
    root = null;
    act(() => vi.runOnlyPendingTimers());
    expect(manager.dispose).toHaveBeenCalledOnce();
  });

  it("closes an active tutorial for a business modal but ignores its Driver.js popover", async () => {
    const { manager } = createManager({
      phase: "running",
      tourId: "basic",
      stepIndex: 0,
      stepCount: 1,
      source: "test",
      message: null,
    });
    render(manager);

    const popover = document.createElement("div");
    popover.className = "driver-popover";
    popover.setAttribute("role", "dialog");
    document.body.append(popover);
    await act(async () => { await Promise.resolve(); });
    expect(manager.close).not.toHaveBeenCalled();

    const businessModal = document.createElement("div");
    businessModal.dataset.tourBlocking = "true";
    document.body.append(businessModal);
    await act(async () => { await Promise.resolve(); });
    expect(manager.close).toHaveBeenCalledOnce();
  });

  it("closes only an active tutorial when the window loses visibility", () => {
    const harness = createManager({
      phase: "running",
      tourId: "basic",
      stepIndex: 0,
      stepCount: 1,
      source: "test",
      message: null,
    });
    render(harness.manager);

    act(() => window.dispatchEvent(new Event("blur")));
    expect(harness.manager.close).toHaveBeenCalledOnce();

    act(() => {
      harness.setSnapshot({
        phase: "idle",
        tourId: null,
        stepIndex: 0,
        stepCount: 0,
        source: null,
        message: null,
      });
    });
    act(() => window.dispatchEvent(new Event("blur")));
    expect(harness.manager.close).toHaveBeenCalledOnce();

    act(() => {
      harness.setSnapshot({
        phase: "waiting-target",
        tourId: "basic",
        stepIndex: 0,
        stepCount: 1,
        source: "test",
        message: null,
      });
    });
    const visibility = vi.spyOn(document, "visibilityState", "get").mockReturnValue("hidden");
    act(() => document.dispatchEvent(new Event("visibilitychange")));
    expect(harness.manager.close).toHaveBeenCalledTimes(2);
    visibility.mockRestore();
  });

  it("surfaces a rejected start that has no active overlay", () => {
    const { manager } = createManager({
      phase: "idle",
      tourId: "basic",
      stepIndex: 0,
      stepCount: 7,
      source: "settings",
      message: "请先关闭当前对话框，再开始教程",
    });

    render(manager);

    expect(document.body.textContent).toContain("教程暂时无法继续");
    expect(document.body.textContent).toContain("请先关闭当前对话框，再开始教程");
    expect(document.querySelector("[data-tour-error]")).toBeNull();
  });

  it("surfaces a skipped-progress persistence warning", () => {
    const { manager } = createManager({
      phase: "skipped",
      tourId: "basic",
      stepIndex: 1,
      stepCount: 7,
      source: "settings",
      message: "教程已关闭，但进度未能持久化",
    });

    render(manager);

    expect(document.body.textContent).toContain("教程已退出");
    expect(document.body.textContent).toContain("教程已关闭，但进度未能持久化");
  });
});
