import { describe, expect, it, vi } from "vitest";

import {
  IncompatibleStreamingEventError,
  openStationKeyConnectivityStream,
  validateStationKeyConnectivityEvent,
} from "./streamingAdapter";

class FakeChannel<Event> {
  onmessage?: (event: Event) => void;
}

function event(sequence: number, overrides: Record<string, unknown> = {}) {
  return {
    schemaVersion: 1,
    runId: "run-1",
    sequence,
    terminal: false,
    cancelCapability: "detach_only",
    event: { type: "delta", text: "hi" },
    ...overrides,
  };
}

describe("streaming adapter contract", () => {
  it("rejects unknown event schema versions", () => {
    expect(() =>
      validateStationKeyConnectivityEvent(event(0, { schemaVersion: 2 })),
    ).toThrow(IncompatibleStreamingEventError);
  });

  it("rejects non-contiguous event sequences", () => {
    const state = { runId: null, nextSequence: 0, terminalSeen: false };
    validateStationKeyConnectivityEvent(event(0), state);
    expect(() => validateStationKeyConnectivityEvent(event(2), state)).toThrow(
      IncompatibleStreamingEventError,
    );
  });

  it("rejects events after the single terminal envelope", () => {
    const state = { runId: null, nextSequence: 0, terminalSeen: false };
    validateStationKeyConnectivityEvent(event(0), state);
    validateStationKeyConnectivityEvent(
      event(1, {
        terminal: true,
        event: { type: "completed", ok: true },
      }),
      state,
    );
    expect(() => validateStationKeyConnectivityEvent(event(2), state)).toThrow(
      IncompatibleStreamingEventError,
    );
  });

  it("resolves only after a terminal event arrives", async () => {
    let channel: FakeChannel<unknown> | null = null;
    const result = {
      stationKeyId: "key-1",
      ok: true,
      statusCode: 200,
      durationMs: 12,
      model: "gpt-test",
      message: "ok",
      responseMode: "stream" as const,
      streamFallbackReason: null,
    };
    const invoke = vi.fn(({ progress }) => {
      channel = progress as FakeChannel<unknown>;
      channel.onmessage?.(event(0));
      channel.onmessage?.(
        event(1, {
          terminal: true,
          event: { type: "completed", ok: true },
        }),
      );
      return Promise.resolve(result);
    });

    await expect(
      openStationKeyConnectivityStream(
        { stationKeyId: "key-1", model: "gpt-test" },
        { ChannelConstructor: FakeChannel, invoke },
      ).promise,
    ).resolves.toEqual(result);
  });

  it("treats close as detach-only without reporting cancellation", async () => {
    const channels: FakeChannel<unknown>[] = [];
    const onEvent = vi.fn();
    const invoke = vi.fn(({ progress }) => {
      channels.push(progress as FakeChannel<unknown>);
      return new Promise<never>(() => undefined);
    });
    const { promise, subscription } = openStationKeyConnectivityStream(
      { stationKeyId: "key-1", model: "gpt-test" },
      { ChannelConstructor: FakeChannel, invoke, onEvent },
    );

    subscription.close();
    channels[0]?.onmessage?.(event(0));

    await expect(Promise.race([promise, Promise.resolve("detached")])).resolves.toBe("detached");
    expect(subscription.cancelCapability).toBe("detach_only");
    expect(onEvent).not.toHaveBeenCalled();
  });
});
