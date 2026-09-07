// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { describe, expect, it, vi } from "vitest";
import { PageSizeSelect, Pagination, buildPaginationItems } from "./Pagination";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

describe("Pagination", () => {
  it("builds classic page windows around the current page", () => {
    expect(buildPaginationItems(1, 33)).toEqual([1, 2, 3, "ellipsis", 33]);
    expect(buildPaginationItems(8, 33)).toEqual([1, "ellipsis", 6, 7, 8, 9, 10, "ellipsis", 33]);
    expect(buildPaginationItems(33, 33)).toEqual([1, "ellipsis", 31, 32, 33]);
    expect(buildPaginationItems(1, 412)).toEqual([1, 2, 3, "ellipsis", 412]);
    expect(buildPaginationItems(Number.NaN, Number.POSITIVE_INFINITY)).toEqual([1]);
  });

  it("exposes numbered navigation and disabled boundary arrows", async () => {
    const onPageChange = vi.fn();
    const host = document.createElement("div");
    const root = createRoot(host);

    await act(async () => root.render(
      <Pagination ariaLabel="使用记录分页" page={1} totalPages={33} onPageChange={onPageChange} />,
    ));

    const previous = host.querySelector<HTMLButtonElement>('button[aria-label="上一页"]')!;
    const next = host.querySelector<HTMLButtonElement>('button[aria-label="下一页"]')!;
    const pageThree = host.querySelector<HTMLButtonElement>('button[aria-label="第 3 页"]')!;

    expect(previous.disabled).toBe(true);
    expect(next.disabled).toBe(false);
    expect(host.querySelector('[aria-current="page"]')?.textContent).toBe("1");

    await act(async () => pageThree.click());
    await act(async () => next.click());

    expect(onPageChange).toHaveBeenNthCalledWith(1, 3);
    expect(onPageChange).toHaveBeenNthCalledWith(2, 2);

    await act(async () => root.unmount());
  });

  it("reuses SelectControl for compact page-size choices", async () => {
    const onChange = vi.fn();
    const host = document.createElement("div");
    document.body.append(host);
    const root = createRoot(host);

    await act(async () => {
      root.render(
        <PageSizeSelect
          ariaLabel="每页记录数"
          value={20}
          options={[20, 50, 100]}
          onChange={onChange}
        />,
      );
    });

    const trigger = host.querySelector<HTMLButtonElement>('button[aria-label="每页记录数"]')!;
    expect(host.querySelector("select")).toBeNull();
    expect(trigger.className).toContain("min-w-0");
    expect(trigger.className).toContain("shadow-none");

    await act(async () => trigger.click());
    const menu = document.querySelector<HTMLElement>('[role="listbox"]')!;
    const selectedOption = document.querySelector<HTMLButtonElement>('[role="option"][aria-selected="true"]')!;
    expect(menu.className).toContain("w-max");
    expect(menu.style.width).toBe("max-content");
    expect(selectedOption.className).toContain("w-full");
    expect(selectedOption.className).toContain("justify-start");
    expect(selectedOption.className).toContain("gap-1.5");
    expect(selectedOption.className).not.toContain("justify-between");
    const hovered = document.querySelectorAll<HTMLButtonElement>('[role="option"]')[1]!;
    expect(hovered.className).toContain("w-full");
    expect(Array.from(document.querySelectorAll('[role="listbox"] [role="option"]')).map((option) => option.textContent)).toEqual(["20", "50", "100"]);

    await act(async () => document.querySelectorAll<HTMLButtonElement>('[role="option"]')[1]?.click());
    expect(onChange).toHaveBeenCalledWith(50);

    await act(async () => root.unmount());
    host.remove();
  });
});
