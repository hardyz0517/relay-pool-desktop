import { useMemo, useState, type FormEvent } from "react";
import type { DragEndEvent, DragStartEvent } from "@dnd-kit/core";
import { useQueryClient } from "@tanstack/react-query";
import { createChannelMonitor, updateChannelMonitor } from "@/lib/api/channelMonitors";
import { listGroupRateRecords, listStationGroupBindings } from "@/lib/api/groupFacts";
import { getStationKeyCapabilities } from "@/lib/api/routing";
import { deleteStationKey, reorderKeyPool, saveStationKeyWithDefaults, updateStationKey } from "@/lib/api/stationKeys";
import {
  createStationKeyMonitorInput,
  findStationKeyMonitor,
  preferredStationKeyMonitorTemplate,
  updateStationKeyMonitorEnabledInput,
} from "@/lib/channelMonitorViewModel";
import { readError } from "@/lib/errors";
import { inferGroupCategoryFromEvidence } from "@/lib/groupCategories";
import { buildStationGroupOptionsFromCurrentFactsForSelect, findMatchingGroupOption } from "@/lib/groupOptionViewModels";
import { deriveStationGroupDisplayFacts } from "@/lib/projections/groupFacts";
import { queryKeys } from "@/lib/query/queryKeys";
import { invalidatePricingMonitoringQueries } from "@/lib/query/pricingMonitoringInvalidation";
import { channelMonitoringQueryOptions, keyPoolQueryOptions, stationsQueryOptions } from "@/lib/query/resourceQueries";
import { useActivityQuery } from "@/lib/query/useActivityQuery";
import type { StationGroupOption } from "@/lib/types/groupFacts";
import type { StationKeyCapabilities } from "@/lib/types/routing";
import type { KeyPoolItem, StationKeyConnectivityProgressEvent, StationKeyConnectivityTestResult } from "@/lib/types/stationKeys";
import { useToast } from "@/components/ui";
import {
  ConnectivityOperationCancelledError,
} from "./connectivityOperationController";
import { keyPoolMonitorStatus } from "./keyPoolMonitorStatus";
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
import { useConnectivityOperation } from "./useConnectivityOperation";

export type FilterMode = "all" | "enabled" | "disabled";

export type KeyPoolPageControllerOptions = {
  onAddKey?: (stationId: string | null) => void;
  onEditKey?: (stationKeyId: string) => void;
};

export function useKeyPoolPageController({
  onAddKey,
  onEditKey,
}: KeyPoolPageControllerOptions) {
  const toast = useToast();
  const queryClient = useQueryClient();
  const keyPoolItemsQuery = useActivityQuery(keyPoolQueryOptions());
  const stationsQuery = useActivityQuery(stationsQueryOptions());
  const channelMonitoringQuery = useActivityQuery(channelMonitoringQueryOptions(5_000));
  const connectivityOperation = useConnectivityOperation();
  const stations = stationsQuery.data ?? [];
  const items = keyPoolItemsQuery.data ?? [];
  const monitors = channelMonitoringQuery.data?.monitors ?? [];
  const channelStatusRows = channelMonitoringQuery.data?.statusWorkspace.rows ?? [];
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
  const monitorStatusByKey = useMemo(() => {
    return new Map(
      Array.from(monitorByKey.entries()).map(([stationKeyId, monitor]) => [
        stationKeyId,
        keyPoolMonitorStatus(monitor, channelStatusRows),
      ]),
    );
  }, [channelStatusRows, monitorByKey]);

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
      toast.error("更新密钥调度失败", readError(requestError));
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
      toast.error("无法测试连通性", "该密钥没有保存 API 密钥。");
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
    const handleConnectivityEvent = (event: StationKeyConnectivityProgressEvent) => {
      if (event.type === "attemptStarted") {
        setDisplayedResponseText("");
        setConnectivityStreamFallbackReason(null);
        setConnectivityProgressLabel(`流式请求：${event.protocol} / ${event.model}`);
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
      setDisplayedResponseText(result.message);
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
        const [capabilities, groupOptions] = await Promise.all([
          getStationKeyCapabilities(item.id),
          loadCurrentStationGroupOptions(item.stationId),
        ]);
        const groupCategory = findMatchingGroupOption({
          groupBindingId: item.groupBindingId,
          groupIdHash: item.groupIdHash,
          groupName: item.groupName ?? "",
        }, groupOptions)?.effectiveGroupCategory ?? inferGroupCategoryFromEvidence({
          groupName: item.groupName,
        });
        const preferredTemplate = preferredStationKeyMonitorTemplate(monitorTemplates, {
          stationType: item.stationType,
          stationUpstreamApiFormat: item.stationUpstreamApiFormat,
          capabilities,
        }) ?? template;
        await createChannelMonitor(
          createStationKeyMonitorInput(item, preferredTemplate, capabilities, groupCategory),
        );
      }
      await invalidatePricingMonitoringQueries(queryClient);
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

  function handleAddKeyClick() {
    if (onAddKey) {
      onAddKey(selectedStationId === "all" ? null : selectedStationId);
      return;
    }
    void handleCreateKey();
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

  function closeEditDialog() {
    setCreatingKey(false);
    setEditingItem(null);
  }

  function closeConnectivityDialog() {
    connectivityOperation.cancel();
    setConnectivityDialogItem(null);
    setConnectivityCapabilities(null);
    setConnectivityTestResult(null);
    setConnectivityTestError(null);
    setDisplayedResponseText("");
    setConnectivityStreamFallbackReason(null);
    setConnectivityProgressLabel(null);
    setTestingKeyId(null);
  }

  return {
    activeDragItem,
    connectivityCapabilities,
    connectivityDialogItem,
    connectivityProgressLabel,
    connectivityStreamFallbackReason,
    connectivityTestError,
    connectivityTestResult,
    creatingKey,
    displayError,
    displayedResponseText,
    dragEnabled,
    editForm,
    editingItem,
    filterMode,
    filteredEnabledCount,
    filteredItems,
    groupOptionsForEdit,
    loading,
    monitorByKey,
    monitorStatusByKey,
    monitoringKeyId,
    pendingDeleteItem,
    query,
    saving,
    selectedStationId,
    stationOptions,
    stations,
    testingKeyId,
    closeConnectivityDialog,
    closeEditDialog,
    handleAddKeyClick,
    handleConfirmDelete,
    handleCreateSave,
    handleCreateStationChange,
    handleDelete,
    handleDragCancel,
    handleDragEnd,
    handleDragStart,
    handleEdit,
    handleEditSave,
    handleRunConnectivityTest,
    handleTestConnectivity,
    handleToggleEnabled,
    handleToggleMonitoring,
    setDisplayedResponseText,
    setEditForm,
    setFilterMode,
    setPendingDeleteItem,
    setQuery,
    setSelectedStationId,
  };
}

async function loadCurrentStationGroupOptions(stationId: string) {
  const [bindings, rates] = await Promise.all([
    listStationGroupBindings(stationId),
    listGroupRateRecords(stationId),
  ]);
  return buildStationGroupOptionsFromCurrentFactsForSelect(
    deriveStationGroupDisplayFacts({ bindings, rates }),
  );
}
