import { describe, expect, it, vi } from "vitest";
import { THEME_STORAGE_KEY } from "./theme";
import { readThemePreference, writeThemePreference, type ThemeStorage } from "./themeStorage";

function storage(value: string | null): ThemeStorage {
  return { getItem: vi.fn(() => value), setItem: vi.fn() };
}

describe("theme storage", () => {
  it("reads and validates a stored preference", () => {
    expect(readThemePreference(storage("dark"))).toBe("dark");
    expect(readThemePreference(storage("legacy"))).toBe("system");
    expect(readThemePreference(storage(null))).toBe("system");
  });

  it("falls back when storage access throws", () => {
    expect(readThemePreference({ getItem: () => { throw new Error("blocked"); }, setItem: vi.fn() })).toBe("system");
  });

  it("reports persistence success and failure", () => {
    const target = storage(null);
    expect(writeThemePreference("light", target)).toBe(true);
    expect(target.setItem).toHaveBeenCalledWith(THEME_STORAGE_KEY, "light");
    expect(writeThemePreference("dark", { getItem: vi.fn(), setItem: () => { throw new Error("full"); } })).toBe(false);
    expect(writeThemePreference("system", null)).toBe(false);
  });
});
