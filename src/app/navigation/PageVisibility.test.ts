// @vitest-environment jsdom

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useInteractionActivity } from "@/components/ui/InteractionActivity";
import { useActivityQuery } from "@/lib/query/useActivityQuery";
import {
  getPageRetentionDecision,
} from "./pageRetentionPolicy";
import {
  createPageVisibility,
  PageVisibilityProvider,
  shellPageVisibilityForState,
  transientPageVisibility,
  usePageVisibility,
} from "./PageVisibility";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let host: HTMLDivElement;
let root: Root;

beforeEach(() => {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});

afterEach(async () => {
  await act(async () => {
    root.unmount();
  });
  host.remove();
});

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

  it("retains only current and transition shell pages by default", () => {
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
    })).toEqual({ retain: false, reason: "default-unmounted" });
  });

  it("publishes canonical visibility to query and interaction consumers", async () => {
    const snapshots: Array<{
      interactive: boolean;
      queryEnabled: boolean;
      reason: string;
    }> = [];

    function Probe() {
      const visibility = usePageVisibility();
      const interactive = useInteractionActivity();
      snapshots.push({
        interactive,
        queryEnabled: visibility.queryEnabled,
        reason: visibility.reason,
      });
      return null;
    }

    await act(async () => {
      root.render(
        createElement(
          PageVisibilityProvider,
          {
            visibility: shellPageVisibilityForState("inactive"),
            children: createElement(Probe),
          },
        ),
      );
    });

    expect(snapshots[snapshots.length - 1]).toEqual({
      interactive: false,
      queryEnabled: false,
      reason: "inactive",
    });

    await act(async () => {
      root.render(
        createElement(
          PageVisibilityProvider,
          {
            visibility: createPageVisibility({ kind: "foreground", reason: "active" }),
            children: createElement(Probe),
          },
        ),
      );
    });

    expect(snapshots[snapshots.length - 1]).toEqual({
      interactive: true,
      queryEnabled: true,
      reason: "active",
    });
  });

  it("prevents hidden page query execution until visibility becomes foreground", async () => {
    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    const queryFn = vi.fn(async () => "ready");

    function Probe() {
      useActivityQuery({
        queryKey: ["page-visibility-contract"],
        queryFn,
      });
      return null;
    }

    const renderWithVisibility = (state: "inactive" | "active") =>
      root.render(
        createElement(
          QueryClientProvider,
          {
            client: queryClient,
            children: createElement(
              PageVisibilityProvider,
              {
                visibility: shellPageVisibilityForState(state),
                children: createElement(Probe),
              },
            ),
          },
        ),
      );

    await act(async () => {
      renderWithVisibility("inactive");
    });

    expect(queryFn).not.toHaveBeenCalled();

    await act(async () => {
      renderWithVisibility("active");
      await Promise.resolve();
    });

    expect(queryFn).toHaveBeenCalledTimes(1);
    queryClient.clear();
  });
});
