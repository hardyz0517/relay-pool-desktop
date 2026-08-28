// @vitest-environment jsdom

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  TourTargetResolver,
  TourTargetResolverError,
  escapeCssIdent,
} from "./tourTargetResolver";

function setLayout(element: HTMLElement, width = 120, height = 32) {
  Object.defineProperty(element, "getBoundingClientRect", {
    configurable: true,
    value: () => ({
      bottom: height,
      height,
      left: 0,
      right: width,
      top: 0,
      width,
      x: 0,
      y: 0,
      toJSON: () => ({}),
    }),
  });
  return element;
}

function layer(
  route: string,
  state: "active" | "background" | "entering" | "inactive" = "active",
  kind: "shell" | "transient" = "shell",
) {
  const node = document.createElement("section");
  node.dataset.pageTransitionLayer = "";
  node.dataset.pageTransitionKind = kind;
  node.dataset.pageTransitionPageId = route;
  node.dataset.pageTransitionState = state;
  return node;
}

function target(anchor: string, width = 120, height = 32) {
  const node = document.createElement("button");
  node.dataset.tour = anchor;
  return setLayout(node, width, height);
}

function appendTarget(
  route: string,
  anchor: string,
  options?: {
    state?: "active" | "background" | "entering" | "inactive";
    kind?: "shell" | "transient";
    targetWidth?: number;
    targetHeight?: number;
  },
) {
  const root = layer(route, options?.state, options?.kind);
  const node = target(anchor, options?.targetWidth, options?.targetHeight);
  root.append(node);
  document.body.append(root);
  return { root, node };
}

describe("TourTargetResolver", () => {
  beforeEach(() => {
    document.body.replaceChildren();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("escapes anchors before using them in an attribute selector", () => {
    const anchor = 'proxy"] .not-a-tour-target';
    const { node } = appendTarget("settings", anchor);
    const resolver = new TourTargetResolver();

    expect(escapeCssIdent(anchor)).toContain('\\"');
    expect(resolver.resolveTarget(anchor, "settings")).toBe(node);
    expect(resolver.resolveTarget("not-a-tour-target", "settings")).toBeNull();
  });

  it("chooses only the active layer when prewarmed pages contain the same anchor", () => {
    const background = appendTarget("dashboard", "overview", { state: "inactive" });
    const active = appendTarget("dashboard", "overview", { state: "active" });
    const resolver = new TourTargetResolver();

    expect(resolver.resolveTarget("overview", "dashboard")).toBe(active.node);
    expect(background.node.isConnected).toBe(true);
  });

  it("accepts a foreground shell entering layer and an active transient layer", () => {
    const entering = appendTarget("settings", "settings-proxy", { state: "entering" });
    const resolver = new TourTargetResolver();
    expect(resolver.resolveTarget("settings-proxy", "settings")).toBe(entering.node);

    document.body.replaceChildren();
    const transient = appendTarget("addProvider", "provider-form", {
      state: "active",
      kind: "transient",
    });
    expect(resolver.resolveTarget("provider-form", "addProvider")).toBe(transient.node);
  });

  it("accepts an explicitly global anchor outside page transition layers", () => {
    const nav = document.createElement("nav");
    nav.dataset.tourScope = "global";
    const node = target("global-navigation");
    nav.append(node);
    document.body.append(nav);

    expect(new TourTargetResolver().resolveTarget("global-navigation", "dashboard")).toBe(node);
  });

  it("still applies visibility guards to global anchors", () => {
    const hiddenScope = document.createElement("nav");
    hiddenScope.dataset.tourScope = "global";
    hiddenScope.setAttribute("aria-hidden", "true");
    const hiddenNode = target("global-health");
    hiddenScope.append(hiddenNode);

    const inertScope = document.createElement("nav");
    inertScope.dataset.tourScope = "global";
    inertScope.setAttribute("inert", "");
    const inertNode = target("global-settings");
    inertScope.append(inertNode);
    document.body.append(hiddenScope, inertScope);

    const resolver = new TourTargetResolver();
    expect(resolver.resolveTarget("global-health", "dashboard")).toBeNull();
    expect(resolver.resolveTarget("global-settings", "settings")).toBeNull();
  });

  it("selects the first visible global candidate when a stale hidden copy remains", () => {
    const staleScope = document.createElement("nav");
    staleScope.dataset.tourScope = "global";
    staleScope.style.display = "none";
    const staleNode = target("global-sidebar");
    staleScope.append(staleNode);

    const activeScope = document.createElement("nav");
    activeScope.dataset.tourScope = "global";
    const activeNode = target("global-sidebar");
    activeScope.append(activeNode);
    document.body.append(staleScope, activeScope);

    expect(new TourTargetResolver().resolveTarget("global-sidebar", "dashboard")).toBe(activeNode);
  });

  it.each([
    ["inert ancestor", (node: HTMLElement) => node.parentElement?.setAttribute("inert", "")],
    ["aria-hidden ancestor", (node: HTMLElement) => node.parentElement?.setAttribute("aria-hidden", "true")],
    ["display none", (node: HTMLElement) => { node.style.display = "none"; }],
    ["visibility hidden", (node: HTMLElement) => { node.style.visibility = "hidden"; }],
    ["opacity zero", (node: HTMLElement) => { node.style.opacity = "0"; }],
    ["hidden ancestor", (node: HTMLElement) => { node.parentElement?.setAttribute("hidden", ""); }],
    ["zero width", (node: HTMLElement) => {
      Object.defineProperty(node, "getBoundingClientRect", { configurable: true, value: () => ({ width: 0, height: 32 }) });
    }],
    ["zero height", (node: HTMLElement) => {
      Object.defineProperty(node, "getBoundingClientRect", { configurable: true, value: () => ({ width: 120, height: 0 }) });
    }],
  ] as const)("rejects %s targets", (_name, hide) => {
    const { node } = appendTarget("dashboard", "health");
    hide(node);
    expect(new TourTargetResolver().resolveTarget("health", "dashboard")).toBeNull();
  });

  it("rejects detached targets and targets from another route", () => {
    const { node } = appendTarget("dashboard", "health");
    const resolver = new TourTargetResolver();
    node.remove();
    expect(resolver.resolveTarget("health", "dashboard")).toBeNull();

    appendTarget("settings", "health");
    expect(resolver.resolveTarget("health", "dashboard")).toBeNull();
  });

  it("waits for a target added after a DOM mutation", async () => {
    const resolver = new TourTargetResolver({ timeoutMs: 500 });
    const promise = resolver.waitForTarget("delayed", "dashboard", new AbortController().signal);

    const root = layer("dashboard");
    document.body.append(root);
    const node = target("delayed");
    root.append(node);

    await expect(promise).resolves.toBe(node);
  });

  it("retries layout after a hidden target becomes visible", async () => {
    const resolver = new TourTargetResolver({ timeoutMs: 500 });
    const root = layer("dashboard");
    const node = target("eventual");
    node.style.display = "none";
    root.append(node);
    document.body.append(root);

    const promise = resolver.waitForTarget("eventual", "dashboard", new AbortController().signal);
    node.style.display = "block";
    await expect(promise).resolves.toBe(node);
  });

  it("times out with cleanup when a target never appears", async () => {
    const disconnect = vi.spyOn(MutationObserver.prototype, "disconnect");
    const resolver = new TourTargetResolver({ timeoutMs: 20 });
    const error = await resolver
      .waitForTarget("missing", "dashboard", new AbortController().signal)
      .catch((value: unknown) => value);

    expect(error).toBeInstanceOf(TourTargetResolverError);
    expect(error).toMatchObject({ code: "timeout", anchor: "missing", route: "dashboard" });
    expect(disconnect).toHaveBeenCalled();
  });

  it("aborts promptly and disconnects observers", async () => {
    const disconnect = vi.spyOn(MutationObserver.prototype, "disconnect");
    const controller = new AbortController();
    const resolver = new TourTargetResolver({ timeoutMs: 500 });
    const promise = resolver.waitForTarget("cancelled", "dashboard", controller.signal);
    controller.abort();

    await expect(promise).rejects.toMatchObject({ code: "aborted" });
    expect(disconnect).toHaveBeenCalled();
  });

  it("rejects invalid and already-aborted requests without observing", async () => {
    const disconnect = vi.spyOn(MutationObserver.prototype, "disconnect");
    const resolver = new TourTargetResolver();
    await expect(resolver.waitForTarget("   ", "dashboard", new AbortController().signal))
      .rejects.toMatchObject({ code: "invalid-anchor" });

    const controller = new AbortController();
    controller.abort();
    await expect(resolver.waitForTarget("health", "dashboard", controller.signal))
      .rejects.toMatchObject({ code: "aborted" });
    expect(disconnect).not.toHaveBeenCalled();
  });
});
