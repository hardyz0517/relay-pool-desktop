import { describe, expect, it } from "vitest";
import type { AppPageId, AppRouteId } from "@/lib/types/navigation";
import { createTourNavigationPort, TourNavigationCancelledError } from "./tourNavigation";

describe("tour navigation port", () => {
  it("resolves an already-settled same-route request", async () => {
    let current = { routeId: "dashboard" as const, shellRouteId: "dashboard" as const, sequence: 2, pending: false };
    const port = createTourNavigationPort({ navigate: (route) => { current = { ...current, routeId: route as typeof current.routeId }; }, getCurrent: () => current });
    port.navigate("dashboard", 1);
    await expect(port.waitForReady({ routeId: "dashboard", sessionId: 1, requestToken: 1, afterSequence: 1, signal: new AbortController().signal })).resolves.toMatchObject({ routeId: "dashboard", sequence: 2 });
  });

  it("ignores stale ready events and rejects aborted waiters", async () => {
    const port = createTourNavigationPort({
      navigate: () => undefined,
      getCurrent: () => ({ routeId: "dashboard", shellRouteId: "dashboard", sequence: 0, pending: true }),
    });
    const controller = new AbortController();
    port.navigate("settings", 4);
    const waiting = port.waitForReady({ routeId: "settings", sessionId: 1, requestToken: 4, afterSequence: 1, signal: controller.signal });
    port.notifyReady({ routeId: "settings", shellRouteId: "settings", sequence: 1 });
    controller.abort();
    await expect(waiting).rejects.toBeInstanceOf(TourNavigationCancelledError);
  });

  it("rejects an older waiter when a newer navigation request supersedes it", async () => {
    let current: { routeId: AppPageId; shellRouteId: AppRouteId; sequence: number; pending: boolean } = { routeId: "dashboard", shellRouteId: "dashboard", sequence: 0, pending: true };
    const port = createTourNavigationPort({
      navigate: (route) => { current = { ...current, routeId: route }; },
      getCurrent: () => current,
    });
    const first = new AbortController();
    port.navigate("settings", 1);
    const firstWait = port.waitForReady({ routeId: "settings", sessionId: 1, requestToken: 1, afterSequence: 0, signal: first.signal });
    port.navigate("routing", 2);
    await expect(firstWait).rejects.toBeInstanceOf(TourNavigationCancelledError);

    const secondWait = port.waitForReady({ routeId: "routing", sessionId: 1, requestToken: 2, afterSequence: 0, signal: new AbortController().signal });
    current = { routeId: "routing", shellRouteId: "routing", sequence: 1, pending: false };
    port.notifyReady(current);
    await expect(secondWait).resolves.toMatchObject({ routeId: "routing", sequence: 1 });
  });

  it("accepts the initial settled dashboard without requiring a new sequence", async () => {
    const current = { routeId: "dashboard" as const, shellRouteId: "dashboard" as const, sequence: 0, pending: false };
    const port = createTourNavigationPort({ navigate: () => undefined, getCurrent: () => current });
    port.navigate("dashboard", 7);
    await expect(port.waitForReady({ routeId: "dashboard", sessionId: 1, requestToken: 7, afterSequence: 0, signal: new AbortController().signal })).resolves.toMatchObject({ routeId: "dashboard", sequence: 0 });
  });

  it("rejects pending waits when disposed and ignores later ready events", async () => {
    const port = createTourNavigationPort({
      navigate: () => undefined,
      getCurrent: () => ({ routeId: "settings", shellRouteId: "settings", sequence: 0, pending: true }),
    });
    port.navigate("settings", 3);
    const waiting = port.waitForReady({ routeId: "settings", sessionId: 1, requestToken: 3, afterSequence: 0, signal: new AbortController().signal });
    port.dispose();
    port.notifyReady({ routeId: "settings", shellRouteId: "settings", sequence: 1 });
    await expect(waiting).rejects.toBeInstanceOf(TourNavigationCancelledError);
    await expect(port.waitForReady({ routeId: "settings", sessionId: 1, requestToken: 3, afterSequence: 0, signal: new AbortController().signal })).rejects.toBeInstanceOf(TourNavigationCancelledError);
  });
});
