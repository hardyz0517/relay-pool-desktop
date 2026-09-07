import { useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import {
  closestCenter,
  DndContext,
  KeyboardSensor,
  PointerSensor,
  type DragEndEvent,
  useSensor,
  useSensors,
} from "@dnd-kit/core";
import {
  SortableContext,
  sortableKeyboardCoordinates,
  useSortable,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { useQueryClient } from "@tanstack/react-query";
import { ArrowDownUp } from "lucide-react";
import { Button, EmptyState, StatusBadge, useToast } from "@/components/ui";
import { reorderKeyPool } from "@/lib/api/stationKeys";
import { readError } from "@/lib/errors";
import { queryKeys } from "@/lib/query/queryKeys";
import { synchronizeRoutingQueriesAfterMutation } from "@/lib/query/routingQuerySynchronization";
import type { KeyPoolItem } from "@/lib/types/stationKeys";
import type { RoutingCandidateView, RoutingWorkspaceView } from "@/lib/types/routingWorkspace";
import { cn } from "@/lib/utils";
import {
  LocalRoutingStatusCandidateHeader,
  LocalRoutingStatusCandidateRow,
} from "./LocalRoutingStatusCandidateRow";
import { buildEditableRoutingCandidates } from "./editableRoutingCandidates";
import { getRoutingEffectiveScore } from "./routingScore";

type ReorderSyncState = "idle" | "saving" | "synced" | "failed";

const syncLabels: Record<ReorderSyncState, string | null> = {
  idle: null,
  saving: "保存中",
  synced: "已同步",
  failed: "保存失败",
};

const syncTones: Record<Exclude<ReorderSyncState, "idle">, "healthy" | "warning" | "error"> = {
  saving: "warning",
  synced: "healthy",
  failed: "error",
};

type RoutingCandidateOrderPanelProps = {
  workspace: RoutingWorkspaceView | null;
  keyPoolItems: readonly KeyPoolItem[] | undefined;
  loading: boolean;
  nowMs: number;
  heading: string;
};

export function RoutingCandidateOrderPanel({
  workspace,
  keyPoolItems,
  loading,
  nowMs,
  heading,
}: RoutingCandidateOrderPanelProps) {
  const toast = useToast();
  const queryClient = useQueryClient();
  const [candidateIds, setCandidateIds] = useState<string[]>([]);
  const [syncState, setSyncState] = useState<ReorderSyncState>("idle");
  const [syncError, setSyncError] = useState<string | null>(null);
  const saveOperationRef = useRef(0);
  const keyPoolVersionRef = useRef(0);
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 6 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  );
  const candidateById = useMemo(() => {
    if (!workspace) return new Map<string, RoutingCandidateView>();
    return new Map(
      buildEditableRoutingCandidates(
        keyPoolItems ?? [],
        workspace.candidates,
        workspace.settings.routingGroupFilter,
      ).map((candidate) => [candidate.stationKeyId, candidate]),
    );
  }, [keyPoolItems, workspace]);
  const candidates = useMemo(
    () => candidateIds.flatMap((candidateId) => {
      const candidate = candidateById.get(candidateId);
      return candidate && candidate.enabled !== false ? [candidate] : [];
    }),
    [candidateById, candidateIds],
  );
  const attemptOrderById = useMemo(
    () => buildRoutingAttemptOrder([...candidateById.values()]),
    [candidateById],
  );
  const syncLabel = syncLabels[syncState];

  function handleSortByScore() {
    const visibleIds = candidates.map((candidate) => candidate.stationKeyId);
    const sortedVisibleIds = sortCandidateIdsByScore(visibleIds, candidateById);
    setCandidateIds((currentIds) => mergeVisibleCandidateOrder(currentIds, candidateById, sortedVisibleIds));
  }

  useEffect(() => {
    keyPoolVersionRef.current += 1;
    saveOperationRef.current += 1;
    if (!keyPoolItems) {
      setCandidateIds([]);
      setSyncState("idle");
      setSyncError(null);
      return;
    }
    setCandidateIds(orderedCandidateIds(keyPoolItems));
    setSyncState("idle");
    setSyncError(null);
  }, [keyPoolItems]);

  async function handleDragEnd(event: DragEndEvent) {
    if (syncState === "saving") return;
    const { active, over } = event;
    if (!over || active.id === over.id) return;
    const visibleCandidateIds = candidates.map((candidate) => candidate.stationKeyId);
    const activeIndex = visibleCandidateIds.indexOf(String(active.id));
    const overIndex = visibleCandidateIds.indexOf(String(over.id));
    if (activeIndex === -1 || overIndex === -1) return;

    const previousCandidateIds = candidateIds;
    const nextVisibleCandidateIds = [...visibleCandidateIds];
    const [moved] = nextVisibleCandidateIds.splice(activeIndex, 1);
    nextVisibleCandidateIds.splice(overIndex, 0, moved);
    const nextStationKeyIds = mergeVisibleCandidateOrder(candidateIds, candidateById, nextVisibleCandidateIds);
    const operationId = saveOperationRef.current + 1;
    const keyPoolVersionAtStart = keyPoolVersionRef.current;
    saveOperationRef.current = operationId;
    setCandidateIds(nextStationKeyIds);
    setSyncState("saving");
    setSyncError(null);

    let saved: Awaited<ReturnType<typeof reorderKeyPool>>;
    try {
      // A page activation can leave an older key-pool request in flight. Do not
      // let that response overwrite the order being persisted by this drag.
      await queryClient.cancelQueries({ queryKey: queryKeys.keyPool });
      saved = await reorderKeyPool(nextStationKeyIds);
      queryClient.setQueryData(queryKeys.keyPool, saved);
    } catch (requestError) {
      if (operationId !== saveOperationRef.current || keyPoolVersionAtStart !== keyPoolVersionRef.current) return;
      setCandidateIds(previousCandidateIds);
      setSyncState("failed");
      const message = readError(requestError);
      setSyncError(message);
      toast.error("保存候选顺序失败", message);
      return;
    }

    const operationIsCurrent = operationId === saveOperationRef.current && keyPoolVersionAtStart === keyPoolVersionRef.current;
    if (operationIsCurrent) {
      setCandidateIds(nextStationKeyIds);
      setSyncState("synced");
    }

    let synchronization: Awaited<ReturnType<typeof synchronizeRoutingQueriesAfterMutation>>;
    try {
      [synchronization] = await Promise.all([
        synchronizeRoutingQueriesAfterMutation(queryClient),
        queryClient.invalidateQueries({ queryKey: queryKeys.keyPool }),
      ]);
    } catch (refreshError) {
      // The mutation already succeeded; keep its response even if a read-model
      // refresh fails and report the refresh problem separately.
      queryClient.setQueryData(queryKeys.keyPool, saved);
      if (operationIsCurrent) {
        toast.error("候选顺序已保存，但状态刷新失败", readError(refreshError));
      }
      return;
    }

    // The mutation response is authoritative. Re-apply it after invalidation
    // so a stale refresh response cannot become the final cached order.
    queryClient.setQueryData(queryKeys.keyPool, saved);

    if (!synchronization.refreshed && operationIsCurrent) {
      toast.error("候选顺序已保存，但状态刷新失败", readError(synchronization.errors[0]));
    }
  }

  return (
    <section className="grid gap-2" aria-labelledby="local-routing-candidates-title">
      <header className="flex min-h-8 flex-wrap items-center justify-between gap-3">
        <h2 id="local-routing-candidates-title" className="text-sm font-semibold text-foreground">{heading}</h2>
        <div className="flex items-center gap-2">
          {syncLabel && syncState !== "idle" ? <StatusBadge tone={syncTones[syncState]}>{syncLabel}</StatusBadge> : null}
          <Button type="button" variant="secondary" size="sm" aria-label="按评分排序" title="按评分排序" onClick={handleSortByScore}>
            <ArrowDownUp className="size-4" />
            按评分排序
          </Button>
        </div>
      </header>
      {syncError ? <div className="rounded-[var(--surface-radius)] border border-danger-border bg-danger-surface px-3 py-2 text-xs text-danger-foreground">{syncError}</div> : null}
      {loading && !workspace ? (
        <div className="text-sm text-muted-foreground">正在加载候选密钥...</div>
      ) : workspace && !keyPoolItems ? (
        <div className="text-sm text-muted-foreground">密钥池加载中...</div>
      ) : candidates.length === 0 ? (
        <EmptyState title="暂无候选密钥" description="当前配置下没有可预览的路由密钥。" />
      ) : (
        <DndContext sensors={sensors} collisionDetection={closestCenter} onDragEnd={handleDragEnd}>
          <SortableContext items={candidates.map((candidate) => candidate.stationKeyId)} strategy={verticalListSortingStrategy}>
            <div className="overflow-hidden rounded-[var(--surface-radius)] border border-border bg-surface">
              <LocalRoutingStatusCandidateHeader sortable />
              <div className="divide-y divide-border">
                {candidates.map((candidate) => (
                  <SortableStatusCandidateRow
                    key={candidate.stationKeyId}
                    candidate={candidate}
                    attemptOrder={attemptOrderById.get(candidate.stationKeyId) ?? null}
                    nowMs={nowMs}
                    disabled={syncState === "saving"}
                  />
                ))}
              </div>
            </div>
          </SortableContext>
        </DndContext>
      )}
    </section>
  );
}

function orderedCandidateIds(keyPoolItems: readonly KeyPoolItem[]) {
  return keyPoolItems.map((item) => item.id);
}

export function sortCandidateIdsByScore(
  candidateIds: readonly string[],
  candidateById: ReadonlyMap<string, RoutingCandidateView>,
) {
  return [...candidateIds].sort((leftId, rightId) => {
    const left = candidateById.get(leftId);
    const right = candidateById.get(rightId);
    if (!left || !right) return left ? -1 : right ? 1 : 0;
    return compareRoutingAttemptCandidates(left, right);
  });
}

function mergeVisibleCandidateOrder(
  allCandidateIds: readonly string[],
  candidateById: ReadonlyMap<string, RoutingCandidateView>,
  visibleCandidateIds: readonly string[],
) {
  let visibleIndex = 0;
  return allCandidateIds.map((candidateId) => {
    const candidate = candidateById.get(candidateId);
    if (candidate && candidate.enabled !== false) {
      return visibleCandidateIds[visibleIndex++] ?? candidateId;
    }
    return candidateId;
  });
}

/**
 * Projects the deterministic order used by the routing planner onto the
 * editable queue. The queue itself remains in persisted key-pool order so it
 * can still be dragged; the ordinal shown in each row must not be derived
 * from that display order.
 *
 * Candidates that cannot participate in the current request do not receive
 * an ordinal because routing will never attempt them. Eligible and
 * conditionally eligible candidates are ordered by the planner's effective
 * score (descending), with a stable station-key id tie-break. A missing score
 * is kept after finite scores and then ordered by the same stable id fallback.
 */
export function buildRoutingAttemptOrder(
  candidates: readonly RoutingCandidateView[],
): ReadonlyMap<string, number> {
  const routable = candidates
    .filter((candidate) => (
      candidate.enabled !== false
      && candidate.scoreStatus === "scored"
      && (candidate.participationStatus === "eligible"
        || candidate.participationStatus === "conditionally_eligible")
    ))
    .sort(compareRoutingAttemptCandidates);

  return new Map(routable.map((candidate, index) => [candidate.stationKeyId, index + 1]));
}

function compareRoutingAttemptCandidates(
  left: RoutingCandidateView,
  right: RoutingCandidateView,
) {
  const leftScore = getRoutingEffectiveScore(left);
  const rightScore = getRoutingEffectiveScore(right);
  if (leftScore != null && rightScore != null && leftScore !== rightScore) {
    return rightScore - leftScore;
  }
  if (leftScore != null && rightScore == null) return -1;
  if (leftScore == null && rightScore != null) return 1;
  if (left.stationKeyId < right.stationKeyId) return -1;
  if (left.stationKeyId > right.stationKeyId) return 1;
  return 0;
}

function SortableStatusCandidateRow({ candidate, attemptOrder, nowMs, disabled }: { candidate: RoutingCandidateView; attemptOrder: number | null; nowMs: number; disabled: boolean }) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({ id: candidate.stationKeyId, disabled });
  const style: CSSProperties = { transform: CSS.Transform.toString(transform), transition };
  return (
    <div ref={setNodeRef} style={style} className={cn("will-change-transform", isDragging && "opacity-60")}>
      <LocalRoutingStatusCandidateRow candidate={candidate} attemptOrder={attemptOrder} nowMs={nowMs} dragDisabled={disabled} dragAttributes={attributes} dragListeners={listeners} />
    </div>
  );
}
