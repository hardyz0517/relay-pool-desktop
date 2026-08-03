// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";
import { SelectControl } from "./SelectControl";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

afterEach(() => {
  document.body.innerHTML = "";
  vi.restoreAllMocks();
});

describe("SelectControl positioning", () => {
  it("keeps a wide menu inside a narrow viewport", async () => {
    vi.spyOn(window, "innerWidth", "get").mockReturnValue(360);
    const host = document.createElement("div");
    document.body.append(host);
    const root = createRoot(host);

    await act(async () => {
      root.render(
        <SelectControl
          ariaLabel="选择分组"
          menuMinWidth={420}
          value="group"
          options={[{ value: "group", label: "长分组名称" }]}
          onChange={vi.fn()}
        />,
      );
    });

    const trigger = document.querySelector<HTMLButtonElement>('button[aria-label="选择分组"]')!;
    vi.spyOn(trigger, "getBoundingClientRect").mockReturnValue({
      bottom: 100,
      height: 32,
      left: 20,
      right: 152,
      top: 68,
      width: 132,
      x: 20,
      y: 68,
      toJSON: () => ({}),
    });

    await act(async () => trigger.click());

    const menu = document.querySelector<HTMLElement>('[role="listbox"]')!;
    expect(menu.style.width).toBe("340px");

    await act(async () => root.unmount());
  });

  it("opens above when the menu does not fit below the trigger", async () => {
    vi.spyOn(window, "innerHeight", "get").mockReturnValue(640);
    const host = document.createElement("div");
    document.body.append(host);
    const root = createRoot(host);

    await act(async () => {
      root.render(
        <SelectControl
          ariaLabel="选择模型"
          value=""
          options={Array.from({ length: 8 }, (_, index) => ({
            value: `model-${index}`,
            label: `model-${index}`,
          }))}
          onChange={vi.fn()}
        />,
      );
    });

    const trigger = document.querySelector<HTMLButtonElement>('button[aria-label="选择模型"]')!;
    vi.spyOn(trigger, "getBoundingClientRect").mockReturnValue({
      bottom: 440,
      height: 32,
      left: 200,
      right: 332,
      top: 408,
      width: 132,
      x: 200,
      y: 408,
      toJSON: () => ({}),
    });

    await act(async () => trigger.click());

    const menu = document.querySelector<HTMLElement>('[role="listbox"]')!;
    expect(menu.style.top).toBe("");
    expect(menu.style.bottom).toBe("238px");
    expect(menu.style.maxHeight).toBe("320px");

    await act(async () => root.unmount());
  });
});
