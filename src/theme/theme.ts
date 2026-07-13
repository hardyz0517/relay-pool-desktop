export type ThemePreference = "light" | "dark" | "system";
export type ResolvedTheme = "light" | "dark";

export type ThemeSnapshot = {
  preference: ThemePreference;
  resolvedTheme: ResolvedTheme;
};

export type ThemeUpdateResult = {
  persisted: boolean;
};

export const THEME_STORAGE_KEY = "relay-pool.theme-preference.v1";

export function parseThemePreference(value: unknown): ThemePreference {
  return value === "light" || value === "dark" || value === "system" ? value : "system";
}

export function resolveTheme(
  preference: ThemePreference,
  systemPrefersDark: boolean,
): ResolvedTheme {
  if (preference === "system") {
    return systemPrefersDark ? "dark" : "light";
  }
  return preference;
}

export function nativeThemeFor(preference: ThemePreference): ResolvedTheme | null {
  return preference === "system" ? null : preference;
}
