// @vitest-environment jsdom
import { describe, expect, it, vi } from "vitest";
import { applyResolvedTheme, subscribeToSystemTheme, systemPrefersDark } from "./themeDom";

describe("theme DOM", () => {
  it("keeps exactly one effective class and color scheme", () => {
    document.documentElement.className = "light unrelated";
    applyResolvedTheme("dark");
    expect(document.documentElement.classList.contains("dark")).toBe(true);
    expect(document.documentElement.classList.contains("light")).toBe(false);
    expect(document.documentElement.classList.contains("unrelated")).toBe(true);
    expect(document.documentElement.style.colorScheme).toBe("dark");
  });

  it("reads and subscribes to the media query", () => {
    const addEventListener = vi.fn();
    const removeEventListener = vi.fn();
    const media = { matches: true, addEventListener, removeEventListener } as unknown as MediaQueryList;
    expect(systemPrefersDark(() => media)).toBe(true);
    const listener = vi.fn();
    const dispose = subscribeToSystemTheme(listener, () => media);
    expect(addEventListener).toHaveBeenCalledWith("change", expect.any(Function));
    const handleChange = addEventListener.mock.calls[0][1] as (event: MediaQueryListEvent) => void;
    handleChange({ matches: true } as MediaQueryListEvent);
    handleChange({ matches: false } as MediaQueryListEvent);
    expect(listener).toHaveBeenNthCalledWith(1, true);
    expect(listener).toHaveBeenNthCalledWith(2, false);
    dispose();
    expect(removeEventListener).toHaveBeenCalledWith("change", handleChange);
  });

  it("falls back when matchMedia is unavailable", () => {
    expect(systemPrefersDark(null)).toBe(false);
    const dispose = subscribeToSystemTheme(vi.fn(), null);
    expect(dispose).not.toThrow();
  });
});
