import { describe, expect, it, vi } from "vitest";

import { createNativeThemeSync, NativeThemeQueue } from "./nativeTheme";

function deferred(): { promise: Promise<void>; resolve: () => void } {
  let resolve!: () => void;
  const promise = new Promise<void>((fulfill) => {
    resolve = fulfill;
  });

  return { promise, resolve };
}

describe("NativeThemeQueue", () => {
  it("skips stale queued generations and applies the latest preference last", async () => {
    const firstCall = deferred();
    const setTheme = vi.fn(() => firstCall.promise);
    const queue = new NativeThemeQueue(setTheme);

    const light = queue.request("light");
    await vi.waitFor(() => expect(setTheme).toHaveBeenCalledOnce());

    const dark = queue.request("dark");
    const system = queue.request("system");
    firstCall.resolve();

    await expect(light).resolves.toEqual({ generation: 1, applied: true, current: false });
    await expect(dark).resolves.toEqual({ generation: 2, applied: false, current: false });
    await expect(system).resolves.toEqual({ generation: 3, applied: true, current: true });
    expect(setTheme.mock.calls).toEqual([["light"], [null]]);
  });

  it("continues processing after a native setter failure", async () => {
    const setTheme = vi
      .fn<(theme: "light" | "dark" | null) => Promise<void>>()
      .mockRejectedValueOnce(new Error("denied"))
      .mockResolvedValue(undefined);
    const queue = new NativeThemeQueue(setTheme);

    await expect(queue.request("dark")).resolves.toEqual({
      generation: 1,
      applied: false,
      current: true,
    });
    await expect(queue.request("light")).resolves.toEqual({
      generation: 2,
      applied: true,
      current: true,
    });
    expect(setTheme.mock.calls).toEqual([["dark"], ["light"]]);
  });
});

describe("createNativeThemeSync", () => {
  it("reports repeated native failures once without exposing their details", async () => {
    const setTheme = vi.fn(() => Promise.reject(new Error("denied")));
    const log = vi.fn();
    const syncTheme = createNativeThemeSync(setTheme, log);

    await syncTheme("dark");
    await syncTheme("light");

    expect(log).toHaveBeenCalledOnce();
    expect(log).toHaveBeenCalledWith("Native window theme synchronization is unavailable.");
  });
});
