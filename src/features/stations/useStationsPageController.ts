import { useCallback, useEffect, useMemo, useState, type FormEvent } from "react";
import type { DragEndEvent, DragStartEvent } from "@dnd-kit/core";
import { useQueryClient } from "@tanstack/react-query";
import { useToast } from "@/components/ui";
import {
  collectSub2apiStation,
  collectStationTask,
  getLatestCollectorSnapshot,
  listCollectorSnapshots,
  startManualAuthorization,
} from "@/lib/api/collector";
import { listCollectorRuns } from "@/lib/api/collectorRuns";
import { listGroupRateRecords, listStationGroupBindings } from "@/lib/api/groupFacts";
import {
  clearStationCredentials,
  createStationKey,
  deleteStationKey,
  getStationCredentials,
  listStationKeys,
  updateStationCredentials,
  updateStationKey,
} from "@/lib/api/stationKeys";
import { createStation, deleteStation, openStationWebsite, reorderStations, updateStation } from "@/lib/api/stations";
import { readError } from "@/lib/errors";
import { queryKeys } from "@/lib/query/queryKeys";
import {
  changeEventsQueryOptions,
  currentStationBalanceSnapshotsQueryOptions,
  stationAssetsQueryOptions,
  stationsQueryOptions,
} from "@/lib/query/resourceQueries";
import { useActivityQuery } from "@/lib/query/useActivityQuery";
import type { CollectorSnapshot } from "@/lib/types/collector";
import type { CollectorRun } from "@/lib/types/collectorRuns";
import type { GroupRateRecord, StationGroupBinding } from "@/lib/types/groupFacts";
import type { StationCredentials, StationKey } from "@/lib/types/stationKeys";
import type { Station } from "@/lib/types/stations";
import {
  buildStationAssetRows,
  filterStationAssetRowsByIssue,
  stationIssueTags,
  type StationIssueFilterValue,
} from "./stationAssetViewModels";
import {
  emptyForm,
  emptyKeyForm,
  formToInput,
  keyToForm,
  toCreateKeyInput,
  toUpdateKeyInput,
  type StationFormState,
  type StationKeyFormState,
} from "./pages/stations/formModel";
import type { DialogMode } from "./pages/stations/StationDialogs";

type StationAction = "collect" | "balance" | "authorize";

export type StationsPageControllerOptions = {
  onEditProvider?: (stationId: string) => void;
  onOpenStation?: (station: Station) => void;
};

export function useStationsPageController({
  onEditProvider,
  onOpenStation,
}: StationsPageControllerOptions) {
  const toast = useToast();
  const queryClient = useQueryClient();
  const [selectedStationId, setSelectedStationId] = useState<string | null>(null);
  const [activeDragId, setActiveDragId] = useState<string | null>(null);
  const [dialogMode, setDialogMode] = useState<DialogMode>(null);
  const [editingStationId, setEditingStationId] = useState<string | null>(null);
  const [detailStationId, setDetailStationId] = useState<string | null>(null);
  const [form, setForm] = useState<StationFormState>(emptyForm);
  const [credentials, setCredentials] = useState<StationCredentials | null>(null);
  const [stationKeys, setStationKeys] = useState<StationKey[]>([]);
  const [snapshots, setSnapshots] = useState<CollectorSnapshot[]>([]);
  const [snapshot, setSnapshot] = useState<CollectorSnapshot | null>(null);
  const [groupBindingsByStation, setGroupBindingsByStation] = useState(new Map<string, StationGroupBinding[]>());
  const [rateRecordsByStation, setRateRecordsByStation] = useState(new Map<string, GroupRateRecord[]>());
  const [collectorRunsByStation, setCollectorRunsByStation] = useState(new Map<string, CollectorRun[]>());
  const [drawerStationId, setDrawerStationId] = useState<string | null>(null);
  const [drawerVisible, setDrawerVisible] = useState(false);
  const [drawerClosing, setDrawerClosing] = useState(false);
  const [keyDialogOpen, setKeyDialogOpen] = useState(false);
  const [pendingDeleteKey, setPendingDeleteKey] = useState<StationKey | null>(null);
  const [pendingDeleteStation, setPendingDeleteStation] = useState<Station | null>(null);
  const [keyForm, setKeyForm] = useState<StationKeyFormState>(emptyKeyForm);
  const [saving, setSaving] = useState(false);
  const [actionSaving, setActionSaving] = useState(false);
  const [stationAction, setStationAction] = useState<{
    stationId: string;
    action: StationAction;
  } | null>(null);
  const [issueFilter, setIssueFilter] = useState<StationIssueFilterValue>("all");
  const [error, setError] = useState<string | null>(null);

  const stationsQuery = useActivityQuery(stationsQueryOptions());
  const balancesQuery = useActivityQuery(
    currentStationBalanceSnapshotsQueryOptions(),
  );
  const changesQuery = useActivityQuery(changeEventsQueryOptions(false));
  const stations = stationsQuery.data ?? [];
  const stationIds = useMemo(() => stations.map((station) => station.id), [stations]);
  const balanceSnapshots = balancesQuery.data ?? [];
  const changeEvents = changesQuery.data ?? [];
  const loading = stationsQuery.isPending && stationsQuery.data === undefined;
  const queryError = stationsQuery.error ? readError(stationsQuery.error) : null;
  const loadError = queryError ?? error;
  const balanceFactsReady = balancesQuery.data !== undefined;
  const stationAssetsQuery = useActivityQuery(stationAssetsQueryOptions(stationIds));
  const assetSnapshotsByStation = useMemo(
    () => {
      const latestSnapshotsByStation = new Map(
        (stationAssetsQuery.data ?? []).map((snapshot) => [snapshot.stationId, snapshot]),
      );
      return new Map(
        stations.map((station) => [station.id, latestSnapshotsByStation.get(station.id) ?? null]),
      );
    },
    [stationAssetsQuery.data, stations],
  );

  useEffect(() => {
    if (!drawerStationId) {
      setDrawerVisible(false);
      return;
    }

    setDrawerClosing(false);
    setDrawerVisible(false);
    const frameId = window.requestAnimationFrame(() => setDrawerVisible(true));
    return () => window.cancelAnimationFrame(frameId);
  }, [drawerStationId]);

  useEffect(() => {
    if (!drawerClosing) {
      return;
    }

    const timeoutId = window.setTimeout(() => {
      setDrawerStationId(null);
      setDetailStationId(null);
      setDrawerClosing(false);
    }, 220);
    return () => window.clearTimeout(timeoutId);
  }, [drawerClosing]);

  const selectedStation = useMemo(
    () => stations.find((station) => station.id === selectedStationId) ?? null,
    [selectedStationId, stations],
  );
  const detailStation = useMemo(
    () => stations.find((station) => station.id === detailStationId) ?? selectedStation,
    [detailStationId, selectedStation, stations],
  );
  const editingStation = useMemo(
    () => stations.find((station) => station.id === editingStationId) ?? null,
    [editingStationId, stations],
  );
  const activeDialogStation = dialogMode === "detail" ? detailStation : editingStation;
  const activeDragStation = useMemo(
    () => stations.find((station) => station.id === activeDragId) ?? null,
    [activeDragId, stations],
  );
  const keysByStation = useMemo(() => {
    const map = new Map<string, StationKey[]>();
    if (activeDialogStation && stationKeys.length > 0) {
      map.set(activeDialogStation.id, stationKeys);
    }
    return map;
  }, [activeDialogStation, stationKeys]);
  const snapshotsByStation = useMemo(() => {
    const map = new Map(assetSnapshotsByStation);
    if (detailStation && snapshot) {
      map.set(detailStation.id, snapshot);
    }
    return map;
  }, [assetSnapshotsByStation, detailStation, snapshot]);
  const stationAssetRows = useMemo(
    () =>
      buildStationAssetRows({
        stations,
        keysByStation,
        balances: balanceSnapshots,
        balanceFactsReady,
        snapshotsByStation,
        groupBindingsByStation,
        changes: changeEvents,
      }),
    [balanceFactsReady, balanceSnapshots, changeEvents, groupBindingsByStation, keysByStation, snapshotsByStation, stations],
  );
  const filteredStationAssetRows = useMemo(
    () => filterStationAssetRowsByIssue(stationAssetRows, issueFilter),
    [issueFilter, stationAssetRows],
  );
  const collectedBalanceCount = useMemo(
    () => filteredStationAssetRows.filter((row) => row.latestBalance?.value != null || row.station.balanceCny != null).length,
    [filteredStationAssetRows],
  );
  const filteredStationIds = useMemo(
    () => filteredStationAssetRows.map((row) => row.station.id),
    [filteredStationAssetRows],
  );
  const attentionCount = useMemo(
    () => filteredStationAssetRows.filter((row) => stationIssueTags(row).length > 0).length,
    [filteredStationAssetRows],
  );
  const activeDragRow = useMemo(
    () => stationAssetRows.find((row) => row.station.id === activeDragStation?.id) ?? null,
    [activeDragStation?.id, stationAssetRows],
  );

  useEffect(() => {
    if (!activeDialogStation) {
      return;
    }
    void refreshExtras(activeDialogStation.id);
  }, [activeDialogStation?.id]);

  useEffect(() => {
    setSelectedStationId((current) => {
      if (current && stations.some((station) => station.id === current)) {
        return current;
      }
      return null;
    });
  }, [stations]);

  const cancelStationSharedQueries = useCallback(
    async () => {
      await Promise.all([
        queryClient.cancelQueries({ queryKey: queryKeys.stations }),
        queryClient.cancelQueries({ queryKey: queryKeys.keyPool }),
        queryClient.cancelQueries({ queryKey: queryKeys.balanceSnapshots }),
        queryClient.cancelQueries({ queryKey: queryKeys.stationAssets }),
      ]);
    },
    [queryClient],
  );

  const invalidateStationSharedQueries = useCallback(
    async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: queryKeys.stations }),
        queryClient.invalidateQueries({ queryKey: queryKeys.keyPool }),
        queryClient.invalidateQueries({ queryKey: queryKeys.balanceSnapshots }),
        queryClient.invalidateQueries({ queryKey: queryKeys.stationAssets }),
      ]);
    },
    [queryClient],
  );

  const refreshExtras = useCallback(async (stationId: string) => {
    try {
      const [nextCredentials, nextKeys, nextSnapshots, nextSnapshot] = await Promise.all([
        getStationCredentials(stationId),
        listStationKeys(stationId),
        listCollectorSnapshots(stationId),
        getLatestCollectorSnapshot(stationId),
      ]);
      setCredentials(nextCredentials);
      setStationKeys(nextKeys);
      setSnapshots(nextSnapshots);
      setSnapshot(nextSnapshot);
      queryClient.setQueryData(queryKeys.stationAsset(stationId), nextSnapshot);
      if (dialogMode === "edit") {
        setForm((current) => ({
          ...current,
          loginUsername: nextCredentials.loginUsername ?? "",
          rememberPassword: nextCredentials.rememberPassword,
        }));
      }
      await refreshStationFacts(stationId);
    } catch (requestError) {
      toast.error("读取中转站详情失败", readError(requestError));
    }
  }, [dialogMode, queryClient]);

  async function refreshStationFacts(stationId: string) {
    const [bindings, rates, runs] = await Promise.all([
      listStationGroupBindings(stationId),
      listGroupRateRecords(stationId),
      listCollectorRuns(stationId),
    ]);
    setGroupBindingsByStation((current) => new Map(current).set(stationId, bindings));
    setRateRecordsByStation((current) => new Map(current).set(stationId, rates));
    setCollectorRunsByStation((current) => new Map(current).set(stationId, runs));
  }

  const openCreate = useCallback(() => {
    setDialogMode("create");
    setEditingStationId(null);
    setDetailStationId(null);
    setForm(emptyForm);
    setCredentials(null);
    setStationKeys([]);
    setSnapshots([]);
    setSnapshot(null);
    setError(null);
  }, []);

  const openEdit = useCallback((station: Station) => {
    if (onEditProvider) {
      setDialogMode(null);
      setEditingStationId(null);
      setDetailStationId(null);
      setDrawerStationId(null);
      setDrawerVisible(false);
      setDrawerClosing(false);
      onEditProvider(station.id);
      return;
    }

    setDialogMode("edit");
    setEditingStationId(station.id);
    setDetailStationId(null);
    setForm({
      name: station.name,
      stationType: station.stationType,
      websiteUrl: station.websiteUrl,
      apiBaseUrl: station.apiBaseUrl,
      apiKey: "",
      enabled: station.enabled,
      creditPerCny: String(station.creditPerCny),
      lowBalanceThresholdCny: station.lowBalanceThresholdCny === null ? "" : String(station.lowBalanceThresholdCny),
      collectionIntervalMinutes: String(station.collectionIntervalMinutes),
      note: station.note ?? "",
      loginUsername: "",
      loginPassword: "",
      rememberPassword: false,
    });
    setError(null);
  }, [onEditProvider]);

  const openDetail = useCallback((station: Station) => {
    if (onOpenStation) {
      setDialogMode(null);
      setDetailStationId(null);
      setEditingStationId(null);
      setDrawerStationId(null);
      setDrawerVisible(false);
      setDrawerClosing(false);
      setKeyDialogOpen(false);
      setKeyForm(emptyKeyForm);
      setError(null);
      onOpenStation(station);
      return;
    }

    const restoringCurrentDrawer = drawerStationId === station.id;
    setDialogMode("detail");
    setDetailStationId(station.id);
    setDrawerStationId(station.id);
    setDrawerClosing(false);
    if (restoringCurrentDrawer) {
      setDrawerVisible(true);
    }
    setEditingStationId(null);
    setSelectedStationId(station.id);
    setError(null);
    void refreshExtras(station.id);
  }, [drawerStationId, onOpenStation, refreshExtras]);

  const closeDrawer = useCallback(() => {
    if (!drawerStationId || drawerClosing) {
      return;
    }

    setDialogMode(null);
    setDrawerVisible(false);
    setDrawerClosing(true);
  }, [drawerClosing, drawerStationId]);

  const closeDialog = useCallback(() => {
    setDialogMode(null);
    setEditingStationId(null);
    setDetailStationId(null);
    setForm(emptyForm);
    setCredentials(null);
    setStationKeys([]);
    setSnapshots([]);
    setSnapshot(null);
    setDrawerStationId(null);
    setDrawerVisible(false);
    setDrawerClosing(false);
    setKeyDialogOpen(false);
    setPendingDeleteKey(null);
    setPendingDeleteStation(null);
    setKeyForm(emptyKeyForm);
  }, []);

  const handleDelete = useCallback((station: Station) => {
    setPendingDeleteStation(station);
  }, []);

  async function handleConfirmDeleteStation() {
    if (!pendingDeleteStation) {
      return;
    }
    setActionSaving(true);
    setError(null);
    try {
      await cancelStationSharedQueries();
      await deleteStation(pendingDeleteStation.id);
      setPendingDeleteStation(null);
      await invalidateStationSharedQueries();
      toast.success("站点已删除");
    } catch (requestError) {
      toast.error("删除站点失败", readError(requestError));
    } finally {
      setActionSaving(false);
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
    const oldIndex = stations.findIndex((station) => station.id === active.id);
    const newIndex = stations.findIndex((station) => station.id === over.id);
    if (oldIndex < 0 || newIndex < 0) {
      return;
    }
    const previousStations = stations;
    const nextStations = [...stations];
    const [moved] = nextStations.splice(oldIndex, 1);
    nextStations.splice(newIndex, 0, moved);
    await cancelStationSharedQueries();
    queryClient.setQueryData(queryKeys.stations, nextStations);
    try {
      const savedStations = await reorderStations(nextStations.map((station) => station.id));
      queryClient.setQueryData(queryKeys.stations, savedStations);
      await queryClient.invalidateQueries({ queryKey: queryKeys.stations });
      toast.success("站点排序已保存");
    } catch (requestError) {
      queryClient.setQueryData(queryKeys.stations, previousStations);
      toast.error("保存站点排序失败", readError(requestError));
    }
  }

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setSaving(true);
    setError(null);
    try {
      await cancelStationSharedQueries();
      const input = formToInput(form);
      if (dialogMode === "edit" && editingStationId) {
        await updateStation({
          ...input,
          id: editingStationId,
          apiKey: form.apiKey.trim() ? form.apiKey.trim() : null,
        });
        if (form.loginUsername.trim() || form.loginPassword.trim() || form.rememberPassword) {
          await updateStationCredentials({
            stationId: editingStationId,
            loginUsername: form.loginUsername.trim() ? form.loginUsername.trim() : null,
            loginPassword: form.loginPassword.trim() ? form.loginPassword.trim() : null,
            rememberPassword: form.rememberPassword,
          });
        }
        toast.success("站点已更新");
      } else {
        const nextStation = await createStation(input);
        if (form.loginUsername.trim() || form.loginPassword.trim() || form.rememberPassword) {
          await updateStationCredentials({
            stationId: nextStation.id,
            loginUsername: form.loginUsername.trim() ? form.loginUsername.trim() : null,
            loginPassword: form.loginPassword.trim() ? form.loginPassword.trim() : null,
            rememberPassword: form.rememberPassword,
          });
        }
        try {
          const result = await collectStationTask(nextStation.id, "balance");
          if (result.snapshot.status === "success") {
            toast.success("站点已创建，余额已刷新");
          } else {
            toast.success("站点已创建");
            toast.error(
              "余额未采集到",
              result.snapshot.errorMessage ?? "采集任务已结束，但没有写入可显示的余额。",
            );
          }
        } catch (balanceError) {
          toast.success("站点已创建");
          toast.error("刷新余额失败", readError(balanceError));
        }
      }
      await invalidateStationSharedQueries();
      closeDialog();
    } catch (requestError) {
      toast.error("保存站点失败", readError(requestError));
    } finally {
      setSaving(false);
    }
  }

  async function handleRemoveLoginInfo() {
    const stationId = activeDialogStation?.id;
    if (!stationId) {
      return;
    }
    setActionSaving(true);
    try {
      await clearStationCredentials(stationId);
      await refreshExtras(stationId);
      toast.success("登录信息已清除");
    } catch (requestError) {
      toast.error("清除登录信息失败", readError(requestError));
    } finally {
      setActionSaving(false);
    }
  }

  async function handleRunCollect(station = selectedStation) {
    if (!station || stationAction !== null) {
      return;
    }
    setStationAction({ stationId: station.id, action: "collect" });
    setError(null);
    try {
      await cancelStationSharedQueries();
      await collectSub2apiStation(station.id);
      await invalidateStationSharedQueries();
      if (station.id === selectedStationId || station.id === drawerStationId) {
        await refreshExtras(station.id);
      }
      await refreshStationFacts(station.id);
      toast.success("已保存采集快照");
    } catch (requestError) {
      toast.error("保存采集快照失败", readError(requestError));
    } finally {
      setStationAction(null);
    }
  }

  async function handleManualAuthorization(station: Station) {
    if (stationAction !== null) {
      return;
    }
    setStationAction({ stationId: station.id, action: "authorize" });
    setError(null);
    try {
      await startManualAuthorization(station.id);
      toast.success("授权窗口已打开，登录成功后会自动保存会话");
    } catch (requestError) {
      toast.error("打开授权窗口失败", readError(requestError));
    } finally {
      setStationAction(null);
    }
  }

  async function handleRefreshBalance(station: Station) {
    if (stationAction !== null) {
      return;
    }
    setStationAction({ stationId: station.id, action: "balance" });
    setError(null);
    try {
      await cancelStationSharedQueries();
      const result = await collectStationTask(station.id, "balance");
      await invalidateStationSharedQueries();
      if (station.id === selectedStationId || station.id === drawerStationId) {
        await refreshExtras(station.id);
      }
      await refreshStationFacts(station.id);
      if (result.snapshot.status === "success") {
        toast.success("余额已刷新");
      } else {
        toast.error(
          "余额未采集到",
          result.snapshot.errorMessage ?? "采集任务已结束，但没有写入可显示的余额。",
        );
      }
    } catch (requestError) {
      toast.error("刷新余额失败", readError(requestError));
    } finally {
      setStationAction(null);
    }
  }

  async function handleSaveKey(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!activeDialogStation) {
      return;
    }
    setActionSaving(true);
    setError(null);
    try {
      await cancelStationSharedQueries();
      if (keyForm.id) {
        await updateStationKey(toUpdateKeyInput(keyForm, activeDialogStation.id));
      } else {
        await createStationKey(toCreateKeyInput(keyForm, activeDialogStation.id));
      }
      setKeyDialogOpen(false);
      setKeyForm(emptyKeyForm);
      await refreshExtras(activeDialogStation.id);
      await invalidateStationSharedQueries();
      toast.success("密钥已保存");
    } catch (requestError) {
      toast.error("保存密钥失败", readError(requestError));
    } finally {
      setActionSaving(false);
    }
  }

  function handleDeleteKey(key: StationKey) {
    setPendingDeleteKey(key);
  }

  async function handleConfirmDeleteKey() {
    if (!pendingDeleteKey) {
      return;
    }
    setActionSaving(true);
    try {
      await cancelStationSharedQueries();
      await deleteStationKey(pendingDeleteKey.id);
      if (activeDialogStation) {
        await refreshExtras(activeDialogStation.id);
      }
      setPendingDeleteKey(null);
      await invalidateStationSharedQueries();
      toast.success("密钥已删除");
    } catch (requestError) {
      toast.error("删除密钥失败", readError(requestError));
    } finally {
      setActionSaving(false);
    }
  }

  const keyCountLabel = activeDialogStation ? `${activeDialogStation.keyCount} 把` : "0 把";

  function handleOpenWebsite(station: Station) {
    void openStationWebsite(station.websiteUrl);
  }

  function openCreateKeyDialog() {
    setKeyForm({ ...emptyKeyForm, priority: String(stationKeys.length) });
    setKeyDialogOpen(true);
  }

  function openEditKeyDialog(key: StationKey) {
    setKeyForm(keyToForm(key));
    setKeyDialogOpen(true);
  }

  return {
    actionSaving,
    activeDialogStation,
    activeDragRow,
    attentionCount,
    changeEvents,
    closeDialog,
    closeDrawer,
    collectedBalanceCount,
    collectorRunsByStation,
    credentials,
    detailStation,
    dialogMode,
    drawerClosing,
    drawerStationId,
    drawerVisible,
    filteredStationAssetRows,
    filteredStationIds,
    form,
    groupBindingsByStation,
    handleConfirmDeleteKey,
    handleConfirmDeleteStation,
    handleDelete,
    handleDeleteKey,
    handleDragCancel,
    handleDragEnd,
    handleDragStart,
    handleManualAuthorization,
    handleOpenWebsite,
    handleRefreshBalance,
    handleRemoveLoginInfo,
    handleRunCollect,
    handleSaveKey,
    handleSubmit,
    issueFilter,
    keyCountLabel,
    keyDialogOpen,
    keyForm,
    loadError,
    loading,
    openCreate,
    openCreateKeyDialog,
    openDetail,
    openEdit,
    openEditKeyDialog,
    pendingDeleteKey,
    pendingDeleteStation,
    rateRecordsByStation,
    refreshExtras,
    saving,
    selectedStationId,
    setForm,
    setIssueFilter,
    setKeyDialogOpen,
    setKeyForm,
    setPendingDeleteKey,
    setPendingDeleteStation,
    snapshot,
    snapshots,
    stationAction,
    stationKeys,
    stations,
  };
}
