// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { initializeTheme } from "./themeBootstrap";

describe("theme bootstrap", () => {
  it("returns and applies one shared initial snapshot", () => {
    const snapshot = initializeTheme(
      { getItem: () => "system", setItem: () => undefined },
      () => ({ matches: true } as MediaQueryList),
    );
    expect(snapshot).toEqual({ preference: "system", resolvedTheme: "dark" });
    expect(document.documentElement.classList.contains("dark")).toBe(true);
  });
});
