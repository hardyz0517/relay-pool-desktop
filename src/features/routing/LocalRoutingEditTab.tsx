import { useEffect, useMemo, useRef, useState, type CSSProperties, type ReactNode } from "react";
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
import { BadgeCheck, ListOrdered, LockKeyhole } from "lucide-react";
import { EmptyState, SectionCard, StatusBadge, useToast } from "@/components/ui";
import { reorderLocalRoutingKeys } from "@/lib/api/localRouting";
import { readError } from "@/lib/errors";
import type { LocalRoutingCandidateRow as LocalRoutingCandidate, LocalRoutingWorkspace } from "@/lib/types/localRouting";
import { cn } from "@/lib/utils";
import { LocalRoutingCandidateRow } from "./LocalRoutingCandidateRow";

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

  if (loading && !workspace) {
    return (
      <SectionCard title="编辑预览">
        <div className="text-sm text-muted-foreground">正在加载本地路由工作区...</div>
      </SectionCard>
    );
  }

  if (!workspace) {
    return (
      <SectionCard title="编辑预览">
        <EmptyState title="暂无可编辑数据" description="当前仅展示本地路由编辑骨架。" />
      </SectionCard>
    );
  }

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
      <SectionCard title="自动调度" contentClassName="grid gap-3">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="flex min-w-0 items-center gap-3">
            <span className="flex h-10 w-10 shrink-0 items-center justify-center rounded-[10px] bg-blue-50 text-blue-700">
              <BadgeCheck className="h-5 w-5" />
            </span>
            <div className="min-w-0">
              <div className="truncate text-sm font-semibold text-slate-900">自动选择倍率上限内综合最优 Key</div>
              <div className="truncate text-xs text-muted-foreground">
                倍率上限：{workspace.settings.maxRateMultiplier == null ? "未设置" : `${workspace.settings.maxRateMultiplier}x`}；分组筛选：{formatRoutingGroupFilter(workspace.settings.routingGroupFilter)}
              </div>
            </div>
          </div>
          {syncLabel && syncState !== "idle" ? (
            <StatusBadge tone={reorderSyncTones[syncState]}>{syncLabel}</StatusBadge>
          ) : (
            <StatusBadge tone="info">自动保存</StatusBadge>
          )}
        </div>
        <div className="grid gap-2 text-xs text-slate-600 sm:grid-cols-2">
          <EditHint icon={<ListOrdered className="h-4 w-4" />} title="Sub2API 风格调度" body="运行时会综合低倍率、低负载、低错误率和低延迟重新选择，不会超过倍率上限。" />
          <EditHint icon={<LockKeyhole className="h-4 w-4" />} title="硬边界" body="分组筛选不会跨组兜底；倍率未知或过期的 Key 不参与路由。" />
        </div>
      </SectionCard>

      <SectionCard
        title="候选编辑列表"
        action={syncLabel && syncState !== "idle" ? <StatusBadge tone={reorderSyncTones[syncState]}>{syncLabel}</StatusBadge> : null}
        contentClassName="grid gap-2"
      >
        {syncError && (
          <div className="rounded-[var(--surface-radius)] border border-rose-100 bg-rose-50 px-3 py-2 text-xs text-rose-700">
            {syncError}
          </div>
        )}
        {candidates.length === 0 ? (
          <EmptyState title="暂无候选 Key" description="候选列表会随本地路由工作区加载后显示。" />
        ) : (
          <DndContext sensors={sensors} collisionDetection={closestCenter} onDragEnd={handleDragEnd}>
            <SortableContext items={candidateIds} strategy={verticalListSortingStrategy}>
              <div className="grid gap-2">
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

function EditHint({ icon, title, body }: { icon: ReactNode; title: string; body: string }) {
  return (
    <div className="flex items-start gap-2 rounded-[var(--surface-radius)] border border-slate-200 bg-slate-50 px-3 py-2">
      <span className="mt-0.5 shrink-0 text-slate-500">{icon}</span>
      <span className="min-w-0">
        <span className="block text-xs font-semibold text-slate-800">{title}</span>
        <span className="block text-xs leading-5 text-muted-foreground">{body}</span>
      </span>
    </div>
  );
}

function formatRoutingGroupFilter(filter: LocalRoutingWorkspace["settings"]["routingGroupFilter"]) {
  if (filter === "all_groups") return "全部分组";
  if (filter === "ungrouped_only") return "未绑定分组";
  if ("group_type" in filter) return `${filter.group_type} 分组`;
  if ("group_binding_id" in filter) return "指定绑定";
  if ("group_id_hash" in filter) return "指定分组";
  return "全部分组";
}
