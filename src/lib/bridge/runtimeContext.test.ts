import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  clearRuntimeContextSession,
  configureRuntimeContextSession,
  currentRuntimeContext,
  runUserInteraction,
} from "./runtimeContext";

const sessionId = "ctx_0123456789abcdef0123456789abcdef";

describe("runtime interaction context", () => {
  beforeEach(() => {
    clearRuntimeContextSession();
    vi.restoreAllMocks();
  });

  it("keeps the capability in memory and starts without an interaction", () => {
    configureRuntimeContextSession(sessionId);
    expect(currentRuntimeContext()).toEqual({ contextSessionId: sessionId, interactionId: null });
  });

  it("reuses one interaction for nested commands and clears it at completion", async () => {
    configureRuntimeContextSession(sessionId);
    let first: string | null = null;
    let nested: string | null = null;
    await runUserInteraction(async () => {
      first = currentRuntimeContext()?.interactionId ?? null;
      await runUserInteraction(() => {
        nested = currentRuntimeContext()?.interactionId ?? null;
      });
      expect(first).not.toBeNull();
      expect(first).toMatch(/^int_[0-9a-f]{32}$/);
    });
    expect(first).toBe(nested);
    expect(currentRuntimeContext()?.interactionId).toBeNull();
  });

  it("rejects malformed or non-hex capability values", () => {
    expect(() => configureRuntimeContextSession("ctx_bad")).toThrow();
    expect(() => configureRuntimeContextSession("ctx_0123456789abcdef0123456789ABCDEG")).toThrow();
  });

  it("does not create an interaction before the backend capability is configured", async () => {
    await runUserInteraction(() => {
      expect(currentRuntimeContext()).toBeNull();
    });
  });

  it("does not cross-wire overlapping async gestures", async () => {
    configureRuntimeContextSession(sessionId);
    let releaseFirst!: () => void;
    const firstReady = new Promise<void>((resolve) => {
      releaseFirst = resolve;
    });
    let firstDuringDispatch: string | null = null;
    let firstAfterAwait: string | null = null;
    const first = runUserInteraction(async () => {
      firstDuringDispatch = currentRuntimeContext()?.interactionId ?? null;
      await firstReady;
      firstAfterAwait = currentRuntimeContext()?.interactionId ?? null;
    });
    let secondDuringDispatch: string | null = null;
    await runUserInteraction(() => {
      secondDuringDispatch = currentRuntimeContext()?.interactionId ?? null;
    });
    releaseFirst();
    await first;
    expect(firstDuringDispatch).not.toBeNull();
    expect(secondDuringDispatch).not.toBeNull();
    expect(secondDuringDispatch).not.toBe(firstDuringDispatch);
    expect(firstAfterAwait).toBeNull();
  });
});
