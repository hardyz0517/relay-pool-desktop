import { resolveTheme, type ThemeSnapshot } from "./theme";
import { applyResolvedTheme, systemPrefersDark } from "./themeDom";
import { readThemePreference, type ThemeStorage } from "./themeStorage";

export function initializeTheme(
  storage?: ThemeStorage | null,
  matchMedia?: (query: string) => MediaQueryList,
): ThemeSnapshot {
  const preference = readThemePreference(storage);
  const resolvedTheme = resolveTheme(preference, systemPrefersDark(matchMedia));
  applyResolvedTheme(resolvedTheme);
  return { preference, resolvedTheme };
}
