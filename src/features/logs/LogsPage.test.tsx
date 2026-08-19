// @vitest-environment jsdom

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ToastProvider } from "@/components/ui";
import type { RequestLog } from "@/lib/types/proxy";
import { LogsPage } from "./LogsPage";

const mockState = vi.hoisted(() => ({ logs: [] as RequestLog[] }));

vi.mock("@/lib/query/useActivityQuery", () => ({
  useActivityQuery: (options: { queryKey?: readonly unknown[] }) => {
    switch (options.queryKey?.[0]) {
      case "requestLogs":
        return { data: mockState.logs, error: null, isFetching: false, isPending: false };
      case "proxyStatus":
        return { data: { running: false }, error: null, isFetching: false, isPending: false };
      case "settings":
        return { data: { developerModeEnabled: false }, error: null, isFetching: false, isPending: false };
      case "keyPool":
      case "stations":
        return { data: [], error: null, isFetching: false, isPending: false };
      default:
        return { data: undefined, error: null, isFetching: false, isPending: false };
    }
  },
}));

vi.mock("./RequestLogTable", () => ({
  RequestLogTable: () => <div data-testid="request-log-table" />,
  RequestLogPagination: ({ pageInfo, onPageChange }: { pageInfo: { page: number }; onPageChange: (page: number) => void }) => (
    <button type="button" data-testid="request-log-page-two" onClick={() => onPageChange(2)}>
      page {pageInfo.page}
    </button>
  ),
}));

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function createLog(id: string): RequestLog {
  return { id, startedAt: "2026-08-18T00:00:00.000Z" } as RequestLog;
}

describe("LogsPage pagination", () => {
  let host: HTMLDivElement;
  let root: Root;
  let queryClient: QueryClient;

  beforeEach(() => {
    mockState.logs = Array.from({ length: 40 }, (_, index) => createLog(`log-${index + 1}`));
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

  it("does not reapply an already handled deep link after logs refresh", () => {
    const render = () => {
      root.render(
        <QueryClientProvider client={queryClient}>
          <ToastProvider>
            <LogsPage deepLink={{ kind: "request-log", requestLogId: "log-1", sequence: 1 }} />
          </ToastProvider>
        </QueryClientProvider>,
      );
    };

    act(render);
    const pageButton = host.querySelector<HTMLButtonElement>('[data-testid="request-log-page-two"]')!;
    act(() => pageButton.click());
    expect(pageButton.textContent).toBe("page 2");

    mockState.logs = [...mockState.logs];
    act(render);

    expect(host.querySelector('[data-testid="request-log-page-two"]')?.textContent).toBe("page 2");
  });
});
