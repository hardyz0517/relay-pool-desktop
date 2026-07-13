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
import { EmptyState, SectionCard, StatusBadge, useToast } from "@/components/ui";
import { reorderLocalRoutingKeys } from "@/lib/api/localRouting";
import { readError } from "@/lib/errors";
import type { LocalRoutingCandidateRow as LocalRoutingCandidate, LocalRoutingWorkspace } from "@/lib/types/localRouting";
import { cn } from "@/lib/utils";
import { LocalRoutingCandidateRow } from "./LocalRoutingCandidateRow";
import { LocalRoutingSettingsEditor } from "./LocalRoutingSettingsEditor";

type LocalRoutingEditTabProps = {
  workspace: LocalRoutingWorkspace | null;
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

export function LocalRoutingEditTab({ workspace, loading }: LocalRoutingEditTabProps) {
  const toast = useToast();
  const [candidates, setCandidates] = useState<LocalRoutingCandidate[]>([]);
  const [syncState, setSyncState] = useState<ReorderSyncState>("idle");
  const [syncError, setSyncError] = useState<string | null>(null);
  const saveOperationRef = useRef(0);
  const workspaceVersionRef = useRef(0);
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 6 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  );
  const candidateIds = useMemo(
    () => candidates.map((candidate) => candidate.stationKeyId),
    [candidates],
  );
  const syncLabel = reorderSyncLabels[syncState];

  useEffect(() => {
    workspaceVersionRef.current += 1;
    saveOperationRef.current += 1;
    if (!workspace) {
      setCandidates([]);
      setSyncState("idle");
      setSyncError(null);
      return;
    }
    setCandidates(workspace.candidates);
    setSyncState("idle");
    setSyncError(null);
  }, [workspace]);

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

    const previousCandidates = candidates;
    const nextCandidates = [...candidates];
    const [moved] = nextCandidates.splice(activeIndex, 1);
    nextCandidates.splice(overIndex, 0, moved);
    const nextStationKeyIds = nextCandidates.map((candidate) => candidate.stationKeyId);
    const operationId = saveOperationRef.current + 1;
    const workspaceVersionAtStart = workspaceVersionRef.current;

    saveOperationRef.current = operationId;
    setCandidates(nextCandidates);
    setSyncState("saving");
    setSyncError(null);

    try {
      const nextWorkspace = await reorderLocalRoutingKeys({ stationKeyIds: nextStationKeyIds });
      if (operationId !== saveOperationRef.current || workspaceVersionAtStart !== workspaceVersionRef.current) {
        return;
      }
      setCandidates(
        nextWorkspace.candidates.length === nextCandidates.length
          ? nextWorkspace.candidates
          : nextCandidates,
      );
      setSyncState("synced");
    } catch (requestError) {
      if (operationId !== saveOperationRef.current || workspaceVersionAtStart !== workspaceVersionRef.current) {
        return;
      }
      setCandidates(previousCandidates);
      setSyncState("failed");
      const message = readError(requestError);
      setSyncError(message);
      toast.error("保存候选顺序失败", message);
    }
  }

  return (
    <div className="grid gap-3">
      <LocalRoutingSettingsEditor workspace={workspace} />

      <SectionCard
        title="候选预览与顺序修正"
        action={syncLabel && syncState !== "idle" ? <StatusBadge tone={reorderSyncTones[syncState]}>{syncLabel}</StatusBadge> : null}
        contentClassName="grid gap-2"
      >
        {syncError && (
          <div className="rounded-[var(--surface-radius)] border border-rose-100 bg-rose-50 px-3 py-2 text-xs text-rose-700">
            {syncError}
          </div>
        )}
        {loading && !workspace ? (
          <div className="text-sm text-muted-foreground">正在加载候选 Key...</div>
        ) : candidates.length === 0 ? (
          <EmptyState title="暂无候选 Key" description="尚未发现可用候选。" />
        ) : (
          <DndContext sensors={sensors} collisionDetection={closestCenter} onDragEnd={handleDragEnd}>
            <SortableContext items={candidateIds} strategy={verticalListSortingStrategy}>
              <div className="divide-y divide-slate-100">
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
            </SortableContext>
          </DndContext>
        )}
      </SectionCard>
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
