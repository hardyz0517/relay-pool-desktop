import { describe, expect, it, vi } from "vitest";
import type {
  NavigationReadySnapshot,
  TourDefinition,
  TourDriverPort,
  TourId,
  TourNavigationCurrent,
  TourNavigationRequest,
  TourPreparationRegistry as TourPreparationRegistryPort,
  TourProgressV1,
} from "./tourTypes";
import { TourManager } from "./TourManager";

const definition = (id: TourId = "basic", steps: TourDefinition["steps"] = [
  { id: "first", route: "dashboard", target: { anchor: "first" }, title: "First", description: "First" },
  { id: "second", route: "stations", target: { anchor: "second" }, title: "Second", description: "Second" },
]): TourDefinition => ({
  id,
  category: id === "full" || id === "basic" ? "recommended" : "page",
  order: 1,
  title: id,
  summary: id,
  revision: 1,
  steps,
});

function harness(options: {
  target?: () => Promise<HTMLElement>;
  definitions?: readonly TourDefinition[];
  developerMode?: boolean;
} = {}) {
  const callbacks: Parameters<TourDriverPort["showStep"]>[0]["callbacks"][] = [];
  const driver: TourDriverPort = {
    beginSession: vi.fn(),
    showStep: vi.fn((input) => callbacks.push(input.callbacks)),
    destroy: vi.fn(),
  };
  let current: TourNavigationCurrent = { routeId: "dashboard", shellRouteId: "dashboard", sequence: 0, pending: false };
  const navigation = {
    navigate: vi.fn((routeId: TourNavigationRequest["routeId"]) => { current = { ...current, routeId, shellRouteId: routeId as TourNavigationCurrent["shellRouteId"], sequence: current.sequence + 1, pending: false }; }),
    getCurrent: vi.fn(() => current),
    waitForReady: vi.fn(async (request: TourNavigationRequest): Promise<NavigationReadySnapshot> => ({ routeId: request.routeId, shellRouteId: request.routeId as TourNavigationCurrent["shellRouteId"], sequence: current.sequence })),
  };
  const target = { waitForTarget: vi.fn(options.target ?? (async () => ({} as HTMLElement))) };
  const preparation = {
    has: vi.fn(() => true),
    run: vi.fn<TourPreparationRegistryPort["run"]>(async () => null),
  };
  let blockingModal = false;
  let stored: TourProgressV1 = { schemaVersion: 1, tours: {} };
  const progress = {
    getSnapshot: vi.fn(() => stored),
    commitCompletion: vi.fn((id: TourId, revision: number) => { stored = { ...stored, tours: { ...stored.tours, [id]: { revision, state: "completed", updatedAt: 1 } } }; return true; }),
    commitSkipped: vi.fn((id: TourId, revision: number) => { stored = { ...stored, tours: { ...stored.tours, [id]: { revision, state: "skipped", updatedAt: 1 } } }; return true; }),
    reset: vi.fn(() => true),
  };
  const manager = new TourManager({
    driver,
    navigation,
    targetResolver: target,
    preparation,
    progress,
    catalog: options.definitions ?? [definition()],
    isDeveloperMode: () => options.developerMode === true,
    hasBlockingModal: () => blockingModal,
  });
  return {
    manager,
    driver,
    callbacks,
    navigation,
    target,
    preparation,
    progress,
    setBlockingModal(value: boolean) {
      blockingModal = value;
    },
  };
}

async function flush(): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
  await new Promise<void>((resolve) => setTimeout(resolve, 0));
  await Promise.resolve();
}

describe("TourManager", () => {
  it("returns a stable snapshot reference between updates", () => {
    const h = harness();
    expect(h.manager.getSnapshot()).toBe(h.manager.getSnapshot());
    h.manager.start("basic", "test");
    expect(h.manager.getSnapshot()).toBe(h.manager.getSnapshot());
  });

  it("completes immediately from the final visible step", async () => {
    const h = harness();
    h.manager.start("basic", "test");
    await flush();
    expect(h.manager.getSnapshot().phase).toBe("running");
    expect(h.driver.showStep).toHaveBeenCalledOnce();
    h.callbacks[0].next();
    await flush();
    expect(h.manager.getSnapshot().phase).toBe("running");
    h.callbacks[1].next();
    expect(h.progress.commitCompletion).toHaveBeenCalledWith("basic", 1, expect.any(Number));
    expect(h.manager.getSnapshot().phase).toBe("completed");
  });

  it("records aggregate tutorial completion independently from other tutorials", async () => {
    const full: TourDefinition = {
      ...definition("full", [
        { id: "only", route: "dashboard", target: { anchor: "only" }, title: "Only", description: "Only" },
      ]),
    };
    const h = harness({ definitions: [full, definition("basic", []), definition("proxy", [])] });

    h.manager.start("full", "test");
    await flush();
    h.callbacks[0].next();

    expect(h.progress.commitCompletion.mock.calls.map(([tourId]) => tourId)).toEqual(["full"]);
    expect(h.manager.getSnapshot().phase).toBe("completed");
  });

  it("records skip and ignores callbacks from a superseded session", async () => {
    const resolvers: Array<(element: HTMLElement) => void> = [];
    const target = () => new Promise<HTMLElement>((resolve) => { resolvers.push(resolve); });
    const h = harness({ target });
    h.manager.start("basic", "test");
    await flush();
    h.manager.start("basic", "test");
    await flush();
    resolvers.forEach((resolve) => resolve({} as HTMLElement));
    await flush();
    expect(h.driver.showStep).toHaveBeenCalledOnce();
    h.manager.skip();
    expect(h.progress.commitSkipped).toHaveBeenCalledTimes(2);
    expect(h.manager.getSnapshot().phase).toBe("skipped");
  });

  it("skips optional unavailable steps and errors on required unavailable steps", async () => {
    const optional = definition("proxy", [
      { id: "optional", route: "dashboard", target: { anchor: "missing" }, title: "Missing", description: "", optional: true },
      { id: "visible", route: "dashboard", target: { anchor: "visible" }, title: "Visible", description: "" },
    ]);
    let targetCalls = 0;
    const h = harness({ definitions: [optional], target: async () => {
      targetCalls += 1;
      if (targetCalls === 1) throw new Error("target timeout");
      return {} as HTMLElement;
    } });
    h.manager.start("proxy", "test");
    await flush();
    expect(h.manager.getSnapshot().phase).toBe("running");
    expect(h.manager.getSnapshot().stepIndex).toBe(1);

    const required = definition("station-setup", [{ id: "required", route: "dashboard", target: { anchor: "missing" }, title: "Missing", description: "" }]);
    const failed = harness({ definitions: [required], target: async () => { throw new Error("target timeout"); } });
    failed.manager.start("station-setup", "test");
    await flush();
    expect(failed.manager.getSnapshot()).toMatchObject({ phase: "error", message: "当前步骤暂不可用，请重试或退出教程" });
  });

  it("completes when the final optional step is unavailable", async () => {
    const h = harness({
      definitions: [definition("proxy", [{ id: "optional", route: "dashboard", target: { anchor: "missing" }, title: "Missing", description: "", optional: true }])],
      target: async () => { throw new Error("target timeout"); },
    });
    h.manager.start("proxy", "test");
    await flush();
    expect(h.progress.commitCompletion).toHaveBeenCalledOnce();
    expect(h.manager.getSnapshot().phase).toBe("completed");
  });

  it("can retry a required step after a recoverable target failure", async () => {
    let attempts = 0;
    const h = harness({
      definitions: [definition("station-setup", [{ id: "required", route: "dashboard", target: { anchor: "required" }, title: "Required", description: "" }])],
      target: async () => {
        attempts += 1;
        if (attempts === 1) throw new Error("target timeout");
        return {} as HTMLElement;
      },
    });
    h.manager.start("station-setup", "test");
    await flush();
    expect(h.manager.getSnapshot().phase).toBe("error");
    h.manager.retry();
    await flush();
    expect(h.manager.getSnapshot().phase).toBe("running");
    expect(h.driver.showStep).toHaveBeenCalledTimes(1);
  });

  it("supports auto-start suppression for handled progress and external destroy as skip", async () => {
    const h = harness();
    h.progress.getSnapshot.mockReturnValue({ schemaVersion: 1, tours: { basic: { revision: 1, state: "skipped", updatedAt: 1 } } });
    expect(h.manager.start("basic", "auto")).toBe(false);
    expect(h.driver.showStep).not.toHaveBeenCalled();

    const fresh = harness();
    fresh.manager.start("basic", "test");
    await flush();
    fresh.callbacks[0].destroyed();
    expect(fresh.manager.getSnapshot().phase).toBe("skipped");
  });

  it("does not auto-start a newer revision after any earlier revision was handled", () => {
    const h = harness({ definitions: [{ ...definition(), revision: 3 }] });
    h.progress.getSnapshot.mockReturnValue({
      schemaVersion: 1,
      tours: { basic: { revision: 1, state: "completed", updatedAt: 1 } },
    });

    expect(h.manager.start("basic", "auto")).toBe(false);
    expect(h.navigation.navigate).not.toHaveBeenCalled();
    expect(h.manager.start("basic", "settings")).toBe(true);
  });

  it("does not supersede a live session for an invalid or blocked start", async () => {
    const h = harness();
    h.manager.start("basic", "test");
    await flush();
    const before = h.manager.getSnapshot();
    expect(h.manager.start("monitoring", "test")).toBe(false);
    expect(h.progress.commitSkipped).not.toHaveBeenCalled();
    expect(h.manager.getSnapshot()).toEqual(before);
  });

  it("does not supersede a live session when a business modal is already open", async () => {
    const h = harness();
    h.manager.start("basic", "test");
    await flush();
    const before = h.manager.getSnapshot();

    h.setBlockingModal(true);
    expect(h.manager.start("basic", "settings")).toBe(false);

    expect(h.manager.getSnapshot()).toEqual(before);
    expect(h.driver.destroy).not.toHaveBeenCalled();
  });

  it("reports a blocked start without creating a session error overlay state", () => {
    const h = harness();
    h.setBlockingModal(true);

    expect(h.manager.start("basic", "settings")).toBe(false);
    expect(h.manager.getSnapshot()).toEqual({
      phase: "idle",
      tourId: null,
      stepIndex: 0,
      stepCount: 0,
      source: null,
      message: "请先关闭当前对话框，再开始教程",
    });
    h.manager.retry();
    h.manager.close();
    expect(h.navigation.navigate).not.toHaveBeenCalled();
  });

  it("rejects a developer-only tour before replacing a live session", async () => {
    const advanced = definition("advanced", [
      {
        id: "advanced-step",
        route: "routing",
        target: { anchor: "advanced" },
        title: "Advanced",
        description: "Advanced",
      },
    ]);
    const h = harness({ definitions: [definition(), { ...advanced, requires: "developer-mode" }] });
    h.manager.start("basic", "test");
    await flush();
    const before = h.manager.getSnapshot();

    h.manager.start("advanced", "settings");

    expect(h.manager.getSnapshot()).toEqual(before);
    expect(h.navigation.navigate).toHaveBeenCalledTimes(1);
    expect(h.driver.destroy).not.toHaveBeenCalled();
    expect(h.progress.commitSkipped).not.toHaveBeenCalled();
  });

  it("allows a developer-only tour when developer mode is enabled", async () => {
    const advanced = definition("advanced", [
      {
        id: "advanced-step",
        route: "routing",
        target: { anchor: "advanced" },
        title: "Advanced",
        description: "Advanced",
      },
    ]);
    const h = harness({
      definitions: [{ ...advanced, requires: "developer-mode" }],
      developerMode: true,
    });

    h.manager.start("advanced", "settings");
    await flush();

    expect(h.manager.getSnapshot().phase).toBe("running");
    expect(h.navigation.navigate).toHaveBeenCalledWith("routing", expect.any(Number));
  });

  it("surfaces skipped persistence failures without blocking exit", async () => {
    const h = harness();
    h.progress.commitSkipped.mockReturnValue(false);
    h.manager.start("basic", "test");
    await flush();
    h.manager.skip();
    expect(h.manager.getSnapshot()).toMatchObject({ phase: "skipped", message: "教程已跳过，但进度未能持久化" });
  });

  it("does not reset persisted progress while a tutorial session is active", async () => {
    const h = harness();
    h.manager.start("basic", "test");
    await flush();

    h.manager.resetProgress();

    expect(h.progress.reset).not.toHaveBeenCalled();
    expect(h.manager.getSnapshot()).toMatchObject({
      phase: "running",
      message: "请先退出当前教程再重置进度",
    });
  });

  it("reports reset persistence failure without creating session error controls", () => {
    const h = harness();
    h.progress.reset.mockReturnValue(false);

    h.manager.resetProgress();

    expect(h.manager.getSnapshot()).toEqual({
      phase: "idle",
      tourId: null,
      stepIndex: 0,
      stepCount: 0,
      source: null,
      message: "教程进度未能持久化",
    });
  });

  it("navigates to a page before preparing its view and resolving the target", async () => {
    const h = harness();
    h.manager.start("basic", "test");
    await flush();

    expect(h.navigation.navigate.mock.invocationCallOrder[0]).toBeLessThan(
      h.navigation.waitForReady.mock.invocationCallOrder[0],
    );
    expect(h.navigation.waitForReady.mock.invocationCallOrder[0]).toBeLessThan(
      h.preparation.run.mock.invocationCallOrder[0],
    );
    expect(h.preparation.run.mock.invocationCallOrder[0]).toBeLessThan(
      h.target.waitForTarget.mock.invocationCallOrder[0],
    );
  });

  it("restores each prepared view exactly once on step change and session close", async () => {
    const cleanupFirst = vi.fn();
    const cleanupSecond = vi.fn();
    const h = harness();
    h.preparation.run
      .mockResolvedValueOnce(cleanupFirst)
      .mockResolvedValueOnce(cleanupSecond);

    h.manager.start("basic", "test");
    await flush();
    h.callbacks[0].next();
    expect(cleanupFirst).toHaveBeenCalledOnce();
    await flush();
    h.manager.close();

    expect(cleanupFirst).toHaveBeenCalledOnce();
    expect(cleanupSecond).toHaveBeenCalledOnce();
  });

  it.each([
    ["skip", (manager: TourManager) => manager.skip()],
    ["complete", (manager: TourManager) => manager.next()],
    ["dispose", (manager: TourManager) => manager.dispose()],
  ])("restores preparation exactly once on %s", async (_reason, finish) => {
    const cleanup = vi.fn();
    const singleStep = definition("basic", [
      { id: "only", route: "dashboard", target: { anchor: "only" }, title: "Only", description: "" },
    ]);
    const h = harness({ definitions: [singleStep] });
    h.preparation.run.mockResolvedValueOnce(cleanup);

    h.manager.start("basic", "test");
    await flush();
    finish(h.manager);
    finish(h.manager);

    expect(cleanup).toHaveBeenCalledOnce();
  });
});
