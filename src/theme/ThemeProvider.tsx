import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";

import { syncNativeTheme } from "./nativeTheme";
import {
  resolveTheme,
  type ThemePreference,
  type ThemeSnapshot,
  type ThemeUpdateResult,
} from "./theme";
import {
  applyResolvedTheme,
  subscribeToSystemTheme,
  systemPrefersDark,
} from "./themeDom";
import { writeThemePreference } from "./themeStorage";

type ThemeContextValue = ThemeSnapshot & {
  setPreference: (preference: ThemePreference) => ThemeUpdateResult;
};

type ThemeProviderProps = {
  children: ReactNode;
  initialSnapshot: ThemeSnapshot;
};

const ThemeContext = createContext<ThemeContextValue | undefined>(undefined);

export function ThemeProvider({ children, initialSnapshot }: ThemeProviderProps) {
  const [snapshot, setSnapshot] = useState(initialSnapshot);
  const preferenceRef = useRef(initialSnapshot.preference);

  const refreshSystemTheme = useCallback(() => {
    if (preferenceRef.current !== "system") return;

    setSnapshot({
      preference: "system",
      resolvedTheme: resolveTheme("system", systemPrefersDark()),
    });
  }, []);

  const setPreference = useCallback((preference: ThemePreference): ThemeUpdateResult => {
    const persisted = writeThemePreference(preference);
    preferenceRef.current = preference;
    setSnapshot({
      preference,
      resolvedTheme: resolveTheme(preference, systemPrefersDark()),
    });
    return { persisted };
  }, []);

  useLayoutEffect(() => {
    applyResolvedTheme(snapshot.resolvedTheme);
  }, [snapshot.resolvedTheme]);

  useEffect(() => {
    preferenceRef.current = snapshot.preference;
    if (snapshot.preference !== "system") return;

    refreshSystemTheme();
    return subscribeToSystemTheme((prefersDark) => {
      if (preferenceRef.current !== "system") return;

      setSnapshot({
        preference: "system",
        resolvedTheme: resolveTheme("system", prefersDark),
      });
    });
  }, [refreshSystemTheme, snapshot.preference]);

  useEffect(() => {
    const preference = snapshot.preference;
    let cancelled = false;

    void syncNativeTheme(preference).then((result) => {
      if (
        !cancelled &&
        result.applied &&
        result.current &&
        preference === "system"
      ) {
        refreshSystemTheme();
      }
    });

    return () => {
      cancelled = true;
    };
  }, [refreshSystemTheme, snapshot.preference]);

  const value = useMemo(
    () => ({ ...snapshot, setPreference }),
    [setPreference, snapshot],
  );

  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
}

export function useTheme(): ThemeContextValue {
  const value = useContext(ThemeContext);
  if (!value) throw new Error("useTheme must be used within ThemeProvider");
  return value;
}
