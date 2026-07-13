// @vitest-environment jsdom
import { describe, expect, it, vi } from "vitest";
import { initializeTheme } from "./themeBootstrap";

describe("theme bootstrap", () => {
  it("returns and applies one shared initial snapshot", () => {
    const getItem = vi.fn(() => "system");
    const snapshot = initializeTheme(
      { getItem, setItem: () => undefined },
      () => ({ matches: true } as MediaQueryList),
    );
    expect(snapshot).toEqual({ preference: "system", resolvedTheme: "dark" });
    expect(document.documentElement.classList.contains("dark")).toBe(true);
    expect(getItem).toHaveBeenCalledTimes(1);
  });
});
