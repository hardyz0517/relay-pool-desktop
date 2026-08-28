// @vitest-environment jsdom

import { act } from "react";
import { useState } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";
import { PUBLISHED_TOURS } from "@/app/tours/tourCatalog";
import type { TourProgressV1 } from "@/app/tours/tourTypes";
import { TourCenterDialog } from "./TourCenterDialog";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

describe("TourCenterDialog", () => {
  let root: Root | null = null;

  afterEach(() => {
    vi.useRealTimers();
    if (root) {
      act(() => root?.unmount());
      root = null;
    }
    document.body.replaceChildren();
  });

  it("shows new, updated, completed and skipped states", async () => {
    const progress: TourProgressV1 = {
      schemaVersion: 1,
      tours: {
        full: { revision: 2, state: "completed", updatedAt: 1 },
        basic: { revision: 1, state: "completed", updatedAt: 2 },
        dashboard: { revision: 1, state: "skipped", updatedAt: 3 },
      },
    };
    root = createRoot(document.createElement("div"));
    await act(async () => {
      root?.render(
        <TourCenterDialog
          open
          tours={PUBLISHED_TOURS}
          progress={progress}
          onClose={vi.fn()}
          onStart={vi.fn()}
          onReset={vi.fn()}
        />,
      );
    });
    expect(document.body.textContent).toContain("已完成");
    expect(document.body.textContent).toContain("未完成");
    expect(document.body.textContent).toContain("新增");
    expect(document.body.textContent).toContain("有更新");
  });

  it("forwards the selected tour and closes through the parent callback", async () => {
    const onStart = vi.fn();
    const container = document.createElement("div");
    document.body.append(container);
    root = createRoot(container);
    await act(async () => {
      root?.render(
        <TourCenterDialog
          open
          tours={PUBLISHED_TOURS}
          progress={{ schemaVersion: 1, tours: {} }}
          onClose={vi.fn()}
          onStart={onStart}
          onReset={vi.fn()}
        />,
      );
    });
    const startButton = Array.from(document.body.querySelectorAll("button")).find(
      (button) => button.textContent?.trim() === "开始",
    );
    expect(startButton).toBeDefined();
    await act(async () => startButton?.click());
    expect(onStart).toHaveBeenCalledWith("full");
  });

  it("labels a completed older revision as updated and replayable", async () => {
    const container = document.createElement("div");
    document.body.append(container);
    root = createRoot(container);
    await act(async () => {
      root?.render(
        <TourCenterDialog
          open
          tours={[{ ...PUBLISHED_TOURS.find((tour) => tour.id === "basic")!, revision: 2 }]}
          progress={{ schemaVersion: 1, tours: { basic: { revision: 1, state: "completed", updatedAt: 1 } } }}
          onClose={vi.fn()}
          onStart={vi.fn()}
          onReset={vi.fn()}
        />,
      );
    });
    expect(document.body.textContent).toContain("有更新");
    expect(document.body.textContent).toContain("重新查看");
  });

  it("keeps the complete experience independent from completed page tours", async () => {
    const completed = Object.fromEntries(
      PUBLISHED_TOURS.filter((tour) => tour.id !== "full").map((tour) => [
        tour.id,
        { revision: tour.revision, state: "completed" as const, updatedAt: 1 },
      ]),
    );
    const container = document.createElement("div");
    document.body.append(container);
    root = createRoot(container);
    await act(async () => {
      root?.render(
        <TourCenterDialog
          open
          tours={PUBLISHED_TOURS}
          progress={{ schemaVersion: 1, tours: completed }}
          onClose={vi.fn()}
          onStart={vi.fn()}
          onReset={vi.fn()}
        />,
      );
    });

    const fullRow = document.body.querySelector("[data-tour-center-id='full']");
    expect(fullRow?.textContent).toContain("新增");
    expect(fullRow?.textContent).not.toContain("已完成");
  });

  it("groups and orders tours while showing estimated duration", async () => {
    const container = document.createElement("div");
    document.body.append(container);
    root = createRoot(container);
    await act(async () => {
      root?.render(
        <TourCenterDialog
          open
          tours={[...PUBLISHED_TOURS].reverse()}
          progress={{ schemaVersion: 1, tours: {} }}
          onClose={vi.fn()}
          onStart={vi.fn()}
          onReset={vi.fn()}
        />,
      );
    });

    const headings = Array.from(document.body.querySelectorAll("h3")).map((node) => node.textContent);
    expect(headings).toEqual(["推荐", "页面教程"]);
    const rows = Array.from(document.body.querySelectorAll("[data-tour-center-id]")).map(
      (node) => node.getAttribute("data-tour-center-id"),
    );
    expect(rows).toEqual([
      "full", "basic", "dashboard", "stations", "key-pool", "routing",
      "pricing", "channels", "changes", "logs", "settings",
    ]);
    expect(document.body.textContent).toContain("约 5 分钟");
  });

  it("notifies the parent after the dialog has actually exited", async () => {
    vi.useFakeTimers();
    const onExited = vi.fn();
    function Harness() {
      const [open, setOpen] = useState(true);
      return (
        <TourCenterDialog
          open={open}
          tours={PUBLISHED_TOURS}
          progress={{ schemaVersion: 1, tours: {} }}
          onClose={() => setOpen(false)}
          onExited={onExited}
          onStart={() => setOpen(false)}
          onReset={vi.fn()}
        />
      );
    }

    const container = document.createElement("div");
    document.body.append(container);
    root = createRoot(container);
    await act(async () => root?.render(<Harness />));
    const startButton = Array.from(document.body.querySelectorAll("button")).find(
      (button) => button.textContent?.trim() === "开始",
    );
    await act(async () => startButton?.click());
    expect(onExited).not.toHaveBeenCalled();
    await act(async () => vi.advanceTimersByTime(200));
    expect(onExited).toHaveBeenCalledOnce();
    vi.useRealTimers();
  });
});
