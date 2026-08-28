import { describe, expect, it } from "vitest";
import type { AppPageId } from "@/lib/types/navigation";
import { PUBLISHED_TOUR_IDS, PUBLISHED_TOURS, TOUR_CATALOG, getTourDefinition, isPublishedTourId } from "./tourCatalog";

const APP_PAGE_IDS = new Set<AppPageId>([
  "dashboard", "stations", "keyPool", "routing", "pricing", "channels", "collectors",
  "runtimeDiagnostics", "changes", "logs", "settings", "addProvider", "editProvider",
  "stationDetail", "addKey", "editKey", "modelBasePrices", "changeSettings",
]);

const PREPARATION_KEYS = new Set([
  "none", "routing-status-tab", "routing-settings-tab", "channels-local-tab",
  "channels-official-tab", "channels-monitoring-tab",
]);

describe("tourCatalog", () => {
  it("publishes recommended and page tours while keeping legacy ids unavailable", () => {
    expect(PUBLISHED_TOUR_IDS).toEqual([
      "full", "basic", "dashboard", "stations", "key-pool", "routing",
      "pricing", "channels", "changes", "logs", "settings",
    ]);
    expect(getTourDefinition("proxy")).toBeUndefined();
    expect(getTourDefinition("station-setup")).toBeUndefined();
    expect(getTourDefinition("monitoring")).toBeUndefined();
    expect(isPublishedTourId("basic")).toBe(true);
    expect(isPublishedTourId("proxy")).toBe(false);
  });

  it("keeps definitions serializable, ordered and free of developer-only steps", () => {
    const stepIds = new Set<string>();
    const orderByCategory = new Map<string, number>();

    for (const definition of PUBLISHED_TOURS) {
      expect(definition.id).toBeDefined();
      expect(Number.isInteger(definition.order)).toBe(true);
      expect(definition.order).toBeGreaterThan(0);
      expect(definition.order).toBeGreaterThan(orderByCategory.get(definition.category) ?? 0);
      orderByCategory.set(definition.category, definition.order);
      expect(Number.isInteger(definition.revision)).toBe(true);
      expect(definition.revision).toBeGreaterThan(0);
      expect(definition.estimatedMinutes).toBeGreaterThan(0);
      expect(definition.requires).not.toBe("developer-mode");

      if (definition.category === "page") {
        expect(definition.steps.length).toBeGreaterThanOrEqual(4);
        expect(definition.steps.length).toBeLessThanOrEqual(7);
      }

      for (const tourStep of definition.steps) {
        expect(APP_PAGE_IDS.has(tourStep.route)).toBe(true);
        expect(tourStep.id).not.toHaveLength(0);
        expect(tourStep.target.anchor).not.toHaveLength(0);
        expect(stepIds.has(tourStep.id)).toBe(false);
        expect(PREPARATION_KEYS.has(tourStep.prepareKey ?? "none")).toBe(true);
        expect(tourStep.requires).not.toBe("developer-mode");
        expect(tourStep.route).not.toBe("collectors");
        expect(tourStep.route).not.toBe("runtimeDiagnostics");
        stepIds.add(tourStep.id);
      }
    }
  });

  it("maintains a short introduction and an independently curated complete experience", () => {
    expect(TOUR_CATALOG.basic.steps).toHaveLength(7);
    expect(TOUR_CATALOG.full.steps.length).toBeGreaterThanOrEqual(12);
    expect(TOUR_CATALOG.full.steps.length).toBeLessThanOrEqual(18);
    expect(TOUR_CATALOG.full.steps.every((tourStep) => tourStep.id.startsWith("full-"))).toBe(true);
    expect(TOUR_CATALOG.full.steps.some((tourStep) => tourStep.route === "settings")).toBe(true);
    expect(TOUR_CATALOG.full.steps).toContainEqual(expect.objectContaining({
      id: "full-settings-system-proxy",
      route: "settings",
      target: { anchor: "settings-network" },
    }));
    expect(TOUR_CATALOG.full.steps).not.toEqual(
      Object.values(TOUR_CATALOG)
        .filter((tour) => tour.id !== "full")
        .flatMap((tour) => tour.steps),
    );
  });
});
