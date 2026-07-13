async (page) => {
  await page.reload({ waitUntil: "domcontentloaded" });
  await page.waitForTimeout(1_200);

  return page.evaluate(async () => {
    document.querySelector('[data-navigation-route-id="stations"]')?.click();
    await new Promise((resolve) => window.setTimeout(resolve, 240));

    const activeStationsLayer = document.querySelector(
      '[data-page-transition-kind="shell"]' +
        '[data-page-transition-page-id="stations"]' +
        '[data-page-transition-state="active"]',
    );
    const addProviderButton = Array.from(
      activeStationsLayer?.querySelectorAll("button") ?? [],
    ).find(
      (button) => button.textContent?.trim() === "添加供应商",
    );
    if (!addProviderButton) {
      throw new Error("transient probe could not find the add-provider entry point");
    }

    const readLayers = () =>
      Array.from(document.querySelectorAll('[data-page-transition-layer]'))
        .map((element) => {
          const style = getComputedStyle(element);
          const content = element.querySelector(":scope > .app-page-transition-content");
          const contentStyle = content ? getComputedStyle(content) : null;
          return {
            backgroundColor: style.backgroundColor,
            contentAnimationName: contentStyle?.animationName ?? "none",
            contentOpacity: Number(contentStyle?.opacity ?? 1),
            display: style.display,
            kind: element.getAttribute("data-page-transition-kind"),
            opacity: Number(style.opacity),
            state: element.getAttribute("data-page-transition-state"),
          };
        })
        .filter((layer) => layer.display !== "none");

    const assertOpaqueTransient = (samples, label) => {
      for (const sample of samples) {
        for (const layer of sample.filter((item) => item.kind === "transient")) {
          if (layer.opacity !== 1) {
            throw new Error(`${label} exposed the transient overlay at opacity ${layer.opacity}`);
          }
          if (layer.backgroundColor === "rgba(0, 0, 0, 0)") {
            throw new Error(`${label} left the transient overlay background transparent`);
          }
        }
      }
    };

    const entrySamples = [];
    addProviderButton.click();
    for (let index = 0; index < 14; index += 1) {
      await new Promise((resolve) => window.setTimeout(resolve, 20));
      entrySamples.push(readLayers());
    }
    assertOpaqueTransient(entrySamples, "transient entry");
    const entryContentOpacities = entrySamples.flatMap((sample) =>
      sample
        .filter((layer) => layer.kind === "transient")
        .map((layer) => layer.contentOpacity),
    );
    if (!entryContentOpacities.some((opacity) => opacity > 0 && opacity < 1)) {
      throw new Error("transient entry did not animate its content");
    }
    if (Math.min(...entryContentOpacities) > 0.1) {
      throw new Error("transient entry content animation is not visually perceptible");
    }

    const backButton = Array.from(
      document.querySelectorAll('[data-page-transition-kind="transient"] button'),
    ).find(
      (button) => button.getAttribute("aria-label") === "返回中转站",
    );
    if (!backButton) {
      const buttonLabels = Array.from(document.querySelectorAll("button"))
        .map((button) => button.textContent?.trim())
        .filter(Boolean)
        .slice(0, 20);
      throw new Error(
        `transient probe could not find the return action: ${buttonLabels.join(" | ")}`,
      );
    }

    const exitSamples = [];
    backButton.click();
    for (let index = 0; index < 14; index += 1) {
      await new Promise((resolve) => window.setTimeout(resolve, 20));
      exitSamples.push(readLayers());
    }
    assertOpaqueTransient(exitSamples, "transient exit");
    const finalLayers = readLayers();
    if (
      finalLayers.length !== 1 ||
      finalLayers[0].kind !== "shell" ||
      finalLayers[0].state !== "active"
    ) {
      throw new Error("transient exit did not settle on one active shell layer");
    }

    return {
      entryMinimumContentOpacity: Math.min(...entryContentOpacities),
      entryMinimumOverlayOpacity: Math.min(
        ...entrySamples.flatMap((sample) =>
          sample.filter((layer) => layer.kind === "transient").map((layer) => layer.opacity),
        ),
      ),
      exitMinimumOverlayOpacity: Math.min(
        ...exitSamples.flatMap((sample) =>
          sample.filter((layer) => layer.kind === "transient").map((layer) => layer.opacity),
        ),
      ),
      settledState: finalLayers[0].state,
    };
  });
}
