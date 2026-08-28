// @vitest-environment jsdom

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it } from "vitest";
import { TourOverlay } from "./TourOverlay";
import type { TourManagerApi, TourManagerSnapshot } from "./tourTypes";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const snapshot: TourManagerSnapshot = {
  phase: "completed",
  tourId: "basic",
  stepIndex: 1,
  stepCount: 2,
  source: "test",
  message: null,
};

describe("TourOverlay", () => {
  let root: Root | null = null;

  afterEach(() => {
    if (root) act(() => root?.unmount());
    root = null;
    document.body.replaceChildren();
  });

  it("renders an accessible announcement from the manager snapshot", () => {
    const manager: TourManagerApi = {
      getSnapshot: () => snapshot,
      subscribe: (listener) => {
        listener(snapshot);
        return () => undefined;
      },
      start: () => true,
      next: () => undefined,
      previous: () => undefined,
      retry: () => undefined,
      skip: () => undefined,
      close: () => undefined,
      resetProgress: () => undefined,
      dispose: () => undefined,
    };
    const host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    act(() => root?.render(<TourOverlay manager={manager} />));
    expect(host.querySelector("[aria-live='polite']")?.textContent).toContain("教程已完成");
  });
});
