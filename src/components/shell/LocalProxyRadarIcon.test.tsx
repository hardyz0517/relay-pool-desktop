// @vitest-environment jsdom

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { LocalProxyRadarIcon } from "./LocalProxyRadarIcon";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

type ObserverCallback = IntersectionObserverCallback;

class IntersectionObserverStub {
  static instances: IntersectionObserverStub[] = [];
  readonly callback: ObserverCallback;
  readonly disconnect = vi.fn();
  readonly observe = vi.fn();
  readonly unobserve = vi.fn();

  constructor(callback: ObserverCallback) {
    this.callback = callback;
    IntersectionObserverStub.instances.push(this);
  }

  trigger(isIntersecting: boolean) {
    this.callback(
      [{
        isIntersecting,
        target: document.createElement("span"),
        boundingClientRect: isIntersecting
          ? { width: 24, height: 24, left: 0, right: 24, top: 0, bottom: 24 }
          : { width: 24, height: 24, left: -48, right: -24, top: 0, bottom: 24 },
      } as unknown as IntersectionObserverEntry],
      this as unknown as IntersectionObserver,
    );
  }

  triggerEmpty() {
    this.callback([], this as unknown as IntersectionObserver);
  }

  triggerMalformed() {
    this.callback(
      [{ isIntersecting: false, target: document.createElement("span") } as unknown as IntersectionObserverEntry],
      this as unknown as IntersectionObserver,
    );
  }
}

const originalVisibilityDescriptor = Object.getOwnPropertyDescriptor(document, "visibilityState");

describe("LocalProxyRadarIcon", () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    host = document.createElement("div");
    document.body.appendChild(host);
    root = createRoot(host);
    IntersectionObserverStub.instances = [];
    Object.defineProperty(document, "visibilityState", { configurable: true, value: "visible" });
    vi.stubGlobal("IntersectionObserver", IntersectionObserverStub);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
    vi.unstubAllGlobals();
    if (originalVisibilityDescriptor) {
      Object.defineProperty(document, "visibilityState", originalVisibilityDescriptor);
    }
  });

  function renderIcon(active: boolean, size: 24 | 32 = 24) {
    act(() => {
      root.render(<LocalProxyRadarIcon active={active} size={size} />);
    });
    return host.querySelector<HTMLElement>(".local-proxy-globe")!;
  }

  it("renders a fixed-size active sprite and supports the 32px asset", () => {
    const icon = renderIcon(true, 32);
    expect(icon.classList.contains("local-proxy-globe--active")).toBe(true);
    expect(icon.dataset.state).toBe("active");
    expect(icon.style.width).toBe("32px");
    expect(icon.style.height).toBe("32px");
    expect(icon.style.getPropertyValue("--local-proxy-globe-size")).toBe("32px");
    expect(icon.style.getPropertyValue("--local-proxy-globe-sprite-light")).toContain("routing-globe-sprite-32-light");
    expect(icon.style.getPropertyValue("--local-proxy-globe-static-light")).toContain("routing-globe-static-32-light");
  });

  it("shows the static baseline when inactive", () => {
    const icon = renderIcon(false);
    expect(icon.classList.contains("local-proxy-globe--active")).toBe(false);
    expect(icon.dataset.state).toBe("idle");
    expect(icon.dataset.visible).toBe("true");
    expect(icon.style.getPropertyValue("--local-proxy-globe-static-light")).toContain("routing-globe-static-24-light");
  });

  it("keeps routing animation active when reduced motion is requested", () => {
    vi.stubGlobal("matchMedia", vi.fn(() => ({ matches: true })));

    const icon = renderIcon(true);

    expect(icon.classList.contains("local-proxy-globe--active")).toBe(true);
    expect(icon.classList.contains("local-proxy-globe--paused")).toBe(false);
    expect(icon.dataset.state).toBe("active");
  });

  it("pauses and resumes when the icon leaves and re-enters the viewport", () => {
    const icon = renderIcon(true);
    const observer = IntersectionObserverStub.instances[0];
    expect(observer).toBeDefined();
    act(() => observer.trigger(false));
    expect(icon.classList.contains("local-proxy-globe--active")).toBe(true);
    expect(icon.classList.contains("local-proxy-globe--paused")).toBe(true);
    expect(icon.dataset.visible).toBe("false");
    act(() => observer.trigger(true));
    expect(icon.classList.contains("local-proxy-globe--active")).toBe(true);
    expect(icon.classList.contains("local-proxy-globe--paused")).toBe(false);
    expect(icon.dataset.visible).toBe("true");
  });

  it("keeps an active animation when an observer callback has no entries", () => {
    const icon = renderIcon(true);
    const observer = IntersectionObserverStub.instances[0];
    expect(observer).toBeDefined();

    act(() => observer.triggerEmpty());

    expect(icon.classList.contains("local-proxy-globe--active")).toBe(true);
    expect(icon.classList.contains("local-proxy-globe--paused")).toBe(false);
    expect(icon.dataset.visible).toBe("true");
  });

  it("ignores an observer entry without geometry until a complete entry arrives", () => {
    const icon = renderIcon(true);
    const observer = IntersectionObserverStub.instances[0];
    expect(observer).toBeDefined();

    act(() => observer.triggerMalformed());

    expect(icon.classList.contains("local-proxy-globe--active")).toBe(true);
    expect(icon.classList.contains("local-proxy-globe--paused")).toBe(false);
    expect(icon.dataset.visible).toBe("true");
  });

  it("pauses while the document is hidden and cleans up on unmount", () => {
    const icon = renderIcon(true);
    const observer = IntersectionObserverStub.instances[0];
    Object.defineProperty(document, "visibilityState", { configurable: true, value: "hidden" });
    act(() => document.dispatchEvent(new Event("visibilitychange")));
    expect(icon.classList.contains("local-proxy-globe--active")).toBe(true);
    expect(icon.classList.contains("local-proxy-globe--paused")).toBe(true);
    expect(icon.dataset.visible).toBe("false");

    act(() => window.dispatchEvent(new Event("focus")));
    expect(icon.classList.contains("local-proxy-globe--paused")).toBe(true);
    expect(icon.dataset.visible).toBe("false");

    act(() => root.unmount());
    expect(observer.disconnect).toHaveBeenCalledTimes(1);
    host.remove();
  });
});
