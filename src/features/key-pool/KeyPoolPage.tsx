import { useMemo, useState, type FormEvent } from "react";
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
import { SortableContext, verticalListSortingStrategy } from "@dnd-kit/sortable";
import { useQueryClient } from "@tanstack/react-query";
import { Plus, Search } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import { Button, ConfirmDialog, EmptyState, SelectControl, StatusBadge, useToast } from "@/components/ui";
import { createChannelMonitor, updateChannelMonitor } from "@/lib/api/channelMonitors";
import { listGroupRateRecords, listStationGroupBindings } from "@/lib/api/groupFacts";
import { getStationKeyCapabilities } from "@/lib/api/routing";
import { deleteStationKey, reorderKeyPool, saveStationKeyWithDefaults, testStationKeyConnectivity, updateStationKey } from "@/lib/api/stationKeys";
import { readError } from "@/lib/errors";
import { buildCurrentStationGroupFacts } from "@/lib/projections/groupFacts";
import { queryKeys } from "@/lib/query/queryKeys";
import { channelMonitoringQueryOptions, keyPoolQueryOptions, stationsQueryOptions } from "@/lib/query/resourceQueries";
import { useActivityQuery } from "@/lib/query/useActivityQuery";
import type { StationGroupOption } from "@/lib/types/groupFacts";
import type { StationKeyCapabilities } from "@/lib/types/routing";
import type { KeyPoolItem, StationKeyConnectivityTestEvent, StationKeyConnectivityTestResult } from "@/lib/types/stationKeys";
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
import { ConnectivityOperationCancelledError } from "./connectivityOperationController";
import { DEFAULT_KEY_CONNECTIVITY_TEST_MODEL, KeyConnectivityTestDialog } from "./KeyConnectivityTestDialog";
import { KeyEditDialog } from "./KeyEditDialog";
import {
  capabilitiesFromEditForm,
  createFormForStation,
  emptyEditForm,
  formFromItem,
  groupSelectionFromCreateForm,
  groupSelectionFromEditForm,
  mergeCapabilitiesIntoForm,
  type KeyPoolEditForm,
} from "./KeyPoolFormModel";
import { KeyRowContent, SortableKeyRow, TableHeadCell, keyPoolGridClassName } from "./KeyPoolRows";
import { useConnectivityOperation } from "./useConnectivityOperation";

type FilterMode = "all" | "enabled" | "disabled";

type KeyPoolPageProps = {
  onAddKey?: (stationId: string | null) => void;
  onEditKey?: (stationKeyId: string) => void;
};

export function KeyPoolPage({ onAddKey, onEditKey }: KeyPoolPageProps) {
  const toast = useToast();
  const queryClient = useQueryClient();
  const keyPoolItemsQuery = useActivityQuery(keyPoolQueryOptions());
  const stationsQuery = useActivityQuery(stationsQueryOptions());
  const channelMonitoringQuery = useActivityQuery(channelMonitoringQueryOptions());
  const connectivityOperation = useConnectivityOperation();
  const stations = stationsQuery.data ?? [];
  const items = keyPoolItemsQuery.data ?? [];
  const monitorSummaries = channelMonitoringQuery.data?.monitorSummaries ?? [];
  const monitors = useMemo(
    () => monitorSummaries.map((summary) => summary.monitor),
    [monitorSummaries],
  );
  const monitorTemplates = channelMonitoringQuery.data?.templates ?? [];
  const [selectedStationId, setSelectedStationId] = useState<string>("all");
  const [filterMode, setFilterMode] = useState<FilterMode>("all");
  const [query, setQuery] = useState("");
  const [saving, setSaving] = useState(false);
  const [activeDragId, setActiveDragId] = useState<string | null>(null);
  const [creatingKey, setCreatingKey] = useState(false);
  const [editingItem, setEditingItem] = useState<KeyPoolItem | null>(null);
  const [connectivityDialogItem, setConnectivityDialogItem] = useState<KeyPoolItem | null>(null);
  const [connectivityCapabilities, setConnectivityCapabilities] = useState<StationKeyCapabilities | null>(null);
  const [connectivityTestResult, setConnectivityTestResult] = useState<StationKeyConnectivityTestResult | null>(null);
  const [connectivityTestError, setConnectivityTestError] = useState<string | null>(null);
  const [displayedResponseText, setDisplayedResponseText] = useState("");
  const [connectivityStreamFallbackReason, setConnectivityStreamFallbackReason] = useState<string | null>(null);
  const [connectivityProgressLabel, setConnectivityProgressLabel] = useState<string | null>(null);
  const [pendingDeleteItem, setPendingDeleteItem] = useState<KeyPoolItem | null>(null);
  const [editForm, setEditForm] = useState<KeyPoolEditForm>(emptyEditForm);
  const [groupOptionsForEdit, setGroupOptionsForEdit] = useState<StationGroupOption[]>([]);
  const [testingKeyId, setTestingKeyId] = useState<string | null>(null);
  const [monitoringKeyId, setMonitoringKeyId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const queryError = keyPoolItemsQuery.error ?? stationsQuery.error ?? channelMonitoringQuery.error;
  const displayError = error ?? (queryError ? readError(queryError) : null);
  const loading =
    keyPoolItemsQuery.isLoading ||
    stationsQuery.isLoading ||
    (channelMonitoringQuery.isPending && channelMonitoringQuery.data === undefined);

  const sensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 8 } }));
  const activeDragItem = useMemo(
    () => items.find((item) => item.id === activeDragId) ?? null,
    [activeDragId, items],
  );

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
        const text = `${item.name} ${item.stationApiBaseUrl} ${item.stationName} ${item.groupName ?? ""} ${item.tierLabel ?? ""}`.toLowerCase();
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

  async function invalidateKeyPoolQueries(includeStations = true) {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: queryKeys.keyPool }),
      ...(includeStations
        ? [queryClient.invalidateQueries({ queryKey: queryKeys.stations })]
        : []),
    ]);
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
    await queryClient.cancelQueries({ queryKey: queryKeys.keyPool });
    queryClient.setQueryData(queryKeys.keyPool, nextOrder);
    setSaving(true);
    try {
      const saved = await reorderKeyPool(nextOrder.map((item) => item.id));
      queryClient.setQueryData(queryKeys.keyPool, saved);
      await invalidateKeyPoolQueries(false);
      toast.success("密钥排序已保存");
    } catch (requestError) {
      queryClient.setQueryData(queryKeys.keyPool, previousItems);
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
        schedulable: item.schedulable,
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
      await invalidateKeyPoolQueries(false);
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
      await invalidateKeyPoolQueries();
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
    setDisplayedResponseText("");
    setConnectivityStreamFallbackReason(null);
    setConnectivityProgressLabel(null);
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
    setDisplayedResponseText("");
    setConnectivityStreamFallbackReason(null);
    setConnectivityProgressLabel("正在请求流式响应...");
    const handleConnectivityEvent = (event: StationKeyConnectivityTestEvent) => {
      if (event.type === "attemptStarted") {
        setDisplayedResponseText("");
        setConnectivityStreamFallbackReason(null);
        setConnectivityProgressLabel(`流式请求：${event.protocol} / ${event.model}`);
        return;
      }
      if (event.type === "delta") {
        setDisplayedResponseText((current) => current + event.text);
        return;
      }
      if (event.type === "fallback") {
        setDisplayedResponseText("");
        setConnectivityStreamFallbackReason(event.reason);
        setConnectivityProgressLabel("流式未完成，正在改用非流式重试...");
      }
    };
    try {
      const result = await connectivityOperation.run(
        { stationKeyId: item.id, model },
        { onEvent: handleConnectivityEvent },
      );
      setConnectivityTestResult(result);
      setConnectivityProgressLabel(null);
      await invalidateKeyPoolQueries(false);
      if (result.ok) {
        toast.success("连通性正常", `${item.name} · ${result.durationMs}ms · ${result.model}`);
      } else {
        toast.error("连通性异常", `${result.statusCode || "网络"} · ${result.message}`);
      }
    } catch (requestError) {
      if (requestError instanceof ConnectivityOperationCancelledError) {
        return;
      }
      const message = readError(requestError);
      setConnectivityTestError(message);
      setConnectivityProgressLabel(null);
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
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: queryKeys.channelMonitoring }),
        queryClient.invalidateQueries({ queryKey: queryKeys.channelStatus }),
      ]);
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
        schedulable: editForm.schedulable,
        priority: Number(editForm.priority),
        tierLabel: editForm.tierLabel.trim() ? editForm.tierLabel.trim() : null,
        note: editForm.note.trim() ? editForm.note.trim() : null,
        groupSelection: groupSelectionFromCreateForm(editForm, groupOptionsForEdit),
        capabilities: capabilitiesFromEditForm(editForm),
      });
      setCreatingKey(false);
      setEditForm(emptyEditForm);
      await invalidateKeyPoolQueries();
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
        schedulable: editForm.schedulable,
        priority: Number(editForm.priority),
        tierLabel: editForm.tierLabel.trim() ? editForm.tierLabel.trim() : null,
        balanceScope: editingItem.balanceScope,
        status: editForm.status,
        note: editForm.note.trim() ? editForm.note.trim() : null,
        groupSelection: groupSelectionFromEditForm(editForm, editingItem, groupOptionsForEdit),
        capabilities: capabilitiesFromEditForm(editForm),
      });
      setEditingItem(null);
      await invalidateKeyPoolQueries();
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
      {displayError && (
        <div className="mb-3 rounded-[var(--surface-radius)] border border-danger-border bg-danger-surface px-3 py-2 text-sm text-danger-foreground">
          {displayError}
        </div>
      )}
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
          renderCurrentGroupOption={currentGroupOption}
          renderGroupOptionLabel={groupOptionLabel}
        />
      )}

      <KeyConnectivityTestDialog
        item={connectivityDialogItem}
        capabilities={connectivityCapabilities}
        result={connectivityTestResult}
        error={connectivityTestError}
        displayedResponseText={displayedResponseText}
        streamFallbackReason={connectivityStreamFallbackReason}
        progressLabel={connectivityProgressLabel}
        onDisplayedResponseTextChange={setDisplayedResponseText}
        testing={Boolean(connectivityDialogItem && testingKeyId === connectivityDialogItem.id)}
        onClose={() => {
          connectivityOperation.cancel();
          setConnectivityDialogItem(null);
          setConnectivityCapabilities(null);
          setConnectivityTestResult(null);
          setConnectivityTestError(null);
          setDisplayedResponseText("");
          setConnectivityStreamFallbackReason(null);
          setConnectivityProgressLabel(null);
          setTestingKeyId(null);
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
