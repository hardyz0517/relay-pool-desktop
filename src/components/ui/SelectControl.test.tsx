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
  it("filters searchable options and reports an empty state", async () => {
    const onChange = vi.fn();
    const host = document.createElement("div");
    document.body.append(host);
    const root = createRoot(host);

    await act(async () => {
      root.render(
        <SelectControl
          ariaLabel="选择模型"
          searchable
          value=""
          options={[{ value: "gpt-4o-mini", label: "gpt-4o-mini" }, { value: "claude-sonnet", label: "claude-sonnet" }]}
          onChange={onChange}
        />,
      );
    });

    await act(async () => document.querySelector<HTMLButtonElement>('button[aria-label="选择模型"]')?.click());
    const search = document.querySelector<HTMLInputElement>('input[aria-label="选择模型 搜索"]')!;
    expect(search.className).toContain("pl-8");
    expect(search.className).toContain("focus:ring-0");
    expect(search.parentElement?.querySelector('svg[aria-hidden="true"]')).toBeTruthy();
    expect(document.querySelectorAll('[role="listbox"] [role="option"]')).toHaveLength(2);
    await act(async () => {
      const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set;
      setter?.call(search, "claude");
      search.dispatchEvent(new Event("input", { bubbles: true }));
      search.dispatchEvent(new Event("change", { bubbles: true }));
    });
    expect(Array.from(document.querySelectorAll('[role="listbox"] [role="option"]')).map((option) => option.textContent)).toEqual(["claude-sonnet"]);

    await act(async () => {
      const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set;
      setter?.call(search, "missing");
      search.dispatchEvent(new Event("input", { bubbles: true }));
      search.dispatchEvent(new Event("change", { bubbles: true }));
    });
    expect(document.querySelector('[role="listbox"] [role="status"]')?.textContent).toBe("无匹配项");

    await act(async () => root.unmount());
  });

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
