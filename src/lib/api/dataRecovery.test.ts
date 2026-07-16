import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: mocks.invoke,
}));

describe("data recovery API", () => {
  beforeEach(() => {
    mocks.invoke.mockReset();
  });

  it("returns the documented browser preview state only when Tauri invoke is unavailable", async () => {
    mocks.invoke.mockRejectedValue(new Error("window.__TAURI_INTERNALS__ is undefined"));
    const { getDataStoreStartupState } = await import("./dataRecovery");

    await expect(getDataStoreStartupState()).resolves.toEqual({
      decision: { kind: "ready", candidateId: "browser-preview" },
      candidates: [],
    });
  });

  it("does not hide ACL or missing-command errors behind browser preview fallback", async () => {
    const { getDataStoreStartupState } = await import("./dataRecovery");

    mocks.invoke.mockRejectedValue(new Error("Command get_data_store_startup_state not allowed by ACL"));
    await expect(getDataStoreStartupState()).rejects.toThrow(/not allowed by ACL/i);

    mocks.invoke.mockRejectedValue(new Error("Command get_data_store_startup_state not found"));
    await expect(getDataStoreStartupState()).rejects.toThrow(/not found/i);
  });
});
