import type { ResolvedTheme } from "./theme";

type MatchMedia = (query: string) => MediaQueryList;

function browserMatchMedia(): MatchMedia | null {
  return typeof window !== "undefined" && typeof window.matchMedia === "function"
    ? window.matchMedia.bind(window)
    : null;
}

export function systemPrefersDark(matchMedia: MatchMedia | null = browserMatchMedia()): boolean {
  return matchMedia?.("(prefers-color-scheme: dark)").matches ?? false;
}

export function applyResolvedTheme(
  theme: ResolvedTheme,
  root: HTMLElement = document.documentElement,
): void {
  root.classList.remove("light", "dark");
  root.classList.add(theme);
  root.style.colorScheme = theme;
}

export function subscribeToSystemTheme(
  listener: (prefersDark: boolean) => void,
  matchMedia: MatchMedia | null = browserMatchMedia(),
): () => void {
  if (!matchMedia) return () => undefined;
  const media = matchMedia("(prefers-color-scheme: dark)");
  const handleChange = (event: MediaQueryListEvent) => listener(event.matches);
  media.addEventListener("change", handleChange);
  return () => media.removeEventListener("change", handleChange);
}
