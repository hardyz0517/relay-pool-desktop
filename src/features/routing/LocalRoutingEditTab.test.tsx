// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ToastProvider } from "@/components/ui";
import type { RoutingCandidateView, RoutingWorkspaceView } from "@/lib/types/routingWorkspace";
import { LocalRoutingEditTab } from "./LocalRoutingEditTab";

const mocks = vi.hoisted(() => ({
  dragEnd: null as ((event: unknown) => Promise<void>) | null,
  reorderKeyPool: vi.fn(),
}));

vi.mock("@dnd-kit/core", () => ({
  closestCenter: vi.fn(),
  DndContext: ({ children, onDragEnd }: { children: unknown; onDragEnd: (event: unknown) => Promise<void> }) => {
    mocks.dragEnd = onDragEnd;
    return children;
  },
  KeyboardSensor: vi.fn(),
  PointerSensor: vi.fn(),
  useSensor: vi.fn(() => ({})),
  useSensors: vi.fn(() => []),
}));

vi.mock("@dnd-kit/sortable", () => ({
  sortableKeyboardCoordinates: vi.fn(),
  SortableContext: ({ children }: { children: unknown }) => children,
  useSortable: vi.fn(() => ({
    attributes: {},
    listeners: {},
    setNodeRef: vi.fn(),
    transform: null,
    transition: undefined,
    isDragging: false,
  })),
  verticalListSortingStrategy: {},
}));

vi.mock("@dnd-kit/utilities", () => ({
  CSS: { Transform: { toString: vi.fn(() => undefined) } },
}));

vi.mock("@/lib/api/stationKeys", () => ({
  reorderKeyPool: mocks.reorderKeyPool,
}));

vi.mock("./LocalRoutingSettingsEditor", () => ({
  LocalRoutingSettingsEditor: () => null,
}));

vi.mock("./LocalRoutingCandidateRow", () => ({
  LocalRoutingCandidateHeader: () => null,
  LocalRoutingCandidateRow: ({
    candidate,
    order,
  }: {
    candidate: RoutingCandidateView;
    order: number;
  }) => <div data-candidate-id={candidate.stationKeyId}>{order}</div>,
}));

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

afterEach(() => {
  document.body.innerHTML = "";
  mocks.dragEnd = null;
  mocks.reorderKeyPool.mockReset();
  vi.restoreAllMocks();
});

describe("LocalRoutingEditTab", () => {
  it("publishes a saved reorder and invalidates both routing read models", async () => {
    const initial = workspace([candidate("key-1"), candidate("key-2")]);
    mocks.reorderKeyPool.mockResolvedValue([]);
    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    const invalidateQueries = vi.spyOn(queryClient, "invalidateQueries");
    const host = document.createElement("div");
    document.body.append(host);
    const root = createRoot(host);

    await act(async () => {
      root.render(
        <QueryClientProvider client={queryClient}>
          <ToastProvider>
            <LocalRoutingEditTab loading={false} workspace={initial} />
          </ToastProvider>
        </QueryClientProvider>,
      );
    });

    const candidateSection = document.querySelector<HTMLElement>(
      'section[aria-labelledby="local-routing-edit-candidates-title"]',
    );
    expect(candidateSection).not.toBeNull();
    expect(candidateSection?.querySelector(".shadow-surface")).toBeNull();
    expect(candidateSection?.querySelector(".p-4")).toBeNull();

    await act(async () => {
      await mocks.dragEnd?.({ active: { id: "key-2" }, over: { id: "key-1" } });
    });

    expect(mocks.reorderKeyPool).toHaveBeenCalledWith(["key-2", "key-1"]);
    expect(invalidateQueries).toHaveBeenCalledWith({ queryKey: ["routing"] });

    await act(async () => root.unmount());
    queryClient.clear();
  });
});

function candidate(stationKeyId: string): RoutingCandidateView {
  return {
    stationKeyId,
    stationId: "station-1",
    stationName: "Station",
    keyName: stationKeyId,
    endpoint: "chat_completions",
    priority: 0,
    enabled: true,
    schedulable: true,
    healthState: "ready",
    lastSuccessAt: null,
    lastFailureAt: null,
    cooldownUntil: null,
    routingGroupScope: "all_groups",
    routingGroupMatch: true,
    previewEligible: true,
    previewRejectReasons: [],
    facts: [],
  };
}

function workspace(candidates: RoutingCandidateView[]): RoutingWorkspaceView {
  return {
    proxyStatus: {
      running: false,
      lifecycle: "stopped",
      bindAddr: "127.0.0.1",
      port: 1431,
      startedAt: null,
      lastError: null,
      activeRequests: 0,
      requestCount: 0,
    },
    settings: {
      enabled: true,
      bindAddr: "127.0.0.1",
      port: 1431,
      endpoint: "chat_completions",
      policy: {
        version: 1,
        reliabilityWeight: 4000,
        responsivenessWeight: 2500,
        costWeight: 2000,
        preferenceWeight: 1500,
        maxCandidates: 64,
        explorationShareBasisPoints: 500,
        allowDepletedFallback: false,
        affinityEnabled: false,
        affinityTtlSeconds: 300,
      },
      maxRateMultiplier: null,
      routingGroupFilter: "all_groups",
      fallbackEnabled: true,
      previewKind: "baseline_eligibility",
    },
    summary: {
      candidateCount: candidates.length,
      previewEligibleCandidateCount: candidates.length,
      previewExcludedCandidateCount: 0,
      cooldownCandidateCount: 0,
      lastDecisionAt: null,
    },
    candidates,
    latestDecision: null,
  };
}
