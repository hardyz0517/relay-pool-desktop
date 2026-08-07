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
import { EmptyState, StatusBadge, useToast } from "@/components/ui";
import { reorderKeyPool } from "@/lib/api/stationKeys";
import { readError } from "@/lib/errors";
import { queryKeys } from "@/lib/query/queryKeys";
import { synchronizeRoutingQueriesAfterMutation } from "@/lib/query/routingQuerySynchronization";
import type { RoutingCandidateView as LocalRoutingCandidate, RoutingWorkspaceView } from "@/lib/types/routingWorkspace";
import type { KeyPoolItem } from "@/lib/types/stationKeys";
import { cn } from "@/lib/utils";
import {
  LocalRoutingCandidateHeader,
  LocalRoutingCandidateRow,
} from "./LocalRoutingCandidateRow";
import { LocalRoutingSettingsEditor } from "./LocalRoutingSettingsEditor";
import { buildEditableRoutingCandidates } from "./editableRoutingCandidates";

type LocalRoutingEditTabProps = {
  workspace: RoutingWorkspaceView | null;
  keyPoolItems: readonly KeyPoolItem[] | undefined;
  loading: boolean;
};

type ReorderSyncState = "idle" | "saving" | "synced" | "failed";

const reorderSyncLabels: Record<ReorderSyncState, string | null> = {
  idle: null,
  saving: "保存中",
  synced: "已同步",
  failed: "保存失败",
};

const reorderSyncTones: Record<Exclude<ReorderSyncState, "idle">, "healthy" | "warning" | "error"> = {
  saving: "warning",
  synced: "healthy",
  failed: "error",
};

export function LocalRoutingEditTab({ workspace, keyPoolItems, loading }: LocalRoutingEditTabProps) {
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
    if (!workspace) return new Map<string, LocalRoutingCandidate>();
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
      return candidate ? [candidate] : [];
    }),
    [candidateById, candidateIds],
  );
  const syncLabel = reorderSyncLabels[syncState];

  useEffect(() => {
    keyPoolVersionRef.current += 1;
    saveOperationRef.current += 1;
    if (!keyPoolItems) {
      setCandidateIds([]);
      setSyncState("idle");
      setSyncError(null);
      return;
    }
    setCandidateIds(keyPoolItems.map((item) => item.id));
    setSyncState("idle");
    setSyncError(null);
  }, [keyPoolItems]);

  async function handleDragEnd(event: DragEndEvent) {
    if (syncState === "saving") {
      return;
    }

    const { active, over } = event;
    if (!over || active.id === over.id) {
      return;
    }

    const activeIndex = candidateIds.indexOf(String(active.id));
    const overIndex = candidateIds.indexOf(String(over.id));
    if (activeIndex === -1 || overIndex === -1) {
      return;
    }

    const previousCandidateIds = candidateIds;
    const nextStationKeyIds = [...candidateIds];
    const [moved] = nextStationKeyIds.splice(activeIndex, 1);
    nextStationKeyIds.splice(overIndex, 0, moved);
    const operationId = saveOperationRef.current + 1;
    const keyPoolVersionAtStart = keyPoolVersionRef.current;

    saveOperationRef.current = operationId;
    setCandidateIds(nextStationKeyIds);
    setSyncState("saving");
    setSyncError(null);

    try {
      const saved = await reorderKeyPool(nextStationKeyIds);
      queryClient.setQueryData(queryKeys.keyPool, saved);
    } catch (requestError) {
      if (operationId !== saveOperationRef.current || keyPoolVersionAtStart !== keyPoolVersionRef.current) {
        return;
      }
      setCandidateIds(previousCandidateIds);
      setSyncState("failed");
      const message = readError(requestError);
      setSyncError(message);
      toast.error("保存候选顺序失败", message);
      return;
    }

    const operationIsCurrent =
      operationId === saveOperationRef.current &&
      keyPoolVersionAtStart === keyPoolVersionRef.current;
    if (operationIsCurrent) {
      setCandidateIds(nextStationKeyIds);
      setSyncState("synced");
    }

    const [synchronization] = await Promise.all([
      synchronizeRoutingQueriesAfterMutation(queryClient),
      queryClient.invalidateQueries({ queryKey: queryKeys.keyPool }),
    ]);
    if (!synchronization.refreshed && operationIsCurrent) {
      toast.error("候选顺序已保存，但状态刷新失败", readError(synchronization.errors[0]));
    }
  }

  return (
    <div className="grid gap-3">
      <LocalRoutingSettingsEditor />

      <section className="grid gap-2" aria-labelledby="local-routing-edit-candidates-title">
        <header className="flex min-h-8 flex-wrap items-center justify-between gap-3">
          <h2
            id="local-routing-edit-candidates-title"
            className="text-sm font-semibold text-foreground"
          >
            候选预览与顺序修正
          </h2>
          {syncLabel && syncState !== "idle" ? (
            <StatusBadge tone={reorderSyncTones[syncState]}>{syncLabel}</StatusBadge>
          ) : null}
        </header>
        {syncError && (
          <div className="rounded-[var(--surface-radius)] border border-danger-border bg-danger-surface px-3 py-2 text-xs text-danger-foreground">
            {syncError}
          </div>
        )}
        {loading && !workspace ? (
          <div className="text-sm text-muted-foreground">正在加载候选 Key...</div>
        ) : workspace && !keyPoolItems ? (
          <div className="text-sm text-muted-foreground">Loading complete Key Pool...</div>
        ) : candidates.length === 0 ? (
          <EmptyState title="暂无候选 Key" description="尚未发现可用候选。" />
        ) : (
          <DndContext sensors={sensors} collisionDetection={closestCenter} onDragEnd={handleDragEnd}>
            <SortableContext items={candidateIds} strategy={verticalListSortingStrategy}>
              <div className="overflow-hidden rounded-[var(--surface-radius)] border border-border bg-surface">
                <LocalRoutingCandidateHeader />
                <div className="divide-y divide-border">
                  {candidates.map((candidate, index) => (
                    <SortableLocalRoutingCandidateRow
                      key={candidate.stationKeyId}
                      candidate={candidate}
                      order={index + 1}
                      syncState={syncState}
                      disabled={syncState === "saving"}
                    />
                  ))}
                </div>
              </div>
            </SortableContext>
          </DndContext>
        )}
      </section>
    </div>
  );
}
function SortableLocalRoutingCandidateRow({
  candidate,
  order,
  syncState,
  disabled,
}: {
  candidate: LocalRoutingCandidate;
  order: number;
  syncState: ReorderSyncState;
  disabled: boolean;
}) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({
    id: candidate.stationKeyId,
    disabled,
  });
  const style: CSSProperties = {
    transform: CSS.Transform.toString(transform),
    transition,
  };

  return (
    <div
      ref={setNodeRef}
      style={style}
      className={cn("will-change-transform", isDragging && "opacity-60")}
    >
      <LocalRoutingCandidateRow
        candidate={candidate}
        order={order}
        syncState={syncState}
        dragDisabled={disabled}
        dragAttributes={attributes}
        dragListeners={listeners}
      />
    </div>
  );
}
