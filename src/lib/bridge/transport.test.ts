import { beforeEach, describe, expect, it, vi } from "vitest";

import { BackendError, ResultUnknownError } from "./errors";
import { clearRuntimeContextSession, configureRuntimeContextSession, runUserInteraction } from "./runtimeContext";
import { classifyNonIdempotentRejection, invoke } from "./transport";

const tauriInvoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke: tauriInvoke }));

describe("non-idempotent transport", () => {
  beforeEach(() => {
    tauriInvoke.mockReset().mockResolvedValue(undefined);
    clearRuntimeContextSession();
  });

  it("preserves typed backend failures as confirmed failures", async () => {
    const error = classifyNonIdempotentRejection("create_station", {
      code: "conflict",
      message: "The station already exists.",
      retryable: false,
    });
    expect(error).toBeInstanceOf(BackendError);
  });

  it("returns result unknown for an ambiguous post-dispatch rejection", async () => {
    const error = classifyNonIdempotentRejection(
      "create_station",
      new Error("response channel closed"),
    );
    expect(error).toBeInstanceOf(ResultUnknownError);
  });

  it("injects the validated runtime context at the one transport boundary", async () => {
    configureRuntimeContextSession("ctx_0123456789abcdef0123456789abcdef");
    await runUserInteraction(() => invoke("app_status", { input: {} }));

    const args = tauriInvoke.mock.calls[0]?.[1] as Record<string, unknown>;
    expect(args.input).toEqual({});
    expect(args.runtimeContext).toMatchObject({
      contextSessionId: "ctx_0123456789abcdef0123456789abcdef",
    });
    expect((args.runtimeContext as { interactionId: string }).interactionId).toMatch(/^int_[0-9a-f]{32}$/);
  });

  it("strips caller-supplied runtime metadata when no capability is active", async () => {
    await invoke("app_status", { input: {}, runtimeContext: { interactionId: "int_forbidden" } });
    expect(tauriInvoke.mock.calls[0]?.[1]).toEqual({ input: {} });
  });
});
