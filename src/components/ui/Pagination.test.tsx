// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { describe, expect, it, vi } from "vitest";
import { Pagination, buildPaginationItems } from "./Pagination";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

describe("Pagination", () => {
  it("builds classic page windows around the current page", () => {
    expect(buildPaginationItems(1, 33)).toEqual([1, 2, 3, "ellipsis", 33]);
    expect(buildPaginationItems(8, 33)).toEqual([1, "ellipsis", 6, 7, 8, 9, 10, "ellipsis", 33]);
    expect(buildPaginationItems(33, 33)).toEqual([1, "ellipsis", 31, 32, 33]);
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
});
