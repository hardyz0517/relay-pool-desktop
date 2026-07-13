async (page) => {
  await page.addInitScript(() => {
    window.requestIdleCallback = (callback) =>
      window.setTimeout(
        () => callback({ didTimeout: true, timeRemaining: () => 0 }),
        5_000,
      );
    window.cancelIdleCallback = (handle) => window.clearTimeout(handle);
  });

  const cdp = await page.context().newCDPSession(page);
  await cdp.send("Emulation.setCPUThrottlingRate", { rate: 4 });

  async function runScenario(reducedMotion) {
    await page.emulateMedia({ reducedMotion: reducedMotion ? "reduce" : "no-preference" });
    await page.reload({ waitUntil: "domcontentloaded" });
    await page.waitForSelector("aside nav button");

    async function measure(routeId, settleMs = 700) {
      return page.evaluate(
        async ({ routeId: targetRouteId, settleMs: waitMs }) => {
          const button = document.querySelector(
            `[data-navigation-route-id="${targetRouteId}"]`,
          );
          if (!button) throw new Error(`missing navigation route ${targetRouteId}`);

          performance.clearMeasures();
          const startedAt = performance.now();
          const oldLayer = document.querySelector(
            '[data-page-transition-kind="shell"][data-page-transition-state="active"]',
          );
          const oldRouteId = oldLayer?.getAttribute("data-page-transition-page-id") ?? null;
          const stateEvents = [];
          const frameGaps = [];
          const longTasks = [];
          let previousFrameAt = startedAt;
          let stopped = false;

          const readStates = () =>
            Array.from(document.querySelectorAll('[data-page-transition-kind="shell"]'))
              .map((layer) => ({
                routeId: layer.getAttribute("data-page-transition-page-id"),
                state: layer.getAttribute("data-page-transition-state"),
              }))
              .filter((item) => item.routeId === oldRouteId || item.routeId === targetRouteId);

          const stateObserver = new MutationObserver(() => {
            stateEvents.push({ at: performance.now() - startedAt, states: readStates() });
          });
          stateObserver.observe(document.body, {
            attributes: true,
            attributeFilter: ["data-page-transition-state"],
            childList: true,
            subtree: true,
          });

          const taskObserver = new PerformanceObserver((list) => {
            longTasks.push(...list.getEntries().map((entry) => entry.duration));
          });
          taskObserver.observe({ entryTypes: ["longtask"] });

          function sampleFrame(timestamp) {
            frameGaps.push(timestamp - previousFrameAt);
            previousFrameAt = timestamp;
            if (!stopped) requestAnimationFrame(sampleFrame);
          }
          requestAnimationFrame(sampleFrame);

          const hiddenStartsBefore =
            window.__relayNavigationPerformance?.snapshot().hiddenPageQueryStarts ?? 0;
          button.click();
          stateEvents.push({ at: performance.now() - startedAt, states: readStates() });
          await new Promise((resolve) => window.setTimeout(resolve, waitMs));

          stopped = true;
          stateObserver.disconnect();
          taskObserver.disconnect();

          const findStateAt = (candidateRouteId, candidateState) =>
            stateEvents.find((event) =>
              event.states.some(
                (item) => item.routeId === candidateRouteId && item.state === candidateState,
              ),
            )?.at ?? null;
          const latestMark = (suffix) =>
            performance
              .getEntriesByType("mark")
              .filter((entry) => entry.name.endsWith(suffix) && entry.startTime >= startedAt)
              .at(-1)?.startTime ?? null;
          const intentAt = latestMark(":intent");
          const contentAt = latestMark(":content");
          const completeAt = latestMark(":complete");
          const hiddenStartsAfter =
            window.__relayNavigationPerformance?.snapshot().hiddenPageQueryStarts ?? 0;

          return {
            routeId: targetRouteId,
            clickToIntent: intentAt === null ? null : intentAt - startedAt,
            clickToLeaving: findStateAt(oldRouteId, "leaving"),
            clickToEntering: findStateAt(targetRouteId, "entering"),
            clickToContent: contentAt === null ? null : contentAt - startedAt,
            clickToComplete: completeAt === null ? null : completeAt - startedAt,
            maxFrameGap: Math.max(0, ...frameGaps),
            overBudgetFrames: frameGaps.filter((gap) => gap > 20).length,
            maxLongTask: Math.max(0, ...longTasks),
            hiddenQueryStarts: hiddenStartsAfter - hiddenStartsBefore,
          };
        },
        { routeId, settleMs },
      );
    }

    const coldStations = await measure("stations");
    const logs = await measure("logs");
    const dashboard = await measure("dashboard");
    const warmStations = await measure("stations");
    return { coldStations, logs, dashboard, warmStations };
  }

  try {
    return {
      motion: await runScenario(false),
      reducedMotion: await runScenario(true),
    };
  } finally {
    await cdp.send("Emulation.setCPUThrottlingRate", { rate: 1 });
    await cdp.detach();
  }
}
