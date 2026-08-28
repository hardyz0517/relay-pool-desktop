// @vitest-environment jsdom

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { scheduleTourAutoStart } from "./tourAutoStart";

describe("scheduleTourAutoStart", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("locks only after the manager accepts a retry", async () => {
    const start = vi.fn().mockReturnValueOnce(false).mockReturnValueOnce(true);
    const accepted = vi.fn();
    const dispose = scheduleTourAutoStart({
      canAttempt: () => true,
      hasTarget: () => true,
      start,
      onAccepted: accepted,
      retryMs: 100,
    });

    await vi.advanceTimersByTimeAsync(20);
    expect(start).toHaveBeenCalledTimes(1);
    expect(accepted).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(100);
    expect(start).toHaveBeenCalledTimes(2);
    expect(accepted).toHaveBeenCalledOnce();
    await vi.advanceTimersByTimeAsync(500);
    expect(start).toHaveBeenCalledTimes(2);
    dispose();
  });

  it("waits for readiness and a target", async () => {
    let ready = false;
    let target = false;
    const start = vi.fn(() => true);
    const dispose = scheduleTourAutoStart({
      canAttempt: () => ready,
      hasTarget: () => target,
      start,
      onAccepted: vi.fn(),
      retryMs: 50,
    });

    await vi.advanceTimersByTimeAsync(60);
    expect(start).not.toHaveBeenCalled();
    ready = true;
    await vi.advanceTimersByTimeAsync(50);
    expect(start).not.toHaveBeenCalled();
    target = true;
    await vi.advanceTimersByTimeAsync(50);
    expect(start).toHaveBeenCalledOnce();
    dispose();
  });

  it("stops at the deadline", async () => {
    const start = vi.fn(() => false);
    const dispose = scheduleTourAutoStart({
      canAttempt: () => true,
      hasTarget: () => true,
      start,
      onAccepted: vi.fn(),
      retryMs: 50,
      maxWaitMs: 120,
    });

    await vi.advanceTimersByTimeAsync(300);
    const calls = start.mock.calls.length;
    expect(calls).toBeGreaterThan(0);
    await vi.advanceTimersByTimeAsync(300);
    expect(start).toHaveBeenCalledTimes(calls);
    dispose();
  });
});
