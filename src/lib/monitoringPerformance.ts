const enabled = import.meta.env.DEV;

export type MonitoringPerformanceMetric = {
  name: string;
  durationMs?: number;
  rowCount?: number;
  trendCellCount?: number;
  dtoBytes?: number;
  cacheHit?: boolean;
  queryCalls?: number;
  phase?: "mount" | "update" | "nested-update";
};

const metrics: MonitoringPerformanceMetric[] = [];
let longTaskCount = 0;

export function recordMonitoringPerformance(metric: MonitoringPerformanceMetric) {
  if (!enabled) return;
  metrics.push({ ...metric, durationMs: metric.durationMs == null ? undefined : Math.round(metric.durationMs * 100) / 100 });
  if (metrics.length > 200) metrics.splice(0, metrics.length - 200);
}

export function measureMonitoring<T>(
  name: string,
  operation: () => T,
  details?: Omit<MonitoringPerformanceMetric, "name" | "durationMs">,
): T {
  if (!enabled) return operation();
  const start = performance.now();
  const result = operation();
  recordMonitoringPerformance({ name, durationMs: performance.now() - start, ...details });
  return result;
}

export function getMonitoringPerformanceSnapshot() {
  return { metrics: metrics.slice(), longTaskCount };
}

declare global {
  interface Window {
    __relayMonitoringPerformance?: { snapshot: typeof getMonitoringPerformanceSnapshot };
  }
}

if (enabled && typeof window !== "undefined") {
  window.__relayMonitoringPerformance = { snapshot: getMonitoringPerformanceSnapshot };
  if (typeof PerformanceObserver !== "undefined") {
    try {
      const observer = new PerformanceObserver((list) => {
        for (const entry of list.getEntries()) {
          if (entry.duration >= 50) longTaskCount += 1;
        }
      });
      observer.observe({ type: "longtask", buffered: true });
    } catch {
      // Long-task entries are not supported by every WebView.
    }
  }
}
