// @vitest-environment jsdom

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { queryKeys } from "@/lib/query/queryKeys";
import type { StationPublishedStatusWorkspace } from "@/lib/types/stationPublishedStatus";
import { useStationPublishedStatus } from "./useStationPublishedStatus";

const mocks = vi.hoisted(() => ({
  collectStationTask: vi.fn(),
  getStationPublishedStatusWorkspace: vi.fn(),
}));

vi.mock("@/lib/api/collector", () => ({
  collectStationTask: mocks.collectStationTask,
}));

vi.mock("@/lib/api/stationPublishedStatus", () => ({
  getStationPublishedStatusWorkspace: mocks.getStationPublishedStatusWorkspace,
}));

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let host: HTMLDivElement;
let root: Root;
let queryClient: QueryClient;
let controller: ReturnType<typeof useStationPublishedStatus> | null;

function HookProbe({ stationId }: { stationId: string }) {
  controller = useStationPublishedStatus(stationId);
  return null;
}

async function renderHook(stationId: string) {
  await act(async () => {
    root.render(
      <QueryClientProvider client={queryClient}>
        <HookProbe stationId={stationId} />
      </QueryClientProvider>,
    );
  });
}

beforeEach(async () => {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  queryClient = new QueryClient({
    defaultOptions: { mutations: { retry: false }, queries: { retry: false } },
  });
  controller = null;
  mocks.collectStationTask.mockReset().mockResolvedValue(undefined);
  mocks.getStationPublishedStatusWorkspace.mockReset().mockResolvedValue(workspace("station-1"));

  await renderHook("station-1");
  await waitForWorkspaceValue("station-1");
});

afterEach(async () => {
  await act(async () => {
    root.unmount();
  });
  host.remove();
  queryClient.clear();
});

describe("useStationPublishedStatus", () => {
  it("refreshes with the closed published-status collector task and invalidates only related keys", async () => {
    const invalidateQueries = vi.spyOn(queryClient, "invalidateQueries");

    await act(async () => {
      await controller!.refresh();
    });

    expect(mocks.collectStationTask).toHaveBeenCalledWith("station-1", "published_status");
    expect(invalidateQueries).toHaveBeenCalledWith({
      queryKey: queryKeys.stationPublishedStatus("station-1"),
    });
    expect(invalidateQueries).toHaveBeenCalledWith({
      queryKey: queryKeys.collectorRuns("station-1"),
    });
    expect(invalidateQueries).toHaveBeenCalledWith({
      queryKey: queryKeys.collectorSnapshots("station-1"),
    });
  });

  it("retries a workspace read without triggering another collection", async () => {
    const initialReadCount = mocks.getStationPublishedStatusWorkspace.mock.calls.length;
    await act(async () => {
      await controller!.retryWorkspace();
    });

    expect(mocks.getStationPublishedStatusWorkspace.mock.calls.length).toBeGreaterThan(initialReadCount);
    expect(mocks.collectStationTask).not.toHaveBeenCalled();
  });

  it("keeps late station A data isolated from the current station B hook", async () => {
    const stationA = deferred<StationPublishedStatusWorkspace>();
    const stationB = deferred<StationPublishedStatusWorkspace>();
    mocks.getStationPublishedStatusWorkspace.mockImplementation((stationId: string) =>
      stationId === "station-a" ? stationA.promise : stationB.promise,
    );

    await renderHook("station-a");
    await renderHook("station-b");
    await waitForWorkspaceRead("station-a");
    await waitForWorkspaceRead("station-b");
    await act(async () => {
      stationB.resolve(workspace("station-b"));
      await stationB.promise;
    });
    await waitForWorkspaceValue("station-b");
    expect(controller?.workspace?.stationId).toBe("station-b");

    await act(async () => {
      stationA.resolve(workspace("station-a"));
      await stationA.promise;
    });
    await waitForWorkspaceValue("station-b");

    expect(controller?.workspace?.stationId).toBe("station-b");
    expect(queryClient.getQueryData(queryKeys.stationPublishedStatus("station-a"))).toEqual(workspace("station-a"));
    expect(queryClient.getQueryData(queryKeys.stationPublishedStatus("station-b"))).toEqual(workspace("station-b"));
  });
});

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((resolvePromise) => {
    resolve = resolvePromise;
  });
  return { promise, resolve };
}

function workspace(stationId: string): StationPublishedStatusWorkspace {
  return {
    stationId,
    endpointRevision: 1,
    supported: true,
    sourceState: "available",
    completeness: "complete",
    lastAttemptAtMs: null,
    lastSuccessAtMs: null,
    lastCompleteAtMs: null,
    monitorCount: 0,
    stale: false,
    safeErrorKind: null,
    rows: [],
  };
}

async function waitForWorkspaceRead(stationId: string) {
  for (let attempt = 0; attempt < 10; attempt += 1) {
    if (mocks.getStationPublishedStatusWorkspace.mock.calls.some(([id]) => id === stationId)) {
      return;
    }
    await act(async () => {
      await new Promise<void>((resolve) => setTimeout(resolve, 0));
    });
  }
  expect(mocks.getStationPublishedStatusWorkspace.mock.calls.map(([id]) => id)).toContain(stationId);
}

async function waitForWorkspaceValue(stationId: string) {
  for (let attempt = 0; attempt < 10; attempt += 1) {
    if (controller?.workspace?.stationId === stationId) {
      return;
    }
    await act(async () => {
      await new Promise<void>((resolve) => setTimeout(resolve, 0));
    });
  }
  expect(controller?.workspace?.stationId).toBe(stationId);
}
