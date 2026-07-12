async (page) => {
  const targets = [
    { label: "中转站资产", routeId: "stations" },
    { label: "变更中心", routeId: "changes" },
    { label: "设置", routeId: "settings" },
    { label: "使用记录", routeId: "logs" },
    { label: "价格 / 倍率", routeId: "pricing" },
    { label: "总览", routeId: "dashboard" },
  ];
  const percentile = (values, ratio) => {
    const sorted = [...values].sort((a, b) => a - b);
    return sorted[Math.max(0, Math.ceil(sorted.length * ratio) - 1)] ?? 0;
  };

  async function runBurst(intervalMs) {
    return page.evaluate(async ({ targets, interval }) => {
      const clicks = new Map();
      const acknowledgements = new Map();
      const contentDurations = new Map();
      const pendingAcknowledgements = new Set();
      const pendingContent = new Set();
      const longTasks = [];
      const buttons = Array.from(document.querySelectorAll("aside nav button"));
      const targetByLabel = new Map(targets.map((target) => [target.label, target]));
      const observer = new MutationObserver(() => {
        for (const button of buttons) {
          const label = button.getAttribute("aria-label");
          const target = label ? targetByLabel.get(label) : null;
          if (
            target &&
            clicks.has(target.routeId) &&
            button.classList.contains("bg-slate-900") &&
            !acknowledgements.has(target.routeId) &&
            !pendingAcknowledgements.has(target.routeId)
          ) {
            pendingAcknowledgements.add(target.routeId);
            requestAnimationFrame(() => {
              acknowledgements.set(target.routeId, performance.now() - clicks.get(target.routeId));
              pendingAcknowledgements.delete(target.routeId);
            });
          }
        }
        for (const layer of document.querySelectorAll('[data-page-transition-kind="shell"]')) {
          const routeId = layer.getAttribute("data-page-transition-page-id");
          const state = layer.getAttribute("data-page-transition-state");
          if (
            routeId &&
            clicks.has(routeId) &&
            (state === "entering" || state === "active") &&
            !contentDurations.has(routeId) &&
            !pendingContent.has(routeId)
          ) {
            pendingContent.add(routeId);
            requestAnimationFrame(() => {
              contentDurations.set(routeId, performance.now() - clicks.get(routeId));
              pendingContent.delete(routeId);
            });
          }
        }
      });
      observer.observe(document.body, {
        attributes: true,
        subtree: true,
        attributeFilter: ["class", "data-page-transition-state"],
      });
      const taskObserver = new PerformanceObserver((list) => {
        longTasks.push(...list.getEntries().map((entry) => entry.duration));
      });
      taskObserver.observe({ entryTypes: ["longtask"] });

      targets.forEach((target, index) => {
        window.setTimeout(() => {
          const button = buttons.find((item) => item.getAttribute("aria-label") === target.label);
          clicks.set(target.routeId, performance.now());
          button?.click();
        }, index * interval);
      });

      await new Promise((resolve) => window.setTimeout(resolve, targets.length * interval + 1_000));
      await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
      observer.disconnect();
      taskObserver.disconnect();
      return {
        active: buttons.find((button) => button.classList.contains("bg-slate-900"))?.getAttribute("aria-label") ?? null,
        acknowledgementDurations: [...acknowledgements.values()],
        contentDurations: [...contentDurations.values()],
        maxLongTask: Math.max(0, ...longTasks),
        interactiveLayers: document.querySelectorAll('[data-page-transition-layer]:not([inert])').length,
        hiddenPageQueryStarts: window.__relayNavigationPerformance?.snapshot().hiddenPageQueryStarts ?? null,
      };
    }, { targets, interval: intervalMs });
  }

  await page.reload({ waitUntil: "domcontentloaded" });
  await page.waitForTimeout(1_200);
  await runBurst(160);
  const normal = await runBurst(80);
  const extreme = await runBurst(12);
  const finalTarget = targets[targets.length - 1];
  const acknowledgementP95 = percentile(normal.acknowledgementDurations, 0.95);
  const contentP95 = percentile(normal.contentDurations, 0.95);
  if (normal.active !== finalTarget.label) throw new Error("normal burst did not end on the final route");
  if (extreme.active !== finalTarget.label) throw new Error("extreme burst did not end on the final route");
  if (normal.acknowledgementDurations.length !== targets.length) throw new Error("an 80ms click lacked acknowledgement");
  if (normal.contentDurations.length !== targets.length) throw new Error("an 80ms click lacked content commit");
  if (acknowledgementP95 > 32) throw new Error(`acknowledgement p95 ${acknowledgementP95}ms exceeds 32ms`);
  if (contentP95 > 100) throw new Error(`content p95 ${contentP95}ms exceeds 100ms`);
  if (normal.maxLongTask > 50) throw new Error(`navigation long task ${normal.maxLongTask}ms exceeds 50ms`);
  if (normal.interactiveLayers !== 1 || extreme.interactiveLayers !== 1) throw new Error("navigation left multiple interactive layers");
  if (extreme.hiddenPageQueryStarts !== 0) throw new Error(`hidden pages started ${extreme.hiddenPageQueryStarts ?? "unknown"} queries`);
  return { normal, extreme, acknowledgementP95, contentP95 };
}
