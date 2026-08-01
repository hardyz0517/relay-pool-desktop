// @vitest-environment jsdom

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ChannelStatusRowView } from "./channelStatusViewModel";
import {
  type ChannelStatusController,
  useChannelStatusController,
} from "./useChannelStatusController";

const mocks = vi.hoisted(() => ({
  runChannelMonitorNowWithTrigger: vi.fn(),
}));

vi.mock("@/lib/api/channelMonitors", () => ({
  cancelChannelMonitorExecution: vi.fn(),
  runChannelMonitorNowWithTrigger: mocks.runChannelMonitorNowWithTrigger,
}));

vi.mock("@/lib/query/useActivityQuery", () => ({
  useActivityQuery: vi.fn(() => ({
    data: undefined,
    error: null,
    isPending: false,
    refetch: vi.fn(),
  })),
}));

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let host: HTMLDivElement;
let root: Root;
let queryClient: QueryClient;
let controller: ChannelStatusController | null;

function ControllerProbe() {
  controller = useChannelStatusController();
  return null;
}

beforeEach(async () => {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  queryClient = new QueryClient({
    defaultOptions: { mutations: { retry: false }, queries: { retry: false } },
  });
  controller = null;
  mocks.runChannelMonitorNowWithTrigger.mockReset().mockResolvedValue({
    executionId: "execution-1",
    monitorId: "monitor-1",
    status: "queued",
  });

  await act(async () => {
    root.render(
      <QueryClientProvider client={queryClient}>
        <ControllerProbe />
      </QueryClientProvider>,
    );
  });
});

afterEach(async () => {
  await act(async () => {
    root.unmount();
  });
  host.remove();
  queryClient.clear();
});

describe("useChannelStatusController", () => {
  it("does not open execution details after starting a manual test", async () => {
    const row = {
      monitorId: "monitor-1",
      runningExecutionId: null,
    } as ChannelStatusRowView;

    await act(async () => {
      controller!.runNow(row);
      await vi.waitFor(() => {
        expect(mocks.runChannelMonitorNowWithTrigger).toHaveBeenCalledTimes(1);
      });
    });

    expect(controller!.selectedExecutionId).toBeNull();
  });
});
