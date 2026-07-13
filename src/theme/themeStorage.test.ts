import { describe, expect, it, vi } from "vitest";
import { readThemePreference, writeThemePreference, type ThemeStorage } from "./themeStorage";

function storage(value: string | null): ThemeStorage {
  return { getItem: vi.fn(() => value), setItem: vi.fn() };
}

describe("theme storage", () => {
  it("reads and validates a stored preference", () => {
    expect(readThemePreference(storage("dark"))).toBe("dark");
    expect(readThemePreference(storage("legacy"))).toBe("system");
  });

  it("falls back when storage access throws", () => {
    expect(readThemePreference({ getItem: () => { throw new Error("blocked"); }, setItem: vi.fn() })).toBe("system");
  });

  it("reports persistence success and failure", () => {
    expect(writeThemePreference("light", storage(null))).toBe(true);
    expect(writeThemePreference("dark", { getItem: vi.fn(), setItem: () => { throw new Error("full"); } })).toBe(false);
    expect(writeThemePreference("system", null)).toBe(false);
  });
});
