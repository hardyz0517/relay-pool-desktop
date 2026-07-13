import { describe, expect, it } from "vitest";
import {
  THEME_STORAGE_KEY,
  nativeThemeFor,
  parseThemePreference,
  resolveTheme,
} from "./theme";

describe("theme model", () => {
  it.each(["light", "dark", "system"] as const)("accepts %s", (value) => {
    expect(parseThemePreference(value)).toBe(value);
  });

  it.each([null, undefined, "", "auto", "LIGHT", 1])(
    "falls back invalid value %s to system",
    (value) => expect(parseThemePreference(value)).toBe("system"),
  );

  it("resolves system and ignores the system value for manual preferences", () => {
    expect(resolveTheme("system", false)).toBe("light");
    expect(resolveTheme("system", true)).toBe("dark");
    expect(resolveTheme("light", true)).toBe("light");
    expect(resolveTheme("dark", false)).toBe("dark");
  });

  it("maps system to the Tauri null theme", () => {
    expect(nativeThemeFor("light")).toBe("light");
    expect(nativeThemeFor("dark")).toBe("dark");
    expect(nativeThemeFor("system")).toBeNull();
    expect(THEME_STORAGE_KEY).toBe("relay-pool.theme-preference.v1");
  });
});
