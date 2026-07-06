import { useEffect, useMemo, useState, type FormEvent } from "react";
import {
  closestCenter,
  type DraggableAttributes,
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
import { Activity, Edit3, GripVertical, KeyRound, Loader2, Plus, Search, Trash2 } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import { Button, ConfirmDialog, Dialog, EmptyState, IconButton, SelectControl, StatusBadge, SwitchControl, type StatusTone, useToast } from "@/components/ui";
import { createChannelMonitor, listChannelMonitorTemplates, listChannelMonitors, updateChannelMonitor } from "@/lib/api/channelMonitors";
import { listStationGroupBindings } from "@/lib/api/groupFacts";
import { getStationKeyCapabilities, updateStationKeyCapabilities } from "@/lib/api/routing";
import { listStations } from "@/lib/api/stations";
import { createStationKey, deleteStationKey, listKeyPoolItems, reorderKeyPool, testStationKeyConnectivity, updateStationKey, updateStationKeyGroupBinding } from "@/lib/api/stationKeys";
import type { ChannelMonitor, ChannelMonitorRequestTemplate } from "@/lib/types/channelMonitors";
import { isCollectedStationGroupBinding, type StationGroupBinding } from "@/lib/types/groupFacts";
import type { StationKeyCapabilities } from "@/lib/types/routing";
import type { Station } from "@/lib/types/stations";
import type { CreateStationKeyInput, KeyPoolItem, StationKeyStatus } from "@/lib/types/stationKeys";
import { cn } from "@/lib/utils";
import {
  createStationKeyMonitorInput,
  findStationKeyMonitor,
  preferredStationKeyMonitorTemplate,
  updateStationKeyMonitorEnabledInput,
} from "@/features/channels/channelMonitorViewModel";

type FilterMode = "all" | "enabled" | "disabled";

type KeyPoolPageProps = {
  onAddKey?: (stationId: string | null) => void;
  onEditKey?: (stationKeyId: string) => void;
};

const statusTone: Record<StationKeyStatus, StatusTone> = {
  unchecked: "info",
  healthy: "healthy",
  warning: "warning",
  error: "error",
  disabled: "disabled",
};

const statusLabels: Record<StationKeyStatus, string> = {
  unchecked: "未检测",
  healthy: "正常",
  warning: "警告",
  error: "错误",
  disabled: "禁用",
};

const keyPoolGridClassName =
  "grid min-w-[780px] grid-cols-[2rem_minmax(18rem,1fr)_7rem_5rem_5rem_12rem_5.5rem] items-center gap-3";

export function KeyPoolPage({ onAddKey, onEditKey }: KeyPoolPageProps) {
  const toast = useToast();
  const [stations, setStations] = useState<Station[]>([]);
  const [items, setItems] = useState<KeyPoolItem[]>([]);
  const [monitors, setMonitors] = useState<ChannelMonitor[]>([]);
  const [monitorTemplates, setMonitorTemplates] = useState<ChannelMonitorRequestTemplate[]>([]);
  const [selectedStationId, setSelectedStationId] = useState<string>("all");
  const [filterMode, setFilterMode] = useState<FilterMode>("all");
  const [query, setQuery] = useState("");
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [activeDragId, setActiveDragId] = useState<string | null>(null);
  const [creatingKey, setCreatingKey] = useState(false);
  const [editingItem, setEditingItem] = useState<KeyPoolItem | null>(null);
  const [pendingDeleteItem, setPendingDeleteItem] = useState<KeyPoolItem | null>(null);
  const [editForm, setEditForm] = useState<KeyPoolEditForm>(emptyEditForm);
  const [bindingsForEdit, setBindingsForEdit] = useState<StationGroupBinding[]>([]);
  const [testingKeyId, setTestingKeyId] = useState<string | null>(null);
  const [monitoringKeyId, setMonitoringKeyId] = useState<string | null>(null);
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
        const text = `${item.name} ${item.stationBaseUrl} ${item.stationName} ${item.groupName ?? ""} ${item.tierLabel ?? ""}`.toLowerCase();
        if (!text.includes(query.trim().toLowerCase())) {
          return false;
        }
      }
      return true;
    });
  }, [filterMode, items, query, selectedStationId]);
  const dragEnabled = filteredItems.length === items.length;
  const filteredEnabledCount = filteredItems.filter((item) => item.enabled).length;
  const monitorByKey = useMemo(() => {
    const entries = items.flatMap((item) => {
      const monitor = findStationKeyMonitor(monitors, item.id);
      return monitor ? [[item.id, monitor] as const] : [];
    });
    return new Map(entries);
  }, [items, monitors]);

  const stationOptions = useMemo(
    () => stations.map((station) => ({ id: station.id, label: station.name })),
    [stations],
  );

  async function refresh() {
    setLoading(true);
    setError(null);
    try {
      const [nextStations, nextItems, nextMonitors, nextTemplates] = await Promise.all([
        listStations(),
        listKeyPoolItems(),
        listChannelMonitors(),
        listChannelMonitorTemplates(),
      ]);
      setStations(nextStations);
      setItems(nextItems);
      setMonitors(nextMonitors);
      setMonitorTemplates(nextTemplates);
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      toast.error("读取密钥池失败", message);
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
      toast.info("清除筛选后可调整全局顺序");
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
      toast.success("密钥排序已保存");
    } catch (requestError) {
      setItems(previousItems);
      toast.error("保存排序失败", readError(requestError));
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
        groupBindingId: item.groupBindingId,
        groupIdHash: item.groupIdHash,
        rateMultiplier: item.rateMultiplier,
        rateSource: item.rateSource,
        balanceScope: item.balanceScope,
        status: item.status,
        note: item.note,
      });
      await refresh();
      toast.success(item.enabled ? "密钥已禁用" : "密钥已启用");
    } catch (requestError) {
      toast.error("更新密钥状态失败", readError(requestError));
    } finally {
      setSaving(false);
    }
  }

  function handleDelete(item: KeyPoolItem) {
    setPendingDeleteItem(item);
  }

  async function handleConfirmDelete() {
    if (!pendingDeleteItem) {
      return;
    }
    setSaving(true);
    setError(null);
    try {
      await deleteStationKey(pendingDeleteItem.id);
      setPendingDeleteItem(null);
      await refresh();
      toast.success("密钥已删除");
    } catch (requestError) {
      toast.error("删除密钥失败", readError(requestError));
    } finally {
      setSaving(false);
    }
  }

  async function handleTestConnectivity(item: KeyPoolItem) {
    if (!item.apiKeyPresent) {
      toast.error("无法测试连通性", "该密钥没有保存 API Key。");
      return;
    }
    setTestingKeyId(item.id);
    setError(null);
    try {
      const result = await testStationKeyConnectivity(item.id);
      await refresh();
      if (result.ok) {
        toast.success("连通性正常", `${item.name} · ${result.durationMs}ms · ${result.model}`);
      } else {
        toast.error("连通性异常", `${result.statusCode || "网络"} · ${result.message}`);
      }
    } catch (requestError) {
      toast.error("测试连通性失败", readError(requestError));
    } finally {
      setTestingKeyId(null);
    }
  }

  async function handleToggleMonitoring(item: KeyPoolItem) {
    const existingMonitor = findStationKeyMonitor(monitors, item.id);
    const nextEnabled = !existingMonitor?.enabled;
    setMonitoringKeyId(item.id);
    setError(null);
    try {
      if (existingMonitor) {
        await updateChannelMonitor(updateStationKeyMonitorEnabledInput(existingMonitor, nextEnabled));
      } else {
        const template = preferredStationKeyMonitorTemplate(monitorTemplates);
        if (!template) {
          throw new Error("暂无启用的监控请求模板，请先在渠道状态的监控页启用模板。");
        }
        const capabilities = await getStationKeyCapabilities(item.id);
        const preferredTemplate = preferredStationKeyMonitorTemplate(monitorTemplates, {
          stationType: item.stationType,
          stationUpstreamApiFormat: item.stationUpstreamApiFormat,
          capabilities,
        }) ?? template;
        const connectivityResult = await testStationKeyConnectivity(item.id);
        if (!connectivityResult.ok) {
          throw new Error(`连通性测试未通过，未创建监控：${connectivityResult.message}`);
        }
        await createChannelMonitor(createStationKeyMonitorInput(item, preferredTemplate, capabilities, connectivityResult.model));
      }
      await refresh();
      toast.success(nextEnabled ? "监控已开启" : "监控已停用");
    } catch (requestError) {
      toast.error("更新监控开关失败", readError(requestError));
    } finally {
      setMonitoringKeyId(null);
    }
  }

  async function handleEdit(item: KeyPoolItem) {
    if (onEditKey) {
      onEditKey(item.id);
      return;
    }
    setCreatingKey(false);
    setEditingItem(item);
    setEditForm(formFromItem(item));
    setBindingsForEdit([]);
    setSaving(true);
    setError(null);
    try {
      const [capabilities, bindings] = await Promise.all([
        getStationKeyCapabilities(item.id),
        listStationGroupBindings(item.stationId),
      ]);
      setBindingsForEdit(bindings);
      setEditForm((current) => current.id === item.id ? mergeCapabilitiesIntoForm(current, capabilities) : current);
    } catch (requestError) {
      toast.error("读取密钥详情失败", readError(requestError));
    } finally {
      setSaving(false);
    }
  }

  async function handleCreateKey() {
    if (stations.length === 0) {
      toast.info("请先添加中转站");
      return;
    }
    const station = selectedStationId !== "all"
      ? stations.find((item) => item.id === selectedStationId) ?? stations[0]
      : stations[0];
    setEditingItem(null);
    setCreatingKey(true);
    setEditForm(createFormForStation(station, items));
    setBindingsForEdit([]);
    setSaving(true);
    setError(null);
    try {
      const bindings = await listStationGroupBindings(station.id);
      setBindingsForEdit(bindings);
    } catch (requestError) {
      toast.error("读取中转站分组失败", readError(requestError));
    } finally {
      setSaving(false);
    }
  }

  async function handleCreateStationChange(stationId: string) {
    const station = stations.find((item) => item.id === stationId);
    if (!station) {
      return;
    }
    setEditForm((current) => ({
      ...current,
      stationId: station.id,
      stationName: station.name,
      priority: String(items.filter((item) => item.stationId === station.id).length),
      groupBindingId: "",
      groupName: "",
      tierLabel: "",
    }));
    setBindingsForEdit([]);
    setSaving(true);
    try {
      const bindings = await listStationGroupBindings(station.id);
      setBindingsForEdit(bindings);
    } catch (requestError) {
      toast.error("读取中转站分组失败", readError(requestError));
    } finally {
      setSaving(false);
    }
  }

  async function handleCreateSave(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!editForm.stationId) {
      toast.info("请选择中转站");
      return;
    }
    if (!editForm.apiKey.trim()) {
      toast.info("请填写密钥");
      return;
    }
    setSaving(true);
    setError(null);
    try {
      const input: CreateStationKeyInput = {
        stationId: editForm.stationId,
        name: editForm.name.trim(),
        apiKey: editForm.apiKey.trim(),
        enabled: editForm.enabled,
        priority: Number(editForm.priority),
        groupBindingId: editForm.groupBindingId || null,
        groupName: editForm.groupName.trim() ? editForm.groupName.trim() : null,
        tierLabel: editForm.tierLabel.trim() ? editForm.tierLabel.trim() : null,
        note: editForm.note.trim() ? editForm.note.trim() : null,
      };
      const createdKey = await createStationKey(input);
      if (editForm.groupBindingId) {
        await updateStationKeyGroupBinding(createdKey.id, editForm.groupBindingId);
      }
      try {
        await updateStationKeyCapabilities({
          stationKeyId: createdKey.id,
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
        throw new Error(`密钥已创建，但路由能力保存失败：${readError(capabilityError)}`);
      }
      setCreatingKey(false);
      setEditForm(emptyEditForm);
      await refresh();
      toast.success("密钥已添加");
    } catch (requestError) {
      toast.error("添加密钥失败", readError(requestError));
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
        groupBindingId: editingItem.groupBindingId,
        groupIdHash: editingItem.groupIdHash,
        rateMultiplier: editingItem.rateMultiplier,
        rateSource: editingItem.rateSource,
        balanceScope: editingItem.balanceScope,
        status: editForm.status,
        note: editForm.note.trim() ? editForm.note.trim() : null,
      });
      if (editForm.groupBindingId && editForm.groupBindingId !== editingItem.groupBindingId) {
        await updateStationKeyGroupBinding(editForm.id, editForm.groupBindingId);
      }
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
        throw new Error(`密钥基础信息已保存，但路由能力保存失败：${readError(capabilityError)}`);
      }
      setEditingItem(null);
      await refresh();
      toast.success("密钥已更新");
    } catch (requestError) {
      toast.error("保存密钥失败", readError(requestError));
    } finally {
      setSaving(false);
    }
  }

  return (
    <PageScaffold
      title="密钥池"
      status={
        <div className="flex min-w-0 flex-wrap items-center gap-1.5" aria-label="密钥池状态">
          <StatusBadge tone="info" className="bg-slate-50 text-slate-600">
            {`${filteredItems.length} 密钥`}
          </StatusBadge>
          <StatusBadge tone={filteredEnabledCount > 0 ? "healthy" : "disabled"}>
            {`${filteredEnabledCount} 启用`}
          </StatusBadge>
        </div>
      }
      actions={
        <div className="flex items-center gap-2">
          <SelectControl
            ariaLabel="筛选中转站"
            className={selectClassName}
            value={selectedStationId}
            options={[
              { value: "all", label: "全部中转站" },
              ...stationOptions.map((station) => ({ value: station.id, label: station.label })),
            ]}
            onChange={setSelectedStationId}
          />
          <SelectControl
            ariaLabel="筛选启用状态"
            className={selectClassName}
            value={filterMode}
            options={[
              { value: "all", label: "全部状态" },
              { value: "enabled", label: "只看启用" },
              { value: "disabled", label: "只看禁用" },
            ]}
            onChange={setFilterMode}
          />
          <div className="relative">
            <Search className="pointer-events-none absolute left-2.5 top-2 h-4 w-4 text-muted-foreground" />
            <input className={`${selectClassName} pl-8`} value={query} onChange={(event) => setQuery(event.target.value)} placeholder="搜索密钥 / 站点" />
          </div>
          <Button
            variant="secondary"
            onClick={() => {
              if (onAddKey) {
                onAddKey(selectedStationId === "all" ? null : selectedStationId);
                return;
              }
              void handleCreateKey();
            }}
            disabled={loading || saving || stations.length === 0}
          >
            <Plus className="h-4 w-4" />
            新增密钥
          </Button>
        </div>
      }
    >
      {loading ? (
        <div className="rounded-[var(--surface-radius)] border border-cyan-100 bg-white/85 px-4 py-5 text-sm text-muted-foreground">
          正在读取密钥池...
        </div>
      ) : filteredItems.length === 0 ? (
        <EmptyState
          title="还没有可管理的密钥"
          description="先在中转站页创建一个站点和它下面的密钥。"
        />
      ) : (
        <div className="space-y-[var(--shell-page-gap)]">
          <DndContext
            sensors={sensors}
            collisionDetection={closestCenter}
            onDragStart={handleDragStart}
            onDragCancel={handleDragCancel}
            onDragEnd={handleDragEnd}
          >
            <SortableContext items={filteredItems.map((item) => item.id)} strategy={verticalListSortingStrategy}>
              <div className="overflow-x-auto">
                <div className={cn(keyPoolGridClassName, "border-b border-slate-200 px-3 pb-2 text-[11px] font-medium text-slate-500")}>
                  <div aria-hidden />
                  <TableHeadCell>名称</TableHeadCell>
                  <TableHeadCell align="center">状态</TableHeadCell>
                  <TableHeadCell align="center">调度</TableHeadCell>
                  <TableHeadCell align="center">监控</TableHeadCell>
                  <TableHeadCell align="center">分组</TableHeadCell>
                  <div className="text-right">操作</div>
                </div>
                <div className="divide-y divide-slate-100">
                  {filteredItems.map((item) => (
                    <SortableKeyRow
                      key={item.id}
                      item={item}
                      dragEnabled={dragEnabled}
                      onEdit={handleEdit}
                      onDelete={handleDelete}
                      onTestConnectivity={handleTestConnectivity}
                      testing={testingKeyId === item.id}
                      onToggleEnabled={handleToggleEnabled}
                      monitor={monitorByKey.get(item.id) ?? null}
                      monitoring={monitoringKeyId === item.id}
                      onToggleMonitoring={handleToggleMonitoring}
                    />
                  ))}
                </div>
              </div>
            </SortableContext>
            <DragOverlay dropAnimation={null}>
              {activeDragItem ? <KeyRowContent overlay item={activeDragItem} /> : null}
            </DragOverlay>
          </DndContext>
        </div>
      )}

      {(creatingKey || editingItem) && (
        <KeyEditDialog
          actionSaving={saving}
          bindings={bindingsForEdit}
          mode={creatingKey ? "create" : "edit"}
          form={editForm}
          stations={stations}
          onClose={() => {
            setCreatingKey(false);
            setEditingItem(null);
          }}
          onFormChange={setEditForm}
          onSave={creatingKey ? handleCreateSave : handleEditSave}
          onStationChange={creatingKey ? handleCreateStationChange : undefined}
        />
      )}

      <ConfirmDialog
        open={pendingDeleteItem !== null}
        title="删除密钥"
        description={`确定要删除密钥 "${pendingDeleteItem?.name ?? ""}" 吗？此操作无法撤销。`}
        confirming={saving}
        onCancel={() => setPendingDeleteItem(null)}
        onConfirm={() => void handleConfirmDelete()}
      />
    </PageScaffold>
  );
}

function SortableKeyRow({
  item,
  dragEnabled,
  testing,
  monitor,
  monitoring,
  onEdit,
  onTestConnectivity,
  onToggleEnabled,
  onToggleMonitoring,
  onDelete,
}: {
  item: KeyPoolItem;
  dragEnabled: boolean;
  testing: boolean;
  monitor: ChannelMonitor | null;
  monitoring: boolean;
  onEdit: (item: KeyPoolItem) => void;
  onTestConnectivity: (item: KeyPoolItem) => void;
  onToggleEnabled: (item: KeyPoolItem) => void;
  onToggleMonitoring: (item: KeyPoolItem) => void;
  onDelete: (item: KeyPoolItem) => void;
}) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({ id: item.id, disabled: !dragEnabled });
  return (
    <div ref={setNodeRef} style={{ transform: CSS.Transform.toString(transform), transition }} className={cn("will-change-transform", isDragging && "opacity-35")}>
      <KeyRowContent item={item} testing={testing} monitor={monitor} monitoring={monitoring} dragAttributes={dragEnabled ? attributes : undefined} dragListeners={dragEnabled ? listeners : undefined} dragDisabled={!dragEnabled} onEdit={onEdit} onTestConnectivity={onTestConnectivity} onToggleEnabled={onToggleEnabled} onToggleMonitoring={onToggleMonitoring} onDelete={onDelete} />
    </div>
  );
}

function KeyRowContent({
  item,
  overlay = false,
  testing = false,
  monitor,
  monitoring = false,
  dragDisabled = false,
  dragAttributes,
  dragListeners,
  onEdit,
  onTestConnectivity,
  onToggleEnabled,
  onToggleMonitoring,
  onDelete,
}: {
  item: KeyPoolItem;
  overlay?: boolean;
  testing?: boolean;
  monitor?: ChannelMonitor | null;
  monitoring?: boolean;
  dragDisabled?: boolean;
  dragAttributes?: DraggableAttributes;
  dragListeners?: ReturnType<typeof useSortable>["listeners"];
  onEdit?: (item: KeyPoolItem) => void;
  onTestConnectivity?: (item: KeyPoolItem) => void;
  onToggleEnabled?: (item: KeyPoolItem) => void;
  onToggleMonitoring?: (item: KeyPoolItem) => void;
  onDelete?: (item: KeyPoolItem) => void;
}) {
  const cooldownActive = isFutureTime(item.cooldownUntil);
  const badges = compactKeyBadges(item, cooldownActive);
  const status = keyStatusView(item, badges);

  return (
    <div
      className={cn(
        keyPoolGridClassName,
        "group min-h-[66px] px-3 py-2.5 text-left transition-colors hover:bg-slate-50/45",
        overlay && "bg-slate-50",
      )}
    >
      <button
        type="button"
        aria-label="拖拽排序"
        title="拖拽排序"
        tabIndex={dragDisabled ? -1 : 0}
        disabled={dragDisabled}
        className={cn(
          "flex h-7 w-5 shrink-0 items-center justify-center text-slate-300",
          dragDisabled ? "cursor-not-allowed" : "cursor-grab hover:text-slate-500 active:cursor-grabbing",
        )}
        {...dragAttributes}
        {...dragListeners}
      >
        <GripVertical className="h-4 w-4" />
      </button>

      <div className="flex min-w-0 items-center gap-2.5">
        <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-[9px] bg-slate-100 text-slate-600">
          <KeyRound className="h-4 w-4" />
        </div>
        <div className="min-w-0">
          <div className="min-w-0 truncate text-[13px] font-semibold text-slate-900">{item.name}</div>
          <div className="mt-0.5 truncate text-xs text-muted-foreground">{formatStationBaseUrl(item.stationBaseUrl)}</div>
        </div>
      </div>

      <div className="flex min-w-0 justify-center">
        {testing ? (
          <span
            className="inline-flex h-6 min-w-[4.75rem] items-center justify-center gap-1.5 rounded-full border border-teal-200 bg-teal-50 px-2 text-xs font-medium text-teal-700 shadow-[0_1px_0_rgba(15,23,42,0.03)]"
            aria-live="polite"
          >
            <Loader2 className="h-3.5 w-3.5 animate-spin motion-reduce:animate-none" />
            测试中
          </span>
        ) : (
          <StatusBadge tone={status.tone}>{status.label}</StatusBadge>
        )}
      </div>

      <div className="flex min-w-0 items-center justify-center">
        <SwitchControl
          checked={item.enabled}
          ariaLabel={item.enabled ? `关闭调度 ${item.name}` : `打开调度 ${item.name}`}
          className="h-7 w-10 justify-center border-transparent bg-transparent px-0 shadow-none"
          disabled={overlay}
          onCheckedChange={() => onToggleEnabled?.(item)}
          showLabel={false}
        />
      </div>

      <div className="flex min-w-0 items-center justify-center">
        <SwitchControl
          checked={Boolean(monitor?.enabled)}
          ariaLabel={monitor?.enabled ? `关闭监控 ${item.name}` : `打开监控 ${item.name}`}
          className={cn(
            "h-7 w-10 justify-center border-transparent bg-transparent px-0 shadow-none",
            monitoring && "animate-pulse",
          )}
          disabled={overlay || monitoring}
          onCheckedChange={() => onToggleMonitoring?.(item)}
          showLabel={false}
        />
      </div>

      <div className="flex min-w-0 justify-center">
        <span className="inline-flex max-w-full items-center rounded-full bg-emerald-50 px-2 py-1 text-xs font-medium text-emerald-700 ring-1 ring-emerald-100">
          <span className="truncate">{item.stationName}</span>
        </span>
      </div>

      <div
        className="flex shrink-0 items-center justify-end gap-3 md:opacity-0 md:transition-opacity md:group-hover:opacity-100 md:group-focus-within:opacity-100"
        onClick={(event) => event.stopPropagation()}
        onKeyDown={(event) => event.stopPropagation()}
      >
        <IconButton
          className={cn(
            "text-slate-500 hover:bg-teal-50 hover:text-teal-700",
            testing && "animate-pulse text-teal-700",
          )}
          disabled={overlay || testing || !item.apiKeyPresent}
          label={`测试连通性 ${item.name}`}
          onClick={() => onTestConnectivity?.(item)}
        >
          <Activity className="h-4 w-4" />
        </IconButton>
        <IconButton className="text-slate-500 hover:bg-slate-100 hover:text-slate-800" label={`编辑 ${item.name}`} onClick={() => onEdit?.(item)}>
          <Edit3 className="h-4 w-4" />
        </IconButton>
        <IconButton className="text-slate-500 hover:bg-rose-50 hover:text-rose-600" label={`删除 ${item.name}`} onClick={() => onDelete?.(item)}>
          <Trash2 className="h-4 w-4" />
        </IconButton>
      </div>
    </div>
  );
}

function TableHeadCell({
  align = "start",
  children,
}: {
  align?: "start" | "center";
  children: string;
}) {
  return (
    <div
      className={cn(
        "min-w-0 truncate",
        align === "center" && "text-center",
      )}
    >
      {children}
    </div>
  );
}

function keyStatusView(
  item: KeyPoolItem,
  badges: Array<{ label: string; tone: StatusTone }>,
) {
  if (badges[0]) {
    return badges[0];
  }
  if (item.status === "healthy") {
    return { label: "正常", tone: "healthy" as const };
  }
  return { label: "未检测", tone: "info" as const };
}

function compactKeyBadges(item: KeyPoolItem, cooldownActive: boolean) {
  const badges: Array<{ label: string; tone: StatusTone }> = [];
  if (!item.apiKeyPresent) {
    badges.push({ label: "缺少密钥", tone: "error" });
    return badges;
  }
  if (item.status === "disabled" && item.enabled) {
    badges.push({ label: "状态禁用", tone: "disabled" });
    return badges;
  }
  if (item.status === "warning" || item.status === "error") {
    badges.push({ label: statusLabels[item.status], tone: statusTone[item.status] });
    return badges;
  }
  if (cooldownActive) {
    badges.push({ label: "冷却中", tone: "warning" });
    return badges;
  }
  if (item.onlyUseAsBackup) {
    badges.push({ label: "备用", tone: "warning" });
  }
  return badges;
}

type KeyPoolEditForm = {
  id: string;
  stationId: string;
  stationName: string;
  name: string;
  apiKey: string;
  enabled: boolean;
  priority: string;
  groupBindingId: string;
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
  groupBindingId: "",
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
  bindings,
  form,
  mode,
  onClose,
  onFormChange,
  onSave,
  onStationChange,
  stations,
}: {
  actionSaving: boolean;
  bindings: StationGroupBinding[];
  form: KeyPoolEditForm;
  mode: "create" | "edit";
  onClose: () => void;
  onFormChange: (next: KeyPoolEditForm) => void;
  onSave: (event: FormEvent<HTMLFormElement>) => void;
  onStationChange?: (stationId: string) => void;
  stations: Station[];
}) {
  const creating = mode === "create";
  const bindingOptions = bindings
    .filter((binding) => isCollectedStationGroupBinding(binding) || binding.stationKeyId === form.id)
    .map((binding) => ({
      value: binding.id,
      label: bindingOptionLabel(binding),
    }));
  return (
    <Dialog
      open
      title={creating ? "新增密钥" : "编辑密钥"}
      description={creating ? "选择已有中转站并保存一枚可调度密钥。" : "密钥留空则保留旧值。"}
      onClose={onClose}
      footer={
        <div className="flex justify-end gap-2">
          <Button variant="outline" onClick={onClose}>取消</Button>
          <Button type="submit" form="key-pool-edit-form" disabled={actionSaving}>{actionSaving ? "保存中" : "保存"}</Button>
        </div>
      }
    >
      <form id="key-pool-edit-form" className="grid gap-4 p-5" onSubmit={onSave}>
        {creating && (
          <div className="grid gap-2 rounded-[var(--surface-radius)] border border-cyan-100 bg-cyan-50/25 p-3">
            <div className="text-xs font-semibold text-slate-700">预设中转站</div>
            <SelectControl
              ariaLabel="预设中转站"
              className={inputClassName}
              value={form.stationId}
              options={stations.map((station) => ({ value: station.id, label: station.name }))}
              onChange={(stationId) => onStationChange?.(stationId)}
            />
          </div>
        )}
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
        <Field label="密钥">
          <input
            className={inputClassName}
            value={form.apiKey}
            onChange={(event) => onFormChange({ ...form, apiKey: event.target.value })}
            placeholder={creating ? "sk-..." : "留空保留旧密钥"}
            required={creating}
            type="password"
          />
        </Field>
        <div className="grid gap-3 md:grid-cols-3">
          <Field label="分组绑定">
            <SelectControl
              ariaLabel="分组绑定"
              className={inputClassName}
              value={form.groupBindingId}
              options={[
                { value: "", label: bindingOptions.length ? "不调整绑定" : "暂无可用分组" },
                ...bindingOptions,
              ]}
              onChange={(groupBindingId) => {
                const binding = bindings.find((item) => item.id === groupBindingId);
                onFormChange({
                  ...form,
                  groupBindingId,
                  groupName: binding?.groupName ?? form.groupName,
                });
              }}
            />
          </Field>
          <Field label="分组">
            <input className={inputClassName} value={form.groupName} onChange={(event) => onFormChange({ ...form, groupName: event.target.value })} />
          </Field>
          <Field label="档位">
            <input className={inputClassName} value={form.tierLabel} onChange={(event) => onFormChange({ ...form, tierLabel: event.target.value })} />
          </Field>
          <Field label="状态">
            <SelectControl
              ariaLabel="密钥状态"
              className={inputClassName}
              value={form.status}
              options={[
                { value: "unchecked", label: "未检测" },
                { value: "healthy", label: "正常" },
                { value: "warning", label: "警告" },
                { value: "error", label: "错误" },
                { value: "disabled", label: "禁用" },
              ]}
              onChange={(status) => onFormChange({ ...form, status })}
            />
          </Field>
        </div>
        <label className="flex items-center gap-2 text-sm text-slate-700">
          <input checked={form.enabled} className="h-4 w-4 accent-teal-600" type="checkbox" onChange={(event) => onFormChange({ ...form, enabled: event.target.checked })} />
          启用
        </label>
        <div className="grid gap-2 rounded-[var(--surface-radius)] border border-cyan-100 bg-cyan-50/25 p-3">
          <div className="text-xs font-semibold text-slate-700">协议能力</div>
          <div className="grid gap-2 sm:grid-cols-2 md:grid-cols-3">
            <CheckField label="聊天补全" checked={form.supportsChatCompletions} onChange={(checked) => onFormChange({ ...form, supportsChatCompletions: checked })} />
            <CheckField label="响应接口" checked={form.supportsResponses} onChange={(checked) => onFormChange({ ...form, supportsResponses: checked })} />
            <CheckField label="向量接口" checked={form.supportsEmbeddings} onChange={(checked) => onFormChange({ ...form, supportsEmbeddings: checked })} />
            <CheckField label="流式响应" checked={form.supportsStream} onChange={(checked) => onFormChange({ ...form, supportsStream: checked })} />
            <CheckField label="工具调用" checked={form.supportsTools} onChange={(checked) => onFormChange({ ...form, supportsTools: checked })} />
            <CheckField label="图片输入" checked={form.supportsVision} onChange={(checked) => onFormChange({ ...form, supportsVision: checked })} />
            <CheckField label="推理模型" checked={form.supportsReasoning} onChange={(checked) => onFormChange({ ...form, supportsReasoning: checked })} />
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
            仅作为备用密钥
          </label>
          <Field label="路由标签">
            <input className={inputClassName} value={form.routingTags} onChange={(event) => onFormChange({ ...form, routingTags: event.target.value })} placeholder="逗号分隔，例如 高优先级, 低延迟" />
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

function formatRate(value: number | null) {
  if (value === null || !Number.isFinite(value)) {
    return "未知";
  }
  return `${value.toFixed(3).replace(/0+$/, "").replace(/\.$/, "")}x`;
}

function formatStationBaseUrl(value: string) {
  try {
    const url = new URL(value);
    return `${url.protocol}//${url.host}`;
  } catch {
    return value.replace(/\/+$/, "");
  }
}

function bindingOptionLabel(binding: StationGroupBinding) {
  const rate = formatRate(binding.effectiveRateMultiplier);
  const status = binding.bindingStatus === "available" ? "可用" : binding.bindingStatus;
  return `${binding.groupName} · ${rate} · ${status}`;
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
    groupBindingId: item.groupBindingId ?? "",
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

function createFormForStation(station: Station, items: KeyPoolItem[]): KeyPoolEditForm {
  const nextIndex = items.filter((item) => item.stationId === station.id).length;
  return {
    ...emptyEditForm,
    stationId: station.id,
    stationName: station.name,
    name: `${station.name} Key ${nextIndex + 1}`,
    priority: String(nextIndex),
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
