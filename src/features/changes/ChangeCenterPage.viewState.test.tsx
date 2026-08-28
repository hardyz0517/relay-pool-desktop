// @vitest-environment jsdom

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, useState } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ToastProvider } from "@/components/ui";
import {
  buildChangeCenterPageInfo,
  CHANGE_CENTER_DEFAULT_VIEW,
  ChangeCenterPage,
  type ChangeCenterView,
} from "./ChangeCenterPage";

const queryCalls: Array<{ queryKey?: readonly unknown[]; enabled?: boolean }> = [];

vi.mock("@/lib/query/useActivityQuery", () => ({
  useActivityQuery: (options: { queryKey?: readonly unknown[]; enabled?: boolean }) => {
    queryCalls.push(options);
    const queryName = options.queryKey?.[0];
    const input = options.queryKey?.[1] as {
      limit?: number;
      recordType?: string | null;
      unreadOnly?: boolean;
    } | undefined;
    if (queryName === "stations") {
      return { data: [], error: null, isFetching: false, isPending: false };
    }
    if (queryName === "settings") {
      return {
        data: { developerModeEnabled: false },
        error: null,
        isFetching: false,
        isPending: false,
      };
    }
    if (
      queryName === "alertingActivity"
      && input?.recordType === "change"
      && input.unreadOnly === true
      && input.limit === 1
    ) {
      return {
        data: { items: [], nextCursor: null, activeCount: 0, unseenCount: 21, totalCount: 21 },
        error: null,
        isFetching: false,
        isPending: false,
      };
    }
    return {
      data: { items: [], nextCursor: null, activeCount: 0, unseenCount: 14, totalCount: 14 },
      error: null,
      isFetching: false,
      isPending: false,
    };
  },
}));

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function ViewStateHarness({ mounted }: { mounted: boolean }) {
  const [view, setView] = useState<ChangeCenterView>(CHANGE_CENTER_DEFAULT_VIEW);
  return mounted ? (
    <ChangeCenterPage selectedView={view} onSelectedViewChange={setView} />
  ) : null;
}

describe("ChangeCenterPage view retention", () => {
  let host: HTMLDivElement;
  let root: Root;
  let queryClient: QueryClient;

  beforeEach(() => {
    queryCalls.length = 0;
    host = document.createElement("div");
    document.body.appendChild(host);
    root = createRoot(host);
    queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  });

  afterEach(() => {
    act(() => root.unmount());
    queryClient.clear();
    host.remove();
  });

  function render(mounted: boolean) {
    act(() => {
      root.render(
        <QueryClientProvider client={queryClient}>
          <ToastProvider>
            <ViewStateHarness mounted={mounted} />
          </ToastProvider>
        </QueryClientProvider>,
      );
    });
  }

  it("restores the selected view after the page component remounts", () => {
    render(true);
    const allView = Array.from(host.querySelectorAll<HTMLButtonElement>('[role="radio"]'))
      .find((button) => button.textContent === "全部");
    expect(allView?.getAttribute("aria-checked")).toBe("true");

    const unreadView = Array.from(host.querySelectorAll<HTMLButtonElement>('[role="radio"]'))
      .find((button) => button.textContent === "未读");
    expect(unreadView).toBeDefined();

    act(() => unreadView?.click());
    expect(unreadView?.getAttribute("aria-checked")).toBe("true");

    render(false);
    render(true);

    const restoredUnreadView = Array.from(host.querySelectorAll<HTMLButtonElement>('[role="radio"]'))
      .find((button) => button.textContent === "未读");
    expect(restoredUnreadView?.getAttribute("aria-checked")).toBe("true");
  });

  it("loads active incidents from the current-incident query", () => {
    render(true);
    const activeView = Array.from(host.querySelectorAll<HTMLButtonElement>('[role="radio"]'))
      .find((button) => button.textContent === "活动");
    act(() => activeView?.click());

    expect(queryCalls).toContainEqual(expect.objectContaining({
      queryKey: ["alertingCurrent", expect.objectContaining({ lifecycleState: "active" })],
      enabled: true,
    }));
    expect(queryCalls).toContainEqual(expect.objectContaining({
      queryKey: ["alertingActivity", expect.anything()],
      enabled: false,
    }));
  });

  it("uses the shared unread-change count for the summary instead of the current view count", () => {
    render(true);

    expect(host.textContent).toContain("未读变更21");
    expect(queryCalls).toContainEqual(expect.objectContaining({
      queryKey: ["alertingActivity", { limit: 1, recordType: "change", unreadOnly: true }],
    }));
  });

  it("keeps the ordinary-mode tour anchors available in the empty state", () => {
    act(() => {
      root.render(
        <QueryClientProvider client={queryClient}>
          <ToastProvider>
            <ChangeCenterPage onOpenSettings={vi.fn()} />
          </ToastProvider>
        </QueryClientProvider>,
      );
    });

    for (const anchor of [
      "changes-view-filter",
      "changes-severity-filter",
      "changes-list",
      "changes-unread-actions",
      "changes-settings-entry",
    ]) {
      expect(host.querySelector(`[data-tour="${anchor}"]`), anchor).not.toBeNull();
    }
  });

  it("passes the search term to the paged alerting query", () => {
    render(true);
    const search = host.querySelector<HTMLInputElement>('[aria-label="搜索问题"]');
    expect(search).toBeDefined();
    act(() => {
      if (search) {
        Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set?.call(search, "ke");
        search.dispatchEvent(new Event("input", { bubbles: true }));
      }
    });

    expect(queryCalls).toContainEqual(expect.objectContaining({
      queryKey: ["alertingActivity", expect.objectContaining({ search: "ke" })],
    }));
  });

  it("keeps a requested page while its cursor query is loading", () => {
    expect(buildChangeCenterPageInfo({
      page: 7,
      pageSize: 20,
      totalCount: undefined,
      itemCount: 0,
    })).toMatchObject({ currentPage: 7, totalPages: 7 });
  });
});
