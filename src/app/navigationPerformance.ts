const enabled = import.meta.env.DEV;
let hiddenPageQueryStarts = 0;

export type NavigationPerformanceSnapshot = {
  hiddenPageQueryStarts: number;
};

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
