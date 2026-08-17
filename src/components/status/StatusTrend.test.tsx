// @vitest-environment jsdom

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { StatusTrend, type StatusTrendCell } from "./StatusTrend";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let host: HTMLDivElement;
let root: Root;

beforeEach(() => {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});

afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
});

describe("StatusTrend", () => {
  function fixtureCell(index: number, latencyMs: number): StatusTrendCell {
    return {
      id: `sample-${index}`,
      tone: "available",
      label: `来源：站点发布\\n模型：fixture-model\\n检查时间：08-16 10:00\\n状态：正常\\n延迟：${latencyMs} ms\\nPing：7 ms`,
      modelLabel: "fixture-model",
      timeLabel: "官方检查：08-16 10:00",
      availabilityLabel: "状态：正常",
      latencyLabel: `${latencyMs} ms`,
      metricLabel: `延迟：${latencyMs} ms · Ping：7 ms`,
    };
  }

  it("keeps 60 slots and places short official histories after leading empty slots", async () => {
    await act(async () => {
      root.render(
        <StatusTrend
          slotCount={60}
          cells={[fixtureCell(1, 42)]}
        />,
      );
    });

    const slots = host.querySelectorAll("span");
    expect(slots).toHaveLength(60);
    expect(slots[0]?.getAttribute("title")).toBeNull();
    expect(slots[0]?.getAttribute("aria-label")).toBe("无记录");
    expect(slots[59]?.getAttribute("title")).toBeNull();
    expect(slots[59]?.getAttribute("aria-label")).toContain("来源：站点发布");
    expect(slots[59]?.getAttribute("aria-label")).toContain("检查时间：");
    expect(slots[59]?.getAttribute("aria-label")).toContain("延迟：42 ms");
    expect(slots[59]?.getAttribute("aria-label")).toContain("Ping：7 ms");
  });

  it("truncates long histories to the latest fixed slots", async () => {
    const cells = Array.from({ length: 61 }, (_, index) => fixtureCell(index, index));

    await act(async () => {
      root.render(<StatusTrend slotCount={60} cells={cells} />);
    });

    const slots = host.querySelectorAll("span");
    expect(slots).toHaveLength(60);
    expect(slots[0]?.getAttribute("aria-label")).toContain("延迟：1 ms");
    expect(slots[59]?.getAttribute("aria-label")).toContain("延迟：60 ms");
  });

  it("reveals a populated slot tooltip through keyboard focus", async () => {
    await act(async () => {
      root.render(<StatusTrend slotCount={2} cells={[fixtureCell(1, 42)]} />);
    });

    const populatedSlot = host.querySelector("span[aria-label*='来源：站点发布']") as HTMLSpanElement;
    expect(populatedSlot.tabIndex).toBe(0);
    await act(async () => populatedSlot.focus());
    const tooltip = host.querySelector("[role='tooltip']");
    expect(tooltip?.textContent).toContain("fixture-model");
    expect(tooltip?.textContent).toContain("延迟：42 ms");
  });
});
