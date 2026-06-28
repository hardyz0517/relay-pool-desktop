import { useEffect, useMemo, useState, type ComponentPropsWithoutRef, type FormEvent } from "react";
import {
  closestCenter,
  DndContext,
  DragOverlay,
  PointerSensor,
  type DragEndEvent,
  type DragStartEvent,
  useSensor,
  useSensors,
} from "@dnd-kit/core";
import { SortableContext, useSortable, verticalListSortingStrategy } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { CheckCircle2, Edit3, GripVertical, RefreshCcw, Search, Trash2 } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import { Button, Dialog, EmptyState, StatusBadge, Toolbar } from "@/components/ui";
import { getStationKeyCapabilities, updateStationKeyCapabilities } from "@/lib/api/routing";
import { listStations } from "@/lib/api/stations";
import { deleteStationKey, listKeyPoolItems, reorderKeyPool, updateStationKey } from "@/lib/api/stationKeys";
import type { StationKeyCapabilities } from "@/lib/types/routing";
import { stationTypeLabels, type Station } from "@/lib/types/stations";
import type { KeyPoolItem, StationKeyStatus } from "@/lib/types/stationKeys";
import { cn } from "@/lib/utils";

type FilterMode = "all" | "enabled" | "disabled";

const statusTone: Record<StationKeyStatus, "healthy" | "warning" | "error" | "disabled" | "info"> = {
  unchecked: "info",
  healthy: "healthy",
  warning: "warning",
  error: "error",
  disabled: "disabled",
};

export function KeyPoolPage() {
  const [stations, setStations] = useState<Station[]>([]);
  const [items, setItems] = useState<KeyPoolItem[]>([]);
  const [selectedStationId, setSelectedStationId] = useState<string>("all");
  const [filterMode, setFilterMode] = useState<FilterMode>("all");
  const [query, setQuery] = useState("");
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [activeDragId, setActiveDragId] = useState<string | null>(null);
  const [editingItem, setEditingItem] = useState<KeyPoolItem | null>(null);
  const [editForm, setEditForm] = useState<KeyPoolEditForm>(emptyEditForm);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const sensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 8 } }));
  const activeDragItem = useMemo(
    () => items.find((item) => item.id === activeDragId) ?? null,
    [activeDragId, items],
  );

  useEffect(() => {
    void refresh();
  }, []);

  const filteredItems = useMemo(() => {
    return items.filter((item) => {
      if (selectedStationId !== "all" && item.stationId !== selectedStationId) {
        return false;
      }
      if (filterMode === "enabled" && !item.enabled) {
        return false;
      }
      if (filterMode === "disabled" && item.enabled) {
        return false;
      }
      if (query.trim()) {
        const text = `${item.name} ${item.apiKeyMasked} ${item.stationName} ${item.groupName ?? ""} ${item.tierLabel ?? ""}`.toLowerCase();
        if (!text.includes(query.trim().toLowerCase())) {
          return false;
        }
      }
      return true;
    });
  }, [filterMode, items, query, selectedStationId]);
  const dragEnabled = filteredItems.length === items.length;

  const stationOptions = useMemo(
    () => stations.map((station) => ({ id: station.id, label: station.name })),
    [stations],
  );

  async function refresh() {
    setLoading(true);
    setError(null);
    try {
      const [nextStations, nextItems] = await Promise.all([listStations(), listKeyPoolItems()]);
      setStations(nextStations);
      setItems(nextItems);
    } catch (requestError) {
      setError(readError(requestError));
    } finally {
      setLoading(false);
    }
  }

  function handleDragStart(event: DragStartEvent) {
    setActiveDragId(String(event.active.id));
  }

  function handleDragCancel() {
    setActiveDragId(null);
  }

  async function handleDragEnd(event: DragEndEvent) {
    const { active, over } = event;
    setActiveDragId(null);
    if (!over || active.id === over.id) {
      return;
    }
    if (!dragEnabled) {
      setError("清除筛选后可调整全局顺序。");
      return;
    }
    const oldIndex = filteredItems.findIndex((item) => item.id === active.id);
    const newIndex = filteredItems.findIndex((item) => item.id === over.id);
    if (oldIndex < 0 || newIndex < 0) {
      return;
    }
    const previousItems = items;
    const nextVisible = [...filteredItems];
    const [moved] = nextVisible.splice(oldIndex, 1);
    nextVisible.splice(newIndex, 0, moved);
    const visibleIds = new Set(nextVisible.map((item) => item.id));
    const nextOrder: KeyPoolItem[] = [];
    let visibleCursor = 0;
    for (const item of items) {
      if (!visibleIds.has(item.id)) {
        nextOrder.push(item);
        continue;
      }
      nextOrder.push(nextVisible[visibleCursor++]);
    }
    setItems(nextOrder);
    setSaving(true);
    try {
      const saved = await reorderKeyPool(nextOrder.map((item) => item.id));
      setItems(saved);
      setMessage("Key 池排序已保存。");
    } catch (requestError) {
      setItems(previousItems);
      setError(readError(requestError));
    } finally {
      setSaving(false);
    }
  }

  async function handleToggleEnabled(item: KeyPoolItem) {
    setSaving(true);
    setError(null);
    try {
      await updateStationKey({
        id: item.id,
        stationId: item.stationId,
        name: item.name,
        apiKey: null,
        enabled: !item.enabled,
        priority: item.priority,
        groupName: item.groupName,
        tierLabel: item.tierLabel,
        status: item.status,
        note: item.note,
      });
      await refresh();
      setMessage(item.enabled ? "Key 已禁用。" : "Key 已启用。");
    } catch (requestError) {
      setError(readError(requestError));
    } finally {
      setSaving(false);
    }
  }

  async function handleDelete(item: KeyPoolItem) {
    if (!window.confirm(`确认删除 Key「${item.name}」？`)) {
      return;
    }
    setSaving(true);
    setError(null);
    try {
      await deleteStationKey(item.id);
      await refresh();
      setMessage("Key 已删除。");
    } catch (requestError) {
      setError(readError(requestError));
    } finally {
      setSaving(false);
    }
  }

  async function handleEdit(item: KeyPoolItem) {
    setEditingItem(item);
    setEditForm(formFromItem(item));
    setSaving(true);
    setError(null);
    try {
      const capabilities = await getStationKeyCapabilities(item.id);
      setEditForm((current) => current.id === item.id ? mergeCapabilitiesIntoForm(current, capabilities) : current);
    } catch (requestError) {
      setError(`读取 Key 路由能力失败：${readError(requestError)}`);
    } finally {
      setSaving(false);
    }
  }

  async function handleEditSave(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!editingItem) {
      return;
    }
    setSaving(true);
    setError(null);
    try {
      await updateStationKey({
        id: editForm.id,
        stationId: editForm.stationId,
        name: editForm.name.trim(),
        apiKey: editForm.apiKey.trim() ? editForm.apiKey.trim() : null,
        enabled: editForm.enabled,
        priority: Number(editForm.priority),
        groupName: editForm.groupName.trim() ? editForm.groupName.trim() : null,
        tierLabel: editForm.tierLabel.trim() ? editForm.tierLabel.trim() : null,
        status: editForm.status,
        note: editForm.note.trim() ? editForm.note.trim() : null,
      });
      try {
        await updateStationKeyCapabilities({
          stationKeyId: editForm.id,
          supportsChatCompletions: editForm.supportsChatCompletions,
          supportsResponses: editForm.supportsResponses,
          supportsEmbeddings: editForm.supportsEmbeddings,
          supportsStream: editForm.supportsStream,
          supportsTools: editForm.supportsTools,
          supportsVision: editForm.supportsVision,
          supportsReasoning: editForm.supportsReasoning,
          modelAllowlist: linesToList(editForm.modelAllowlist),
          modelBlocklist: linesToList(editForm.modelBlocklist),
          preferredModels: linesToList(editForm.preferredModels),
          onlyUseAsBackup: editForm.onlyUseAsBackup,
          routingTags: commaListToList(editForm.routingTags),
        });
      } catch (capabilityError) {
        await refresh();
        throw new Error(`Key 基础信息已保存，但路由能力保存失败：${readError(capabilityError)}`);
      }
      setEditingItem(null);
      await refresh();
      setMessage("Key 已更新。");
    } catch (requestError) {
      setError(readError(requestError));
    } finally {
      setSaving(false);
    }
  }

  return (
    <PageScaffold
      title="Key 池"
      description="统一管理所有中转站账号下的 API Key。这里的排序、启用状态和健康信息会决定后续本地路由如何选择出口。"
      actions={
        <div className="flex items-center gap-2">
          <select className={selectClassName} value={selectedStationId} onChange={(event) => setSelectedStationId(event.target.value)}>
            <option value="all">全部中转站</option>
            {stationOptions.map((station) => (
              <option key={station.id} value={station.id}>
                {station.label}
              </option>
            ))}
          </select>
          <select className={selectClassName} value={filterMode} onChange={(event) => setFilterMode(event.target.value as FilterMode)}>
            <option value="all">全部状态</option>
            <option value="enabled">只看启用</option>
            <option value="disabled">只看禁用</option>
          </select>
          <div className="relative">
            <Search className="pointer-events-none absolute left-2.5 top-2 h-4 w-4 text-muted-foreground" />
            <input className={`${selectClassName} pl-8`} value={query} onChange={(event) => setQuery(event.target.value)} placeholder="搜索 key / 站点" />
          </div>
          <Button variant="secondary" onClick={() => void refresh()} disabled={loading || saving}>
            <RefreshCcw className="h-4 w-4" />
            刷新
          </Button>
        </div>
      }
    >
      {loading ? (
        <div className="rounded-[var(--surface-radius)] border border-cyan-100 bg-white/85 px-4 py-5 text-sm text-muted-foreground">
          正在读取 Key 池...
        </div>
      ) : filteredItems.length === 0 ? (
        <EmptyState
          title="还没有可管理的 Key"
          description="先在中转站页创建一个站点和它下面的 API Key。"
        />
      ) : (
        <div className="space-y-[var(--shell-page-gap)]">
          <Toolbar>
            <div className="min-w-0">
              <div className="text-[13px] font-semibold text-slate-800">Key 池列表</div>
              <div className="text-xs text-muted-foreground">
                {filteredItems.length} 个 Key，{filteredItems.filter((item) => item.enabled).length} 个启用，{saving ? "保存中" : "等待操作"}。
              </div>
              {!dragEnabled && (
                <div className="mt-1 text-xs text-amber-700">当前有筛选条件，拖拽排序已禁用。</div>
              )}
            </div>
          </Toolbar>

          <DndContext
            sensors={sensors}
            collisionDetection={closestCenter}
            onDragStart={handleDragStart}
            onDragCancel={handleDragCancel}
            onDragEnd={handleDragEnd}
          >
            <SortableContext items={filteredItems.map((item) => item.id)} strategy={verticalListSortingStrategy}>
              <div className="space-y-2">
                {filteredItems.map((item) => (
                  <SortableKeyRow
                    key={item.id}
                    item={item}
                    dragEnabled={dragEnabled}
                    onEdit={handleEdit}
                    onDelete={handleDelete}
                    onToggleEnabled={handleToggleEnabled}
                  />
                ))}
              </div>
            </SortableContext>
            <DragOverlay dropAnimation={null}>
              {activeDragItem ? <KeyRowContent overlay item={activeDragItem} /> : null}
            </DragOverlay>
          </DndContext>
        </div>
      )}

      {(message || error) && (
        <div
          className={cn(
            "fixed bottom-4 right-4 z-40 rounded-2xl border px-4 py-3 text-sm shadow-[0_12px_30px_rgba(33,79,88,0.12)]",
            error ? "border-rose-200 bg-rose-50 text-rose-700" : "border-emerald-200 bg-emerald-50 text-emerald-700",
          )}
        >
          {error ?? message}
        </div>
      )}

      {editingItem && (
        <KeyEditDialog
          actionSaving={saving}
          form={editForm}
          onClose={() => setEditingItem(null)}
          onFormChange={setEditForm}
          onSave={handleEditSave}
        />
      )}
    </PageScaffold>
  );
}

function SortableKeyRow({
  item,
  dragEnabled,
  onEdit,
  onToggleEnabled,
  onDelete,
}: {
  item: KeyPoolItem;
  dragEnabled: boolean;
  onEdit: (item: KeyPoolItem) => void;
  onToggleEnabled: (item: KeyPoolItem) => void;
  onDelete: (item: KeyPoolItem) => void;
}) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({ id: item.id, disabled: !dragEnabled });
  return (
    <div ref={setNodeRef} style={{ transform: CSS.Transform.toString(transform), transition }} className={cn("will-change-transform", isDragging && "opacity-35")}>
      <KeyRowContent item={item} dragAttributes={dragEnabled ? attributes : undefined} dragListeners={dragEnabled ? listeners : undefined} dragDisabled={!dragEnabled} onEdit={onEdit} onToggleEnabled={onToggleEnabled} onDelete={onDelete} />
    </div>
  );
}

function KeyRowContent({
  item,
  overlay = false,
  dragDisabled = false,
  dragAttributes,
  dragListeners,
  onEdit,
  onToggleEnabled,
  onDelete,
}: {
  item: KeyPoolItem;
  overlay?: boolean;
  dragDisabled?: boolean;
  dragAttributes?: ComponentPropsWithoutRef<"button">;
  dragListeners?: ReturnType<typeof useSortable>["listeners"];
  onEdit?: (item: KeyPoolItem) => void;
  onToggleEnabled?: (item: KeyPoolItem) => void;
  onDelete?: (item: KeyPoolItem) => void;
}) {
  const cooldownActive = isFutureTime(item.cooldownUntil);
  const healthSummary = [
    item.successRate === null ? "成功率暂无" : `成功率 ${Math.round(item.successRate * 100)}%`,
    item.avgLatencyMs === null ? "延迟暂无" : `平均 ${item.avgLatencyMs}ms`,
    item.consecutiveFailures > 0 ? `连续失败 ${item.consecutiveFailures}` : "无连续失败",
  ].join(" · ");
  return (
    <div className={cn("grid min-h-[72px] grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-3 rounded-[var(--surface-radius)] border border-border bg-white px-3 py-2 text-left shadow-[var(--surface-shadow)] transition-colors", overlay ? "border-teal-300 shadow-[0_14px_28px_rgba(13,148,136,0.18)]" : "border-cyan-100 hover:border-teal-200 hover:bg-teal-50/25")}>
      <button type="button" className={cn("flex h-9 w-9 items-center justify-center rounded-[12px] border border-cyan-100 bg-cyan-50 transition", dragDisabled ? "cursor-not-allowed text-slate-300" : "cursor-grab text-slate-400 active:cursor-grabbing hover:text-teal-700")} aria-label="拖拽排序" title={dragDisabled ? "清除筛选后可拖拽排序" : "拖拽排序"} {...dragAttributes} {...dragListeners}>
        <GripVertical className="h-4 w-4" />
      </button>
      <div className="min-w-0">
        <div className="flex min-w-0 items-center gap-2">
          <div className="truncate text-[13px] font-semibold text-slate-800">{item.name}</div>
          <StatusBadge tone={statusTone[item.status]}>{item.status}</StatusBadge>
          <span className="rounded-full border border-cyan-100 bg-cyan-50 px-2 py-0.5 text-[11px] text-slate-600">P{item.priority}</span>
          {item.onlyUseAsBackup && <StatusBadge tone="warning">备用</StatusBadge>}
          {cooldownActive && <StatusBadge tone="warning">冷却中</StatusBadge>}
        </div>
        <div className="mt-1 flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
          <span>{item.apiKeyMasked}</span>
          <span>{item.groupName ?? "未分组"}</span>
          <span>{item.tierLabel ?? "无 tier"}</span>
          <span>{item.enabled ? "启用" : "禁用"}</span>
        </div>
        <div className="mt-1 text-xs text-muted-foreground">
          所属中转站：{item.stationName} · {stationTypeLabels[item.stationType as keyof typeof stationTypeLabels] ?? item.stationType}
        </div>
        <div className="mt-1 flex flex-wrap items-center gap-1.5 text-xs text-muted-foreground">
          <span>协议：</span>
          {(item.capabilitySummary.length ? item.capabilitySummary : ["未配置"]).map((capability) => (
            <span key={capability} className="rounded-full border border-slate-200 bg-slate-50 px-2 py-0.5 text-[11px] text-slate-600">
              {capability}
            </span>
          ))}
        </div>
        <div className="mt-1 text-xs text-muted-foreground">
          模型：{item.modelScopeSummary || "全部模型"} · 健康：{healthSummary}
          {cooldownActive ? ` · 冷却至 ${formatNullableTime(item.cooldownUntil)}` : ""}
        </div>
        {item.lastErrorSummary && (
          <div className="mt-1 truncate text-xs text-rose-600">最近错误：{item.lastErrorSummary}</div>
        )}
        <div className="mt-1 text-xs text-muted-foreground">
          最近使用：{formatNullableTime(item.lastUsedAt)} · 最近检查：{formatNullableTime(item.lastCheckedAt)}
        </div>
      </div>
      <div className="flex items-center gap-2 justify-self-end">
        <Button variant={item.enabled ? "secondary" : "outline"} className="h-8" onClick={() => onToggleEnabled?.(item)} disabled={overlay}>
          <CheckCircle2 className="h-4 w-4" />
          {item.enabled ? "停用" : "启用"}
        </Button>
        <Button variant="outline" className="h-8 w-8 px-0" title="编辑" onClick={() => onEdit?.(item)}>
          <Edit3 className="h-4 w-4" />
        </Button>
        <Button variant="danger" className="h-8 w-8 px-0" title="删除" onClick={() => onDelete?.(item)}>
          <Trash2 className="h-4 w-4" />
        </Button>
      </div>
    </div>
  );
}

type KeyPoolEditForm = {
  id: string;
  stationId: string;
  stationName: string;
  name: string;
  apiKey: string;
  enabled: boolean;
  priority: string;
  groupName: string;
  tierLabel: string;
  status: StationKeyStatus;
  note: string;
  supportsChatCompletions: boolean;
  supportsResponses: boolean;
  supportsEmbeddings: boolean;
  supportsStream: boolean;
  supportsTools: boolean;
  supportsVision: boolean;
  supportsReasoning: boolean;
  modelAllowlist: string;
  modelBlocklist: string;
  preferredModels: string;
  onlyUseAsBackup: boolean;
  routingTags: string;
};

const emptyEditForm: KeyPoolEditForm = {
  id: "",
  stationId: "",
  stationName: "",
  name: "",
  apiKey: "",
  enabled: true,
  priority: "0",
  groupName: "",
  tierLabel: "",
  status: "unchecked",
  note: "",
  supportsChatCompletions: true,
  supportsResponses: true,
  supportsEmbeddings: false,
  supportsStream: true,
  supportsTools: false,
  supportsVision: false,
  supportsReasoning: false,
  modelAllowlist: "",
  modelBlocklist: "",
  preferredModels: "",
  onlyUseAsBackup: false,
  routingTags: "",
};

function KeyEditDialog({
  actionSaving,
  form,
  onClose,
  onFormChange,
  onSave,
}: {
  actionSaving: boolean;
  form: KeyPoolEditForm;
  onClose: () => void;
  onFormChange: (next: KeyPoolEditForm) => void;
  onSave: (event: FormEvent<HTMLFormElement>) => void;
}) {
  return (
    <Dialog
      open
      title="编辑 Key"
      description="API Key 留空则保留旧值。"
      onClose={onClose}
      footer={
        <div className="flex justify-end gap-2">
          <Button variant="outline" onClick={onClose}>取消</Button>
          <Button type="submit" form="key-pool-edit-form" disabled={actionSaving}>{actionSaving ? "保存中" : "保存"}</Button>
        </div>
      }
    >
      <form id="key-pool-edit-form" className="grid gap-4 p-5" onSubmit={onSave}>
        <div className="grid gap-3 md:grid-cols-2">
          <Field label="名称">
            <input className={inputClassName} value={form.name} onChange={(event) => onFormChange({ ...form, name: event.target.value })} required />
          </Field>
          <Field label="优先级">
            <input className={inputClassName} type="number" value={form.priority} onChange={(event) => onFormChange({ ...form, priority: event.target.value })} />
          </Field>
        </div>
        <Field label="所属中转站">
          <input className={inputClassName} value={form.stationName} disabled />
        </Field>
        <Field label="API Key">
          <input className={inputClassName} value={form.apiKey} onChange={(event) => onFormChange({ ...form, apiKey: event.target.value })} placeholder="留空保留旧 key" />
        </Field>
        <div className="grid gap-3 md:grid-cols-3">
          <Field label="分组">
            <input className={inputClassName} value={form.groupName} onChange={(event) => onFormChange({ ...form, groupName: event.target.value })} />
          </Field>
          <Field label="Tier">
            <input className={inputClassName} value={form.tierLabel} onChange={(event) => onFormChange({ ...form, tierLabel: event.target.value })} />
          </Field>
          <Field label="状态">
            <select className={inputClassName} value={form.status} onChange={(event) => onFormChange({ ...form, status: event.target.value as StationKeyStatus })}>
              <option value="unchecked">未检测</option>
              <option value="healthy">正常</option>
              <option value="warning">警告</option>
              <option value="error">错误</option>
              <option value="disabled">禁用</option>
            </select>
          </Field>
        </div>
        <label className="flex items-center gap-2 text-sm text-slate-700">
          <input checked={form.enabled} className="h-4 w-4 accent-teal-600" type="checkbox" onChange={(event) => onFormChange({ ...form, enabled: event.target.checked })} />
          启用
        </label>
        <div className="grid gap-2 rounded-[var(--surface-radius)] border border-cyan-100 bg-cyan-50/25 p-3">
          <div className="text-xs font-semibold text-slate-700">协议能力</div>
          <div className="grid gap-2 sm:grid-cols-2 md:grid-cols-3">
            <CheckField label="Chat Completions" checked={form.supportsChatCompletions} onChange={(checked) => onFormChange({ ...form, supportsChatCompletions: checked })} />
            <CheckField label="Responses" checked={form.supportsResponses} onChange={(checked) => onFormChange({ ...form, supportsResponses: checked })} />
            <CheckField label="Embeddings" checked={form.supportsEmbeddings} onChange={(checked) => onFormChange({ ...form, supportsEmbeddings: checked })} />
            <CheckField label="Stream" checked={form.supportsStream} onChange={(checked) => onFormChange({ ...form, supportsStream: checked })} />
            <CheckField label="Tools" checked={form.supportsTools} onChange={(checked) => onFormChange({ ...form, supportsTools: checked })} />
            <CheckField label="Vision" checked={form.supportsVision} onChange={(checked) => onFormChange({ ...form, supportsVision: checked })} />
            <CheckField label="Reasoning" checked={form.supportsReasoning} onChange={(checked) => onFormChange({ ...form, supportsReasoning: checked })} />
          </div>
        </div>
        <div className="grid gap-3 md:grid-cols-3">
          <Field label="允许模型">
            <textarea className={`${inputClassName} min-h-24 resize-none py-2`} value={form.modelAllowlist} onChange={(event) => onFormChange({ ...form, modelAllowlist: event.target.value })} placeholder="每行一个模型；留空表示全部模型" />
          </Field>
          <Field label="禁止模型">
            <textarea className={`${inputClassName} min-h-24 resize-none py-2`} value={form.modelBlocklist} onChange={(event) => onFormChange({ ...form, modelBlocklist: event.target.value })} placeholder="每行一个模型" />
          </Field>
          <Field label="优先模型">
            <textarea className={`${inputClassName} min-h-24 resize-none py-2`} value={form.preferredModels} onChange={(event) => onFormChange({ ...form, preferredModels: event.target.value })} placeholder="每行一个模型" />
          </Field>
        </div>
        <div className="grid gap-3 md:grid-cols-[auto_minmax(0,1fr)]">
          <label className="flex items-center gap-2 text-sm text-slate-700">
            <input checked={form.onlyUseAsBackup} className="h-4 w-4 accent-teal-600" type="checkbox" onChange={(event) => onFormChange({ ...form, onlyUseAsBackup: event.target.checked })} />
            仅作为备用 Key
          </label>
          <Field label="路由标签">
            <input className={inputClassName} value={form.routingTags} onChange={(event) => onFormChange({ ...form, routingTags: event.target.value })} placeholder="逗号分隔，例如 pro, low-latency" />
          </Field>
        </div>
        <Field label="备注">
          <textarea className={`${inputClassName} min-h-20 resize-none py-2`} value={form.note} onChange={(event) => onFormChange({ ...form, note: event.target.value })} />
        </Field>
      </form>
    </Dialog>
  );
}

function readError(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function formatNullableTime(value: string | null) {
  if (!value) {
    return "暂无";
  }
  const numeric = Number(value);
  const date = Number.isFinite(numeric) && numeric > 1000000000000 ? new Date(numeric) : new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return date.toLocaleString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

const selectClassName =
  "h-8 rounded-[12px] border border-cyan-100 bg-cyan-50/45 px-3 text-sm text-slate-800 outline-none transition focus:border-teal-300 focus:bg-white focus:ring-2 focus:ring-teal-100";

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <label className="grid gap-1.5 text-xs font-medium text-muted-foreground">
      {label}
      {children}
    </label>
  );
}

function CheckField({
  label,
  checked,
  onChange,
}: {
  label: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <label className="flex items-center gap-2 text-sm text-slate-700">
      <input
        checked={checked}
        className="h-4 w-4 accent-teal-600"
        type="checkbox"
        onChange={(event) => onChange(event.target.checked)}
      />
      {label}
    </label>
  );
}

function formFromItem(item: KeyPoolItem): KeyPoolEditForm {
  return {
    id: item.id,
    stationId: item.stationId,
    stationName: item.stationName,
    name: item.name,
    apiKey: "",
    enabled: item.enabled,
    priority: String(item.priority),
    groupName: item.groupName ?? "",
    tierLabel: item.tierLabel ?? "",
    status: item.status,
    note: item.note ?? "",
    supportsChatCompletions: true,
    supportsResponses: true,
    supportsEmbeddings: false,
    supportsStream: true,
    supportsTools: false,
    supportsVision: false,
    supportsReasoning: false,
    modelAllowlist: "",
    modelBlocklist: "",
    preferredModels: "",
    onlyUseAsBackup: item.onlyUseAsBackup,
    routingTags: "",
  };
}

function mergeCapabilitiesIntoForm(
  form: KeyPoolEditForm,
  capabilities: StationKeyCapabilities,
): KeyPoolEditForm {
  return {
    ...form,
    supportsChatCompletions: capabilities.supportsChatCompletions,
    supportsResponses: capabilities.supportsResponses,
    supportsEmbeddings: capabilities.supportsEmbeddings,
    supportsStream: capabilities.supportsStream,
    supportsTools: capabilities.supportsTools,
    supportsVision: capabilities.supportsVision,
    supportsReasoning: capabilities.supportsReasoning,
    modelAllowlist: capabilities.modelAllowlist.join("\n"),
    modelBlocklist: capabilities.modelBlocklist.join("\n"),
    preferredModels: capabilities.preferredModels.join("\n"),
    onlyUseAsBackup: capabilities.onlyUseAsBackup,
    routingTags: capabilities.routingTags.join(", "),
  };
}

function linesToList(value: string) {
  return Array.from(
    new Set(
      value
        .split(/\r?\n/)
        .map((item) => item.trim())
        .filter(Boolean),
    ),
  );
}

function commaListToList(value: string) {
  return Array.from(
    new Set(
      value
        .split(",")
        .map((item) => item.trim())
        .filter(Boolean),
    ),
  );
}

function isFutureTime(value: string | null) {
  if (!value) {
    return false;
  }
  const numeric = Number(value);
  const date = Number.isFinite(numeric) && numeric > 1000000000000 ? new Date(numeric) : new Date(value);
  return !Number.isNaN(date.getTime()) && date.getTime() > Date.now();
}

const inputClassName =
  "h-8 rounded-[12px] border border-cyan-100 bg-cyan-50/40 px-3 text-sm text-slate-800 outline-none transition focus:border-teal-300 focus:bg-white focus:ring-2 focus:ring-teal-100";
