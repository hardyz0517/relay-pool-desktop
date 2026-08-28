import { describe, expect, it, vi } from "vitest";
import { TourPreparationRegistry } from "./tourPreparationRegistry";

const context = { tourId: "basic" as const, stepId: "step", route: "dashboard" as const };

describe("TourPreparationRegistry", () => {
  it("always provides the no-op preparation", async () => {
    const registry = new TourPreparationRegistry();
    expect(registry.has("none")).toBe(true);
    await expect(registry.run("none", context, new AbortController().signal)).resolves.toBeNull();
  });

  it("rejects unregistered keys and duplicate/reserved registrations", async () => {
    const registry = new TourPreparationRegistry();
    await expect(
      registry.run("missing" as never, context, new AbortController().signal),
    ).rejects.toMatchObject({ code: "unregistered" });
    expect(() => registry.register("none", vi.fn())).toThrow();
    registry.register("routing-status-tab", vi.fn());
    expect(() => registry.register("routing-status-tab", vi.fn())).toThrow();
  });

  it("propagates cancellation and wraps action failures", async () => {
    const controller = new AbortController();
    const cancelledRegistry = new TourPreparationRegistry();
    cancelledRegistry.register("routing-status-tab", async (_context, signal) => {
      await new Promise<void>((resolve) => setTimeout(resolve, 0));
      if (signal.aborted) throw new Error("aborted");
    });
    controller.abort();
    await expect(
      cancelledRegistry.run("routing-status-tab", context, controller.signal),
    ).rejects.toMatchObject({ code: "aborted" });

    const failedRegistry = new TourPreparationRegistry();
    failedRegistry.register("routing-status-tab", () => { throw new Error("boom"); });
    await expect(
      failedRegistry.run("routing-status-tab", context, new AbortController().signal),
    ).rejects.toMatchObject({ code: "failed" });
  });

  it("returns an idempotent cleanup for reversible view preparation", async () => {
    const cleanup = vi.fn();
    const registry = new TourPreparationRegistry({
      actions: new Map([["routing-status-tab", vi.fn(() => cleanup)]]),
    });

    const restore = await registry.run("routing-status-tab", context, new AbortController().signal);
    restore?.();
    restore?.();

    expect(cleanup).toHaveBeenCalledOnce();
  });

  it("restores a prepared view when cancellation wins after the action", async () => {
    const cleanup = vi.fn();
    const controller = new AbortController();
    const registry = new TourPreparationRegistry({
      actions: new Map([["routing-status-tab", async () => {
        controller.abort();
        return cleanup;
      }]]),
    });

    await expect(
      registry.run("routing-status-tab", context, controller.signal),
    ).rejects.toMatchObject({ code: "aborted" });
    expect(cleanup).toHaveBeenCalledOnce();
  });
});
