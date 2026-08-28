// @vitest-environment jsdom

import { afterEach, describe, expect, it, vi } from "vitest";
import { TourDriverAdapter, type DriverInstance } from "./TourDriverAdapter";

function input(callbacks: Parameters<TourDriverAdapter["showStep"]>[0]["callbacks"]) {
  return {
    element: {} as HTMLElement,
    title: "Title",
    description: "Description",
    stepIndex: 0,
    stepCount: 2,
    callbacks,
  };
}

describe("TourDriverAdapter", () => {
  afterEach(() => document.body.replaceChildren());

  it("maps step content and callbacks to the driver factory", () => {
    const instances: Array<DriverInstance & { config?: Record<string, unknown>; options?: unknown }> = [];
    const create = vi.fn((config) => {
      const instance = {
        config,
        highlight: vi.fn(),
        destroy: vi.fn(),
      };
      instances.push(instance);
      return instance;
    });
    const adapter = new TourDriverAdapter({ create, popoverClass: "tour-popover" });
    const callbacks = { next: vi.fn(), previous: vi.fn(), close: vi.fn(), destroyed: vi.fn() };
    adapter.showStep(input(callbacks));

    expect(create).toHaveBeenCalledWith(expect.objectContaining({
      showProgress: true,
      allowClose: true,
      disableActiveInteraction: true,
    }));
    expect(instances[0].highlight).toHaveBeenCalledWith(expect.objectContaining({
      element: expect.any(Object),
      popover: expect.objectContaining({ title: "Title", progressText: "1 / 2", popoverClass: "tour-popover" }),
    }));
    const config = instances[0].config as { onNextClick: () => void; onPrevClick: () => void; onCloseClick: () => void };
    config.onNextClick(); config.onPrevClick(); config.onCloseClick();
    expect(callbacks.next).toHaveBeenCalledOnce();
    expect(callbacks.previous).toHaveBeenCalledOnce();
    expect(callbacks.close).toHaveBeenCalledOnce();
  });

  it("suppresses destroyed callback for owned teardown and forwards external teardown", () => {
    const callbacks = { next: vi.fn(), previous: vi.fn(), close: vi.fn(), destroyed: vi.fn() };
    let externalDestroy: (() => void) | undefined;
    const instance = { highlight: vi.fn(), destroy: vi.fn(), };
    const adapter = new TourDriverAdapter({ create: (config) => {
      externalDestroy = config.onDestroyed;
      return instance;
    }});
    adapter.showStep(input(callbacks));
    externalDestroy?.();
    expect(callbacks.destroyed).toHaveBeenCalledOnce();

    adapter.showStep(input(callbacks));
    adapter.destroy("step-change");
    expect(callbacks.destroyed).toHaveBeenCalledOnce();
    externalDestroy?.();
    expect(callbacks.destroyed).toHaveBeenCalledOnce();
    adapter.destroy("close");
    expect(instance.destroy).toHaveBeenCalledOnce();
  });

  it("ignores a delayed destroy callback from a stale instance", () => {
    const origin = document.createElement("button");
    const fallback = document.createElement("button");
    document.body.append(origin, fallback);
    origin.focus();
    const callbacksA = { next: vi.fn(), previous: vi.fn(), close: vi.fn(), destroyed: vi.fn() };
    const callbacksB = { next: vi.fn(), previous: vi.fn(), close: vi.fn(), destroyed: vi.fn() };
    let staleDestroy: (() => void) | undefined;
    let currentDestroy: (() => void) | undefined;
    const first = { highlight: vi.fn(), destroy: vi.fn() };
    const second = { highlight: vi.fn(), destroy: vi.fn() };
    let count = 0;
    const adapter = new TourDriverAdapter({ create: (config) => {
      count += 1;
      if (count === 1) staleDestroy = config.onDestroyed;
      else currentDestroy = config.onDestroyed;
      return count === 1 ? first : second;
    }});
    adapter.beginSession();
    adapter.showStep(input(callbacksA));
    adapter.showStep(input(callbacksB));
    fallback.focus();
    staleDestroy?.();
    expect(callbacksA.destroyed).not.toHaveBeenCalled();
    expect(callbacksB.destroyed).not.toHaveBeenCalled();
    expect(document.activeElement).toBe(fallback);
    currentDestroy?.();
    expect(callbacksB.destroyed).toHaveBeenCalledOnce();
    expect(document.activeElement).toBe(origin);
  });

  it("captures focus at session start and preserves it across step changes", () => {
    const origin = document.createElement("button");
    const popoverControl = document.createElement("button");
    document.body.append(origin, popoverControl);
    origin.focus();

    const instances = [
      { highlight: vi.fn(), destroy: vi.fn() },
      { highlight: vi.fn(), destroy: vi.fn() },
    ];
    let index = 0;
    const adapter = new TourDriverAdapter({ create: () => instances[index++] });
    const callbacks = { next: vi.fn(), previous: vi.fn(), close: vi.fn(), destroyed: vi.fn() };

    adapter.beginSession();
    adapter.showStep(input(callbacks));
    popoverControl.focus();
    adapter.showStep(input(callbacks));
    adapter.destroy("complete");

    expect(document.activeElement).toBe(origin);
    expect(instances[0].destroy).toHaveBeenCalledOnce();
    expect(instances[1].destroy).toHaveBeenCalledOnce();
  });

  it("restores focus on terminal teardown when driver creation failed", () => {
    const origin = document.createElement("button");
    const fallback = document.createElement("button");
    document.body.append(origin, fallback);
    origin.focus();
    const adapter = new TourDriverAdapter({ create: () => { throw new Error("create failed"); } });
    const callbacks = { next: vi.fn(), previous: vi.fn(), close: vi.fn(), destroyed: vi.fn() };

    adapter.beginSession();
    expect(() => adapter.showStep(input(callbacks))).toThrow("create failed");
    fallback.focus();
    adapter.destroy("close");

    expect(document.activeElement).toBe(origin);
  });

  it("retains the session focus after highlight failure until terminal teardown", () => {
    const origin = document.createElement("button");
    const fallback = document.createElement("button");
    document.body.append(origin, fallback);
    origin.focus();
    const instance = {
      highlight: vi.fn(() => { throw new Error("highlight failed"); }),
      destroy: vi.fn(),
    };
    const adapter = new TourDriverAdapter({ create: () => instance });
    const callbacks = { next: vi.fn(), previous: vi.fn(), close: vi.fn(), destroyed: vi.fn() };

    adapter.beginSession();
    expect(() => adapter.showStep(input(callbacks))).toThrow("highlight failed");
    adapter.destroy("step-change");
    fallback.focus();
    adapter.destroy("close");

    expect(instance.destroy).toHaveBeenCalledOnce();
    expect(document.activeElement).toBe(origin);
  });

  it.each(["inert", "aria-hidden", "disconnected"] as const)(
    "does not restore focus to an invalid %s origin",
    (condition) => {
      const wrapper = document.createElement("div");
      const origin = document.createElement("button");
      const fallback = document.createElement("button");
      wrapper.append(origin);
      document.body.append(wrapper, fallback);
      origin.focus();
      const adapter = new TourDriverAdapter({
        create: () => ({ highlight: vi.fn(), destroy: vi.fn() }),
      });
      const callbacks = { next: vi.fn(), previous: vi.fn(), close: vi.fn(), destroyed: vi.fn() };

      adapter.beginSession();
      adapter.showStep(input(callbacks));
      fallback.focus();
      if (condition === "inert") wrapper.setAttribute("inert", "");
      if (condition === "aria-hidden") wrapper.setAttribute("aria-hidden", "true");
      if (condition === "disconnected") wrapper.remove();
      adapter.destroy("close");

      expect(document.activeElement).toBe(fallback);
    },
  );
});
