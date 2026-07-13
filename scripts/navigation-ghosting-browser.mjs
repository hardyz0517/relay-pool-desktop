async (page) => {
  await page.reload({ waitUntil: "domcontentloaded" });
  await page.waitForTimeout(1_200);

  return page.evaluate(async () => {
    const readLayers = (elapsedMs) => ({
      elapsedMs,
      layers: Array.from(
        document.querySelectorAll('[data-page-transition-kind="shell"]'),
      )
        .map((element) => {
          const style = getComputedStyle(element);
          const content = element.querySelector(":scope > .app-page-transition-content");
          const contentStyle = content ? getComputedStyle(content) : null;
          return {
            animationName: style.animationName,
            backgroundColor: style.backgroundColor,
            contentAnimationName: contentStyle?.animationName ?? "none",
            contentOpacity: Number(contentStyle?.opacity ?? 1),
            display: style.display,
            inert: element.hasAttribute("inert"),
            opacity: Number(style.opacity),
            pageId: element.getAttribute("data-page-transition-page-id"),
            state: element.getAttribute("data-page-transition-state"),
          };
        })
        .filter((layer) => layer.display !== "none"),
    });

    const assertOpaqueHandoff = (samples, label) => {
      for (const sample of samples) {
        if (sample.layers.length > 2) {
          throw new Error(`${label} rendered more than two visible shell layers`);
        }
        for (const layer of sample.layers) {
          if (layer.opacity !== 1) {
            throw new Error(
              `${label} exposed ${layer.pageId} at layer opacity ${layer.opacity}`,
            );
          }
          if (layer.state === "entering" && layer.backgroundColor === "rgba(0, 0, 0, 0)") {
            throw new Error(`${label} left the entering shell background transparent`);
          }
        }
      }
    };

    const buttons = Array.from(document.querySelectorAll("aside nav button"));
    if (buttons.length < 2) {
      throw new Error("navigation probe requires at least two sidebar routes");
    }
    const buttonForRoute = (routeId) =>
      document.querySelector(`[data-navigation-route-id="${routeId}"]`);

    const normalSamples = [readLayers(-1)];
    buttonForRoute("stations").click();

    for (let index = 0; index < 18; index += 1) {
      await new Promise((resolve) => window.setTimeout(resolve, 20));
      normalSamples.push(readLayers(index * 20));
    }

    assertOpaqueHandoff(normalSamples, "normal navigation");
    const normalContentOpacities = normalSamples.flatMap((sample) =>
      sample.layers
        .filter((layer) => layer.state === "entering")
        .map((layer) => layer.contentOpacity),
    );
    if (Math.min(...normalContentOpacities) > 0.35) {
      throw new Error(
        `normal navigation content animation is not visually perceptible: ${Math.min(
          ...normalContentOpacities,
        )}`,
      );
    }
    const finalNormalLayers = normalSamples.at(-1).layers;
    if (
      finalNormalLayers.length !== 1 ||
      finalNormalLayers[0].state !== "active" ||
      finalNormalLayers[0].animationName !== "none" ||
      finalNormalLayers[0].contentOpacity !== 1
    ) {
      throw new Error("normal navigation restarted an animation after handoff completion");
    }

    const duplicateTarget = buttonForRoute("dashboard");
    duplicateTarget.click();
    window.setTimeout(() => duplicateTarget.click(), 12);
    await new Promise((resolve) => window.setTimeout(resolve, 280));
    const duplicateFinalLayers = readLayers(500).layers;
    if (
      duplicateFinalLayers.length !== 1 ||
      duplicateFinalLayers[0].pageId !== "dashboard" ||
      duplicateFinalLayers[0].state !== "active"
    ) {
      throw new Error("reclicking an entering route did not settle as one active page");
    }

    const rapidSamples = [];
    const rapidRouteIds = ["keyPool", "routing", "pricing", "channels", "changes", "logs"];
    const rapidTargets = rapidRouteIds.map(buttonForRoute);
    if (rapidTargets.some((button) => !button)) {
      throw new Error("rapid navigation probe could not resolve all route buttons");
    }
    const expectedRapidPageId = rapidRouteIds[rapidTargets.length - 1];
    rapidTargets.forEach((button, index) => {
      window.setTimeout(() => button.click(), index * 12);
    });
    for (let index = 0; index < 24; index += 1) {
      await new Promise((resolve) => requestAnimationFrame(resolve));
      rapidSamples.push(readLayers(index));
    }
    assertOpaqueHandoff(rapidSamples, "rapid navigation");
    await new Promise((resolve) => window.setTimeout(resolve, 220));
    const finalRapidLayers = readLayers(999).layers;
    if (
      finalRapidLayers.length !== 1 ||
      finalRapidLayers[0].state !== "active" ||
      finalRapidLayers[0].inert ||
      finalRapidLayers[0].pageId !== expectedRapidPageId
    ) {
      throw new Error(
        `rapid navigation settled on ${finalRapidLayers[0]?.pageId ?? "none"}, expected ${expectedRapidPageId}`,
      );
    }

    return {
      normalMaxVisibleLayers: Math.max(
        ...normalSamples.map((sample) => sample.layers.length),
      ),
      normalMinimumLayerOpacity: Math.min(
        ...normalSamples.flatMap((sample) => sample.layers.map((layer) => layer.opacity)),
      ),
      normalMinimumContentOpacity: Math.min(...normalContentOpacities),
      rapidMaxVisibleLayers: Math.max(
        ...rapidSamples.map((sample) => sample.layers.length),
      ),
      rapidMinimumLayerOpacity: Math.min(
        ...rapidSamples.flatMap((sample) => sample.layers.map((layer) => layer.opacity)),
      ),
      duplicateSettledPageId: duplicateFinalLayers[0].pageId,
      settledPageId: finalRapidLayers[0].pageId,
      expectedRapidPageId,
    };
  });
}
