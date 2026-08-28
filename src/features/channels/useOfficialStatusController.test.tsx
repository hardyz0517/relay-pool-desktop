// @vitest-environment jsdom

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ToastProvider } from "@/components/ui";
import type {
  StationPublishedStatusOverview,
  StationPublishedStatusOverviewInput,
} from "@/lib/types/stationPublishedStatus";
import { useOfficialStatusController } from "./useOfficialStatusController";

const mocks = vi.hoisted(() => ({
  getOverview: vi.fn(),
}));

vi.mock("@/lib/api/stationPublishedStatusOverview", () => ({
  getStationPublishedStatusOverview: mocks.getOverview,
}));

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let host: HTMLDivElement;
let root: Root;
let queryClient: QueryClient;
let controller: ReturnType<typeof useOfficialStatusController> | null;

function HookProbe() {
  controller = useOfficialStatusController();
  return null;
}

beforeEach(async () => {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  controller = null;
  mocks.getOverview.mockReset().mockImplementation((input: StationPublishedStatusOverviewInput) => {
    if (input.cursor === "cursor-page-3") return Promise.resolve(overview(null));
    if (input.cursor === "cursor-page-2") return Promise.resolve(overview("cursor-page-3"));
    return Promise.resolve(overview("cursor-page-2"));
  });

  await act(async () => {
    root.render(
      <QueryClientProvider client={queryClient}>
        <ToastProvider>
          <HookProbe />
        </ToastProvider>
      </QueryClientProvider>,
    );
  });
  await waitForPageCount(3);
});

afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  queryClient.clear();
});

describe("useOfficialStatusController pagination", () => {
  it("builds an opaque cursor stack for direct jumps and reuses known previous pages", async () => {
    await act(async () => {
      await controller!.changePage(3);
    });
    await waitForCurrentPage(3);

    expect(mocks.getOverview.mock.calls.some(([input]) => input.cursor === "cursor-page-2")).toBe(true);
    expect(mocks.getOverview.mock.calls.some(([input]) => input.cursor === "cursor-page-3")).toBe(true);

    const callsBeforeReturn = mocks.getOverview.mock.calls.length;
    await act(async () => {
      await controller!.changePage(1);
    });
    await waitForCurrentPage(1);

    expect(controller?.input.cursor).toBeNull();
    expect(mocks.getOverview.mock.calls.length).toBe(callsBeforeReturn);
  });
});

function overview(nextCursor: string | null): StationPublishedStatusOverview {
  return {
    readAtMs: 1,
    summary: { monitorTotal: 201 },
    rows: [],
    page: { limit: 100, returned: 0, nextCursor },
  };
}

async function waitForPageCount(totalPages: number) {
  for (let attempt = 0; attempt < 20; attempt += 1) {
    if (controller?.pageInfo.totalPages === totalPages) return;
    await act(async () => new Promise<void>((resolve) => setTimeout(resolve, 0)));
  }
  expect(controller?.pageInfo.totalPages).toBe(totalPages);
}

async function waitForCurrentPage(page: number) {
  for (let attempt = 0; attempt < 20; attempt += 1) {
    if (controller?.page === page && !controller.paginationBusy) return;
    await act(async () => new Promise<void>((resolve) => setTimeout(resolve, 0)));
  }
  expect(controller?.page).toBe(page);
}
