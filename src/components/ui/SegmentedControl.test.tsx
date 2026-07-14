// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { Monitor, Moon, Sun } from "lucide-react";
import { describe, expect, it, vi } from "vitest";
import { SegmentedControl } from "./SegmentedControl";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

type Mode = "light" | "dark" | "system";

const options: Array<{ value: Mode; label: string; icon: typeof Sun }> = [
  { value: "light", label: "日间", icon: Sun },
  { value: "dark", label: "夜间", icon: Moon },
  { value: "system", label: "跟随系统", icon: Monitor },
];

describe("SegmentedControl icons", () => {
  it("uses the segmented active token instead of the switch thumb token", async () => {
    const host = document.createElement("div");
    const root = createRoot(host);

    await act(async () => root.render(
      <SegmentedControl ariaLabel="外观模式" options={options} value="dark" onChange={vi.fn()} />,
    ));

    const activeTrack = host.querySelector<HTMLElement>('[aria-hidden="true"]')!;
    const selectedOption = host.querySelector<HTMLElement>('[role="radio"][aria-checked="true"]')!;

    expect(activeTrack.className).toContain("bg-control-active");
    expect(activeTrack.className).not.toContain("bg-control-thumb");
    expect(selectedOption.className).toContain("text-control-active-foreground");

    await act(async () => root.unmount());
  });

  it("keeps labels accessible and layout tracks stable", async () => {
    const onChange = vi.fn();
    const host = document.createElement("div");
    const root = createRoot(host);

    await act(async () => root.render(
      <SegmentedControl ariaLabel="外观模式" options={options} value="light" onChange={onChange} />,
    ));

    const group = host.querySelector<HTMLElement>('[role="radiogroup"]')!;
    const columns = group.style.gridTemplateColumns;
    const radios = [...host.querySelectorAll<HTMLElement>('[role="radio"]')];

    const icons = [...host.querySelectorAll("svg")];
    expect(radios.map((radio) => radio.textContent)).toEqual(["日间", "夜间", "跟随系统"]);
    expect(icons).toHaveLength(3);
    expect(icons.every((icon) => icon.getAttribute("aria-hidden") === "true")).toBe(true);

    await act(async () => group.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true })));
    expect(onChange).toHaveBeenCalledWith("dark");

    await act(async () => root.render(
      <SegmentedControl ariaLabel="外观模式" options={options} value="dark" onChange={onChange} />,
    ));
    expect(host.querySelector<HTMLElement>('[role="radiogroup"]')!.style.gridTemplateColumns).toBe(columns);

    await act(async () => root.unmount());
  });
});
