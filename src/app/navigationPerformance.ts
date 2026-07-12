const enabled = import.meta.env.DEV;
let hiddenPageQueryStarts = 0;

export type NavigationPerformanceSnapshot = {
  hiddenPageQueryStarts: number;
};

export const navigationMarks = {
  intent: (sequence: number) => `navigation:${sequence}:intent`,
  indicator: (sequence: number) => `navigation:${sequence}:indicator`,
  content: (sequence: number) => `navigation:${sequence}:content`,
  complete: (sequence: number) => `navigation:${sequence}:complete`,
};

export function markNavigation(name: string) {
  if (enabled) performance.mark(name);
}

export function measureNavigation(name: string, start: string, end: string) {
  if (!enabled) {
    return null;
  }
  performance.measure(name, start, end);
  const entries = performance.getEntriesByName(name);
  return entries[entries.length - 1]?.duration ?? null;
}

export function recordHiddenPageQueryStart() {
  if (enabled) hiddenPageQueryStarts += 1;
}

export function getNavigationPerformanceSnapshot(): NavigationPerformanceSnapshot {
  return { hiddenPageQueryStarts };
}

declare global {
  interface Window {
    __relayNavigationPerformance?: {
      snapshot: typeof getNavigationPerformanceSnapshot;
    };
  }
}

if (enabled && typeof window !== "undefined") {
  window.__relayNavigationPerformance = { snapshot: getNavigationPerformanceSnapshot };
}
