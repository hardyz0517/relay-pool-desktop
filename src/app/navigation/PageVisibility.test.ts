import { describe, expect, it } from "vitest";
import {
  getPageRetentionDecision,
  MAX_RETAINED_SHELL_PAGES,
} from "./pageRetentionPolicy";
import {
  shellPageVisibilityForState,
  transientPageVisibility,
} from "./PageVisibility";

describe("PageVisibility policy", () => {
  it("treats active and entering shell pages as foreground", () => {
    for (const state of ["active", "entering"] as const) {
      expect(shellPageVisibilityForState(state)).toMatchObject({
        kind: "foreground",
        interactive: true,
        queryEnabled: true,
        reason: state,
      });
    }
  });

  it("treats leaving, inactive and transient-covered shell pages as background", () => {
    expect(shellPageVisibilityForState("leaving")).toMatchObject({
      kind: "background",
      interactive: false,
      queryEnabled: false,
      reason: "leaving",
    });
    expect(shellPageVisibilityForState("inactive")).toMatchObject({
      kind: "background",
      interactive: false,
      queryEnabled: false,
      reason: "inactive",
    });
    expect(shellPageVisibilityForState("background")).toMatchObject({
      kind: "background",
      interactive: false,
      queryEnabled: false,
      reason: "covered-by-transient",
    });
  });

  it("keeps transient pages foreground only while present", () => {
    expect(transientPageVisibility(true)).toMatchObject({
      kind: "foreground",
      queryEnabled: true,
      reason: "transient-active",
    });
    expect(transientPageVisibility(false)).toMatchObject({
      kind: "background",
      queryEnabled: false,
      reason: "transient-exiting",
    });
  });

  it("keeps current and transition shell pages retained before legacy allowlist pruning", () => {
    expect(getPageRetentionDecision({
      routeId: "stations",
      activeRouteId: "stations",
      previousRouteId: "dashboard",
    })).toEqual({ retain: true, reason: "active" });
    expect(getPageRetentionDecision({
      routeId: "dashboard",
      activeRouteId: "stations",
      previousRouteId: "dashboard",
    })).toEqual({ retain: true, reason: "transition" });
    expect(getPageRetentionDecision({
      routeId: "settings",
      activeRouteId: "stations",
      previousRouteId: null,
    })).toEqual({ retain: true, reason: "legacy-allowlist" });
    expect(MAX_RETAINED_SHELL_PAGES).toBe(10);
  });
});
