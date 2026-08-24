// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ToastProvider } from "@/components/ui";
import type { KeyPoolItem } from "@/lib/types/stationKeys";
import type { RoutingWorkspaceView } from "@/lib/types/routingWorkspace";
import { RoutingCandidateOrderPanel } from "./RoutingCandidateOrderPanel";

const mocks = vi.hoisted(() => ({
  dragEnd: null as ((event: unknown) => Promise<void>) | null,
  reorder: vi.fn(),
  synchronize: vi.fn(),
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
  useSortable: vi.fn(() => ({ attributes: {}, listeners: {}, setNodeRef: vi.fn(), transform: null, transition: undefined, isDragging: false })),
  verticalListSortingStrategy: {},
}));

vi.mock("@dnd-kit/utilities", () => ({ CSS: { Transform: { toString: vi.fn(() => undefined) } } }));
vi.mock("@/lib/api/stationKeys", () => ({ reorderKeyPool: mocks.reorder }));
vi.mock("@/lib/query/routingQuerySynchronization", () => ({ synchronizeRoutingQueriesAfterMutation: mocks.synchronize }));
vi.mock("./LocalRoutingStatusCandidateRow", () => ({
  LocalRoutingStatusCandidateHeader: () => <div data-testid="candidate-header" />,
  LocalRoutingStatusCandidateRow: ({ candidate }: { candidate: { stationKeyId: string } }) => <div data-candidate-id={candidate.stationKeyId} />,
}));

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

afterEach(() => {
  document.body.innerHTML = "";
  mocks.dragEnd = null;
  mocks.reorder.mockReset();
  mocks.synchronize.mockReset();
  vi.restoreAllMocks();
});

describe("RoutingCandidateOrderPanel", () => {
  it("renders the overview order, sorts by score, and persists drag changes", async () => {
    mocks.reorder.mockResolvedValue([]);
    mocks.synchronize.mockResolvedValue({ refreshed: true, errors: [] });
    const { host, root, queryClient } = renderPanel();

    expect(Array.from(host.querySelectorAll("[data-candidate-id]")).map((node) => node.getAttribute("data-candidate-id"))).toEqual(["key-2", "key-1", "key-3"]);
    await act(async () => {
      (host.querySelector('button[aria-label="按评分排序"]') as HTMLButtonElement)?.click();
    });
    expect(Array.from(host.querySelectorAll("[data-candidate-id]")).map((node) => node.getAttribute("data-candidate-id"))).toEqual(["key-1", "key-3", "key-2"]);
    await act(async () => {
      await mocks.dragEnd?.({ active: { id: "key-1" }, over: { id: "key-2" } });
    });
    expect(mocks.reorder).toHaveBeenCalledWith(["key-3", "key-2", "key-1"]);

    await act(async () => root.unmount());
    queryClient.clear();
  });
});

function renderPanel() {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const host = document.createElement("div");
  document.body.append(host);
  const root = createRoot(host);
  act(() => {
    root.render(
      <QueryClientProvider client={queryClient}>
        <ToastProvider>
          <RoutingCandidateOrderPanel workspace={workspace()} keyPoolItems={keyPoolItems()} loading={false} nowMs={0} heading="候选基础资格" />
        </ToastProvider>
      </QueryClientProvider>,
    );
  });
  return { host, root, queryClient };
}

function keyPoolItems(): KeyPoolItem[] {
  return [{ id: "key-2" }, { id: "key-1" }, { id: "key-3" }] as KeyPoolItem[];
}

function workspace(): RoutingWorkspaceView {
  return {
    settings: { routingGroupFilter: "all_groups" },
    candidates: ["key-1", "key-2", "key-3"].map((stationKeyId) => ({ stationKeyId, keyName: stationKeyId, stationId: "station-1", stationName: "Station", endpoint: "chat_completions", priority: 0, enabled: true, schedulable: true, healthState: "ready", score: { "key-1": 9_600, "key-3": 8_200 }[stationKeyId] ?? null, scoreDetails: null, currentConcurrency: null, lastSuccessAt: null, lastFailureAt: null, cooldownUntil: null, routingGroupScope: "all_groups", routingGroupMatch: true, previewEligible: true, previewRejectReasons: [], facts: [] })) as RoutingWorkspaceView["candidates"],
    summary: { candidateCount: 3, previewEligibleCandidateCount: 3, previewExcludedCandidateCount: 0, cooldownCandidateCount: 0, lastDecisionAt: null },
  } as RoutingWorkspaceView;
}
