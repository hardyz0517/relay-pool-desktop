// @vitest-environment jsdom

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, useState } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ToastProvider } from "@/components/ui";
import type { ChangeCenterView } from "./ChangeCenterPage";
import { ChangeCenterPage } from "./ChangeCenterPage";

vi.mock("@/lib/query/useActivityQuery", () => ({
  useActivityQuery: (options: { queryKey?: readonly unknown[] }) => {
    const queryName = options.queryKey?.[0];
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
    return {
      data: { items: [], nextCursor: null, activeCount: 0, unseenCount: 0 },
      error: null,
      isFetching: false,
      isPending: false,
    };
  },
}));

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function ViewStateHarness({ mounted }: { mounted: boolean }) {
  const [view, setView] = useState<ChangeCenterView>("active");
  return mounted ? (
    <ChangeCenterPage selectedView={view} onSelectedViewChange={setView} />
  ) : null;
}

describe("ChangeCenterPage view retention", () => {
  let host: HTMLDivElement;
  let root: Root;
  let queryClient: QueryClient;

  beforeEach(() => {
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
    const informationView = Array.from(host.querySelectorAll<HTMLButtonElement>('[role="radio"]'))
      .find((button) => button.textContent === "信息");
    expect(informationView).toBeDefined();

    act(() => informationView?.click());
    expect(informationView?.getAttribute("aria-checked")).toBe("true");

    render(false);
    render(true);

    const restoredInformationView = Array.from(host.querySelectorAll<HTMLButtonElement>('[role="radio"]'))
      .find((button) => button.textContent === "信息");
    expect(restoredInformationView?.getAttribute("aria-checked")).toBe("true");
  });
});
