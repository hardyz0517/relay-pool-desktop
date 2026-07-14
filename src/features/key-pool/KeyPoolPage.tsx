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
import { Activity, Bot, Edit3, GripVertical, KeyRound, Loader2, MessageCircle, Plus, RotateCw, Search, Trash2 } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import {
  usePageActivation,
  usePageRefreshEnabled,
} from "@/components/shell/PageActivity";
import { Button, ConfirmDialog, Dialog, EmptyState, IconButton, SelectControl, StatusBadge, SwitchControl, type StatusTone, useToast } from "@/components/ui";
import { createChannelMonitor, listChannelMonitorTemplates, listChannelMonitors, updateChannelMonitor } from "@/lib/api/channelMonitors";
import { listGroupRateRecords, listStationGroupBindings } from "@/lib/api/groupFacts";
import { getStationKeyCapabilities } from "@/lib/api/routing";
import { listStations } from "@/lib/api/stations";
import { KEY_POOL_ITEMS_UPDATED_EVENT, deleteStationKey, listKeyPoolItems, reorderKeyPool, saveStationKeyWithDefaults, testStationKeyConnectivity, updateStationKey } from "@/lib/api/stationKeys";
import { readError } from "@/lib/errors";
import { buildCurrentStationGroupFacts } from "@/lib/projections/groupFacts";
import { keyPoolQueryOptions, stationsQueryOptions } from "@/lib/query/resourceQueries";
import { useActivityQuery } from "@/lib/query/useActivityQuery";
import { parseTimestampLikeDate } from "@/lib/time";
import type { ChannelMonitor, ChannelMonitorRequestTemplate } from "@/lib/types/channelMonitors";
import type { StationGroupOption } from "@/lib/types/groupFacts";
import type { StationKeyCapabilities } from "@/lib/types/routing";
import type { Station } from "@/lib/types/stations";
import type { KeyPoolItem, StationKeyConnectivityTestResult, StationKeyStatus } from "@/lib/types/stationKeys";
import { cn } from "@/lib/utils";
import { StationGroupOptionLabel } from "@/features/stations/components/StationGroupChip";
import {
  buildStationGroupOptionsFromCurrentFactsForSelect,
  findMatchingGroupOption,
} from "@/features/stations/groupOptionViewModels";
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

const KEEP_GROUP_BINDING_VALUE = "__keep__";
const CLEAR_GROUP_BINDING_VALUE = "__clear__";
const DEFAULT_KEY_CONNECTIVITY_TEST_MODEL = "gpt-5.5";

const defaultKeyConnectivityModelOptions = [
  { value: "gpt-5.5", label: "GPT-5.5" },
  { value: "gpt-5.4", label: "GPT-5.4" },
  { value: "gpt-4.1", label: "GPT-4.1" },
];

export function KeyPoolPage({ onAddKey, onEditKey }: KeyPoolPageProps) {
  const toast = useToast();
  const refreshEnabled = usePageRefreshEnabled();
  useActivityQuery(refreshEnabled, keyPoolQueryOptions());
  useActivityQuery(refreshEnabled, stationsQueryOptions());
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
  const [connectivityDialogItem, setConnectivityDialogItem] = useState<KeyPoolItem | null>(null);
  const [connectivityCapabilities, setConnectivityCapabilities] = useState<StationKeyCapabilities | null>(null);
  const [connectivityTestResult, setConnectivityTestResult] = useState<StationKeyConnectivityTestResult | null>(null);
  const [connectivityTestError, setConnectivityTestError] = useState<string | null>(null);
  const [pendingDeleteItem, setPendingDeleteItem] = useState<KeyPoolItem | null>(null);
  const [editForm, setEditForm] = useState<KeyPoolEditForm>(emptyEditForm);
  const [groupOptionsForEdit, setGroupOptionsForEdit] = useState<StationGroupOption[]>([]);
  const [testingKeyId, setTestingKeyId] = useState<string | null>(null);
  const [monitoringKeyId, setMonitoringKeyId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const sensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 8 } }));
  const activeDragItem = useMemo(
    () => items.find((item) => item.id === activeDragId) ?? null,
    [activeDragId, items],
  );

  usePageActivation(({ isInitial }) => {
    void refresh(isInitial);
  });

  useEffect(() => {
    function handleKeyPoolItemsUpdated() {
      if (!refreshEnabled) {
        return;
      }
      void refresh(false);
    }

    window.addEventListener(KEY_POOL_ITEMS_UPDATED_EVENT, handleKeyPoolItemsUpdated);
    return () => {
      window.removeEventListener(KEY_POOL_ITEMS_UPDATED_EVENT, handleKeyPoolItemsUpdated);
    };
  }, [refreshEnabled]);

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

  async function refresh(showLoading = true) {
    if (showLoading) {
      setLoading(true);
    }
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
      if (showLoading) {
        setLoading(false);
      }
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

  function handleTestConnectivity(item: KeyPoolItem) {
    if (!item.apiKeyPresent) {
      toast.error("无法测试连通性", "该密钥没有保存 API Key。");
      return;
    }
    setConnectivityDialogItem(item);
    setConnectivityCapabilities(null);
    setConnectivityTestResult(null);
    setConnectivityTestError(null);
    void loadConnectivityCapabilities(item);
  }

  async function loadConnectivityCapabilities(item: KeyPoolItem) {
    try {
      const capabilities = await getStationKeyCapabilities(item.id);
      setConnectivityCapabilities(capabilities);
    } catch (requestError) {
      toast.info("读取模型范围失败，先使用默认 GPT 测试模型", readError(requestError));
    }
  }

  async function handleRunConnectivityTest(model: string) {
    if (!connectivityDialogItem) {
      return;
    }
    const item = connectivityDialogItem;
    setTestingKeyId(item.id);
    setError(null);
    setConnectivityTestError(null);
    setConnectivityTestResult(null);
    try {
      const result = await testStationKeyConnectivity(item.id, model);
      setConnectivityTestResult(result);
      await refresh();
      if (result.ok) {
        toast.success("连通性正常", `${item.name} · ${result.durationMs}ms · ${result.model}`);
      } else {
        toast.error("连通性异常", `${result.statusCode || "网络"} · ${result.message}`);
      }
    } catch (requestError) {
      const message = readError(requestError);
      setConnectivityTestError(message);
      toast.error("测试连通性失败", message);
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
        const connectivityResult = await testStationKeyConnectivity(item.id, DEFAULT_KEY_CONNECTIVITY_TEST_MODEL);
        const monitorModel = connectivityResult.ok ? connectivityResult.model : null;
        await createChannelMonitor(createStationKeyMonitorInput(item, preferredTemplate, capabilities, monitorModel));
        if (!connectivityResult.ok) {
          toast.info("即时连通性未通过，已创建定时监控", connectivityResult.message);
        }
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
    setGroupOptionsForEdit([]);
    setSaving(true);
    setError(null);
    try {
      const [capabilities, groupOptions] = await Promise.all([
        getStationKeyCapabilities(item.id),
        loadCurrentStationGroupOptions(item.stationId),
      ]);
      setGroupOptionsForEdit(groupOptions);
      setEditForm((current) =>
        current.id === item.id
          ? mergeCapabilitiesIntoForm(formFromItem(item, groupOptions), capabilities)
          : current,
      );
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
    setGroupOptionsForEdit([]);
    setSaving(true);
    setError(null);
    try {
      const groupOptions = await loadCurrentStationGroupOptions(station.id);
      setGroupOptionsForEdit(groupOptions);
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
    setGroupOptionsForEdit([]);
    setSaving(true);
    try {
      const groupOptions = await loadCurrentStationGroupOptions(station.id);
      setGroupOptionsForEdit(groupOptions);
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
      await saveStationKeyWithDefaults({
        mode: "create",
        stationId: editForm.stationId,
        name: editForm.name.trim(),
        apiKey: editForm.apiKey.trim(),
        enabled: editForm.enabled,
        priority: Number(editForm.priority),
        tierLabel: editForm.tierLabel.trim() ? editForm.tierLabel.trim() : null,
        note: editForm.note.trim() ? editForm.note.trim() : null,
        groupSelection: groupSelectionFromCreateForm(editForm, groupOptionsForEdit),
        capabilities: capabilitiesFromEditForm(editForm),
      });
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
      await saveStationKeyWithDefaults({
        mode: "update",
        id: editForm.id,
        stationId: editForm.stationId,
        name: editForm.name.trim(),
        apiKey: editForm.apiKey.trim() ? editForm.apiKey.trim() : null,
        enabled: editForm.enabled,
        priority: Number(editForm.priority),
        tierLabel: editForm.tierLabel.trim() ? editForm.tierLabel.trim() : null,
        balanceScope: editingItem.balanceScope,
        status: editForm.status,
        note: editForm.note.trim() ? editForm.note.trim() : null,
        groupSelection: groupSelectionFromEditForm(editForm, editingItem, groupOptionsForEdit),
        capabilities: capabilitiesFromEditForm(editForm),
      });
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
          <StatusBadge tone="info" className="bg-surface-subtle text-muted-foreground">
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
        <div className="rounded-[var(--surface-radius)] border border-info-border bg-surface/85 px-4 py-5 text-sm text-muted-foreground">
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
                <div className={cn(keyPoolGridClassName, "border-b border-border px-3 pb-2 text-[11px] font-medium text-muted-foreground")}>
                  <div aria-hidden />
                  <TableHeadCell>名称</TableHeadCell>
                  <TableHeadCell align="center">状态</TableHeadCell>
                  <TableHeadCell align="center">调度</TableHeadCell>
                  <TableHeadCell align="center">监控</TableHeadCell>
                  <TableHeadCell align="center">分组</TableHeadCell>
                  <div className="text-right">操作</div>
                </div>
                <div className="divide-y divide-border">
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
          groupOptions={groupOptionsForEdit}
          sourceItem={editingItem}
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

      <KeyConnectivityTestDialog
        item={connectivityDialogItem}
        capabilities={connectivityCapabilities}
        result={connectivityTestResult}
        error={connectivityTestError}
        testing={Boolean(connectivityDialogItem && testingKeyId === connectivityDialogItem.id)}
        onClose={() => {
          setConnectivityDialogItem(null);
          setConnectivityCapabilities(null);
          setConnectivityTestResult(null);
          setConnectivityTestError(null);
        }}
        onTest={(model) => void handleRunConnectivityTest(model)}
      />

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

function KeyConnectivityTestDialog({
  item,
  capabilities,
  result,
  error,
  testing,
  onClose,
  onTest,
}: {
  item: KeyPoolItem | null;
  capabilities: StationKeyCapabilities | null;
  result: StationKeyConnectivityTestResult | null;
  error: string | null;
  testing: boolean;
  onClose: () => void;
  onTest: (model: string) => void;
}) {
  const [model, setModel] = useState(DEFAULT_KEY_CONNECTIVITY_TEST_MODEL);
  const [displayedResponseText, setDisplayedResponseText] = useState("");
  const open = item !== null;
  const modelOptions = useMemo(
    () => buildKeyConnectivityModelOptions(capabilities),
    [capabilities],
  );
  const completed = Boolean(result || error);
  const selectedModelLabel = modelOptions.find((option) => option.value === model)?.label ?? model;
  const fullResponseText = result
    ? result.ok
      ? result.message || `Hi! What can I help you with? (${formatConnectivityDuration(result.durationMs)})`
      : `${result.statusCode || "网络"} · ${result.message}`
    : error ?? "";
  const responseTypingComplete = completed && displayedResponseText === fullResponseText;

  useEffect(() => {
    if (open) {
      setModel(modelOptions[0]?.value ?? DEFAULT_KEY_CONNECTIVITY_TEST_MODEL);
      setDisplayedResponseText("");
    }
  }, [modelOptions, open, item?.id]);

  useEffect(() => {
    setDisplayedResponseText(fullResponseText);
  }, [fullResponseText]);

  return (
    <Dialog
      open={open}
      title="测试密钥连接"
      className="max-w-[460px] rounded-[14px]"
      onClose={onClose}
      footer={
        <div className="flex items-center justify-end gap-3">
          <Button variant="ghost" className="bg-muted text-foreground hover:bg-hover" onClick={onClose}>
            关闭
          </Button>
          <Button
            className={cn(
              "min-w-[74px] bg-primary-solid hover:bg-primary-solid",
              testing && "bg-primary-solid hover:bg-primary-solid",
            )}
            disabled={!item || testing}
            onClick={() => onTest(model)}
          >
            {testing ? <Loader2 className="h-3.5 w-3.5 animate-spin motion-reduce:animate-none" /> : <RotateCw className="h-3.5 w-3.5" />}
            {testing ? "测试中..." : completed ? "重试" : "测试模型"}
          </Button>
        </div>
      }
    >
      <div data-testid="key-connectivity-test-dialog" className="space-y-4 px-5 py-4">
        <div className="flex items-center justify-between gap-3 rounded-[10px] border border-border bg-surface-subtle p-3">
          <div className="flex min-w-0 items-center gap-3">
            <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-[8px] bg-primary-solid text-primary-foreground shadow-surface">
              <Activity className="h-4 w-4" />
            </div>
            <div className="min-w-0">
              <div className="truncate text-[13px] font-semibold text-foreground">{item?.name ?? "密钥"}</div>
              <div className="mt-1 flex items-center gap-1.5 text-[11px] text-muted-foreground">
                <span className="rounded bg-hover px-1.5 py-0.5 text-[10px] font-semibold uppercase text-muted-foreground">APIKEY</span>
                <span>密钥</span>
              </div>
            </div>
          </div>
          <span className="rounded-full bg-success-surface px-2.5 py-1 text-[11px] font-semibold text-success-foreground">
            {item?.enabled ? "active" : "inactive"}
          </span>
        </div>

        <Field label="选择测试模型">
          <SelectControl
            value={model}
            options={modelOptions}
            ariaLabel="选择测试模型"
            className="h-9 w-full rounded-[10px] border-border bg-surface text-[13px]"
            menuClassName="text-[13px]"
            disabled={testing}
            onChange={setModel}
          />
        </Field>

        <div className="rounded-[10px] bg-surface-inset p-4 font-mono text-[12px] leading-5 text-muted-foreground/60 shadow-inner">
          {buildConnectivityConsoleLines({
            item,
            model,
            selectedModelLabel,
            testing,
            result,
            error,
            displayedResponseText,
            responseTypingComplete,
          }).map((line, index) => (
            <div key={`${line.text}-${index}`} className={line.className}>
              {testing && index === 0 ? (
                <span className="inline-flex items-center gap-1.5">
                  <Loader2
                    data-testid="key-connectivity-console-spinner"
                    className="h-3.5 w-3.5 animate-spin motion-reduce:animate-none"
                  />
                  {line.text}
                </span>
              ) : (
                line.text
              )}
            </div>
          ))}
        </div>

        <div className="flex items-center justify-between text-[11px] text-muted-foreground">
          <span className="inline-flex items-center gap-1.5">
            <Bot className="h-3.5 w-3.5" />
            测试模型
          </span>
          <span className="inline-flex items-center gap-1.5">
            <MessageCircle className="h-3.5 w-3.5" />
            提示词："hi"
          </span>
        </div>
      </div>
    </Dialog>
  );
}

function buildConnectivityConsoleLines({
  item,
  model,
  selectedModelLabel,
  testing,
  result,
  error,
  displayedResponseText,
  responseTypingComplete,
}: {
  item: KeyPoolItem | null;
  model: string;
  selectedModelLabel: string;
  testing: boolean;
  result: StationKeyConnectivityTestResult | null;
  error: string | null;
  displayedResponseText: string;
  responseTypingComplete: boolean;
}) {
  const lines = [
    { text: testing ? "连接 API 中..." : `开始测试密钥：${item?.name ?? "密钥"}`, className: "text-info-foreground" },
    { text: `使用模型：${selectedModelLabel}`, className: "font-semibold text-info-foreground" },
    { text: '发送测试消息："hi"', className: "text-muted-foreground/60" },
    { text: "响应：", className: "font-semibold text-warning-foreground" },
  ];

  if (testing) {
    return lines;
  }
  if (result) {
    return [
      ...lines,
      {
        text: displayedResponseText,
        className: result.ok ? "font-semibold text-success-foreground" : "font-semibold text-danger-foreground",
      },
      ...(responseTypingComplete
        ? [
            {
              text: result.ok ? "测试完成！" : "测试未通过。",
              className: result.ok
                ? "mt-2 border-t border-border-strong pt-2 text-success-foreground"
                : "mt-2 border-t border-border-strong pt-2 text-danger-foreground",
            },
          ]
        : []),
    ];
  }
  if (error) {
    return [
      ...lines,
      { text: displayedResponseText, className: "font-semibold text-danger-foreground" },
      ...(responseTypingComplete
        ? [{ text: "测试失败。", className: "mt-2 border-t border-border-strong pt-2 text-danger-foreground" }]
        : []),
    ];
  }
  return [...lines, { text: `待测试模型 ${model}`, className: "text-muted-foreground/70" }];
}

function formatConnectivityDuration(durationMs: number) {
  return durationMs > 0 ? `${durationMs}ms` : "预览模式";
}

function buildKeyConnectivityModelOptions(capabilities: StationKeyCapabilities | null) {
  const scopedModels =
    capabilities?.modelAllowlist.length
      ? capabilities.modelAllowlist
      : capabilities?.preferredModels.length
        ? capabilities.preferredModels
        : [];
  const sourceModels = scopedModels.length > 0
    ? scopedModels
    : defaultKeyConnectivityModelOptions.map((option) => option.value);
  const seen = new Set<string>();
  return sourceModels.flatMap((model) => {
    const trimmed = model.trim();
    if (!trimmed) {
      return [];
    }
    const normalized = trimmed.toLowerCase();
    if (seen.has(normalized)) {
      return [];
    }
    seen.add(normalized);
    return [{ value: trimmed, label: formatConnectivityModelLabel(trimmed) }];
  });
}

function formatConnectivityModelLabel(model: string) {
  return defaultKeyConnectivityModelOptions.find((option) => option.value === model)?.label ?? model;
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
        "group min-h-[66px] px-3 py-2.5 text-left transition-colors hover:bg-surface-subtle",
        overlay && "bg-surface-subtle",
      )}
    >
      <button
        type="button"
        aria-label="拖拽排序"
        title="拖拽排序"
        tabIndex={dragDisabled ? -1 : 0}
        disabled={dragDisabled}
        className={cn(
          "flex h-7 w-5 shrink-0 items-center justify-center text-muted-foreground/60",
          dragDisabled ? "cursor-not-allowed" : "cursor-grab hover:text-muted-foreground active:cursor-grabbing",
        )}
        {...dragAttributes}
        {...dragListeners}
      >
        <GripVertical className="h-4 w-4" />
      </button>

      <div className="flex min-w-0 items-center gap-2.5">
        <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-[9px] bg-muted text-muted-foreground">
          <KeyRound className="h-4 w-4" />
        </div>
        <div className="min-w-0">
          <div className="min-w-0 truncate text-[13px] font-semibold text-foreground">{item.name}</div>
          <div className="mt-0.5 truncate text-xs text-muted-foreground">{formatStationBaseUrl(item.stationBaseUrl)}</div>
        </div>
      </div>

      <div className="flex min-w-0 justify-center">
        {testing ? (
          <span
            className="inline-flex h-6 min-w-[4.75rem] items-center justify-center gap-1.5 rounded-full border border-primary bg-selected px-2 text-xs font-medium text-primary shadow-surface"
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
        <span className="inline-flex max-w-full items-center rounded-full bg-success-surface px-2 py-1 text-xs font-medium text-success-foreground ring-1 ring-success-border/40">
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
            "text-muted-foreground hover:bg-selected hover:text-primary",
            testing && "animate-pulse text-primary",
          )}
          disabled={overlay || testing || !item.apiKeyPresent}
          label={`测试连通性 ${item.name}`}
          onClick={() => onTestConnectivity?.(item)}
        >
          <Activity className="h-4 w-4" />
        </IconButton>
        <IconButton className="text-muted-foreground hover:bg-muted hover:text-foreground" label={`编辑 ${item.name}`} onClick={() => onEdit?.(item)}>
          <Edit3 className="h-4 w-4" />
        </IconButton>
        <IconButton className="text-muted-foreground hover:bg-danger-surface hover:text-danger-foreground" label={`删除 ${item.name}`} onClick={() => onDelete?.(item)}>
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
  form,
  groupOptions,
  mode,
  onClose,
  onFormChange,
  onSave,
  onStationChange,
  sourceItem,
  stations,
}: {
  actionSaving: boolean;
  form: KeyPoolEditForm;
  groupOptions: StationGroupOption[];
  mode: "create" | "edit";
  onClose: () => void;
  onFormChange: (next: KeyPoolEditForm) => void;
  onSave: (event: FormEvent<HTMLFormElement>) => void;
  onStationChange?: (stationId: string) => void;
  sourceItem: KeyPoolItem | null;
  stations: Station[];
}) {
  const creating = mode === "create";
  const bindingOptions = [
    ...groupOptions
      .filter((option) => option.groupBindingId)
      .map((option) => ({
        value: option.groupBindingId ?? option.value,
        label: groupOptionLabel(option),
      })),
    ...currentGroupOption(sourceItem, groupOptions),
  ];
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
          <div className="grid gap-2 rounded-[var(--surface-radius)] border border-info-border bg-info-surface p-3">
            <div className="text-xs font-semibold text-foreground">预设中转站</div>
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
          <Field label="分组">
            <SelectControl
              ariaLabel="分组"
              className={inputClassName}
              value={form.groupBindingId}
              options={[
                ...(creating
                  ? [{ value: "", label: bindingOptions.length ? "不绑定分组" : "暂无可用分组" }]
                  : [
                      { value: KEEP_GROUP_BINDING_VALUE, label: "不调整绑定" },
                      ...(sourceItem?.groupBindingId ? [{ value: CLEAR_GROUP_BINDING_VALUE, label: "清除绑定" }] : []),
                    ]),
                ...bindingOptions,
              ]}
              onChange={(groupBindingId) => {
                onFormChange({
                  ...form,
                  groupBindingId,
                  groupName: groupNameForDialogSelection(groupBindingId, sourceItem, groupOptions, form.groupName),
                });
              }}
            />
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
        <label className="flex items-center gap-2 text-sm text-foreground">
          <input checked={form.enabled} className="h-4 w-4 accent-primary" type="checkbox" onChange={(event) => onFormChange({ ...form, enabled: event.target.checked })} />
          启用
        </label>
        <div className="grid gap-2 rounded-[var(--surface-radius)] border border-info-border bg-info-surface p-3">
          <div className="text-xs font-semibold text-foreground">协议能力</div>
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
          <label className="flex items-center gap-2 text-sm text-foreground">
            <input checked={form.onlyUseAsBackup} className="h-4 w-4 accent-primary" type="checkbox" onChange={(event) => onFormChange({ ...form, onlyUseAsBackup: event.target.checked })} />
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

function formatStationBaseUrl(value: string) {
  try {
    const url = new URL(value);
    return `${url.protocol}//${url.host}`;
  } catch {
    return value.replace(/\/+$/, "");
  }
}

function groupSelectionFromCreateForm(form: KeyPoolEditForm, options: StationGroupOption[]) {
  const groupOption = selectedGroupOption(options, form.groupBindingId);
  if (!groupOption?.groupBindingId) {
    return { kind: "clear" as const };
  }
  return {
    kind: "set" as const,
    groupBindingId: groupOption.groupBindingId,
    groupIdHash: groupOption.groupIdHash,
    groupName: groupOption.groupName,
  };
}

function groupSelectionFromEditForm(
  form: KeyPoolEditForm,
  sourceItem: KeyPoolItem,
  options: StationGroupOption[],
) {
  if (
    !form.groupBindingId ||
    form.groupBindingId === KEEP_GROUP_BINDING_VALUE ||
    form.groupBindingId === sourceItem.groupBindingId
  ) {
    return { kind: "keep" as const };
  }
  if (form.groupBindingId === CLEAR_GROUP_BINDING_VALUE) {
    return { kind: "clear" as const };
  }
  const groupOption = selectedGroupOption(options, form.groupBindingId);
  return {
    kind: "set" as const,
    groupBindingId: groupOption?.groupBindingId ?? form.groupBindingId,
    groupIdHash: groupOption?.groupIdHash ?? null,
    groupName: groupOption?.groupName ?? null,
  };
}

function capabilitiesFromEditForm(form: KeyPoolEditForm) {
  return {
    stationKeyId: form.id,
    supportsChatCompletions: form.supportsChatCompletions,
    supportsResponses: form.supportsResponses,
    supportsEmbeddings: form.supportsEmbeddings,
    supportsStream: form.supportsStream,
    supportsTools: form.supportsTools,
    supportsVision: form.supportsVision,
    supportsReasoning: form.supportsReasoning,
    modelAllowlist: linesToList(form.modelAllowlist),
    modelBlocklist: linesToList(form.modelBlocklist),
    preferredModels: linesToList(form.preferredModels),
    onlyUseAsBackup: form.onlyUseAsBackup,
    routingTags: commaListToList(form.routingTags),
  };
}

function selectedGroupOption(options: StationGroupOption[], value: string) {
  return options.find((option) => option.groupBindingId === value || option.value === value) ?? null;
}

async function loadCurrentStationGroupOptions(stationId: string) {
  const [bindings, rates] = await Promise.all([
    listStationGroupBindings(stationId),
    listGroupRateRecords(stationId),
  ]);
  return buildStationGroupOptionsFromCurrentFactsForSelect(
    buildCurrentStationGroupFacts({ bindings, rates }),
  );
}

function currentGroupOption(sourceItem: KeyPoolItem | null, options: StationGroupOption[]) {
  if (!sourceItem?.groupBindingId || findMatchingGroupOption(keyPoolItemGroupRow(sourceItem), options)) {
    return [];
  }
  return [
    {
      value: sourceItem.groupBindingId,
      label: <StationGroupOptionLabel option={keyPoolItemGroupOption(sourceItem)} suffix="当前" />,
    },
  ];
}

function groupNameForDialogSelection(
  value: string,
  sourceItem: KeyPoolItem | null,
  options: StationGroupOption[],
  fallback: string,
) {
  if (!value) {
    return "";
  }
  if (value === KEEP_GROUP_BINDING_VALUE) {
    return sourceItem?.groupName ?? fallback;
  }
  if (value === CLEAR_GROUP_BINDING_VALUE) {
    return "";
  }
  if (value === sourceItem?.groupBindingId) {
    return sourceItem.groupName ?? fallback;
  }
  return selectedGroupOption(options, value)?.groupName ?? fallback;
}

function groupOptionLabel(option: StationGroupOption) {
  return <StationGroupOptionLabel option={option} />;
}

function keyPoolItemGroupOption(item: KeyPoolItem) {
  return {
    groupName: item.groupName ?? "当前绑定",
    rateMultiplier: item.rateMultiplier,
  };
}

function keyPoolItemGroupRow(item: KeyPoolItem) {
  return {
    groupBindingId: item.groupBindingId,
    groupIdHash: item.groupIdHash,
    groupName: item.groupName ?? "",
  };
}

const selectClassName =
  "h-8 rounded-[12px] border border-info-border bg-info-surface px-3 text-sm text-foreground outline-none transition focus:border-ring focus:bg-surface focus:ring-2 focus:ring-ring/20";

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
    <label className="flex items-center gap-2 text-sm text-foreground">
      <input
        checked={checked}
        className="h-4 w-4 accent-primary"
        type="checkbox"
        onChange={(event) => onChange(event.target.checked)}
      />
      {label}
    </label>
  );
}

function formFromItem(item: KeyPoolItem, options: StationGroupOption[] = []): KeyPoolEditForm {
  return {
    id: item.id,
    stationId: item.stationId,
    stationName: item.stationName,
    name: item.name,
    apiKey: "",
    enabled: item.enabled,
    priority: String(item.priority),
    groupBindingId: groupBindingValueFromItem(item, options),
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

function groupBindingValueFromItem(item: KeyPoolItem, options: StationGroupOption[]) {
  const option = findMatchingGroupOption(
    {
      groupBindingId: item.groupBindingId,
      groupIdHash: item.groupIdHash,
      groupName: item.groupName ?? "",
    },
    options,
  );
  return option?.groupBindingId ?? item.groupBindingId ?? KEEP_GROUP_BINDING_VALUE;
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
  const date = parseTimestampLikeDate(value);
  return !Number.isNaN(date.getTime()) && date.getTime() > Date.now();
}

const inputClassName =
  "h-8 rounded-[12px] border border-info-border bg-info-surface px-3 text-sm text-foreground outline-none transition focus:border-ring focus:bg-surface focus:ring-2 focus:ring-ring/20";
