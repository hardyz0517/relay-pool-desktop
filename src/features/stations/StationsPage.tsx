import { useCallback, useEffect, useMemo, useState, type FormEvent, type ReactNode } from "react";
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
import {
  SortableContext,
  useSortable,
  verticalListSortingStrategy,
  type AnimateLayoutChanges,
} from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { useQueries, useQueryClient } from "@tanstack/react-query";
import { Clock3, Edit3, GripVertical, KeyRound, Plus, RefreshCw, ShieldCheck, Trash2, X } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import { usePageRefreshEnabled } from "@/components/shell/PageActivity";
import { Button, ConfirmDialog, Dialog, EmptyState, IconButton, MaskedSecret, PropertyList, PropertyRow, SelectControl, StatusBadge, useToast } from "@/components/ui";
import { readError } from "@/lib/errors";
import { parseTimestampLikeDate } from "@/lib/time";
import { createStation, deleteStation, openStationWebsite, reorderStations, updateStation } from "@/lib/api/stations";
import {
  clearStationCredentials,
  createStationKey,
  deleteStationKey,
  getStationCredentials,
  listStationKeys,
  updateStationCredentials,
  updateStationKey,
} from "@/lib/api/stationKeys";
import {
  collectSub2apiStation,
  collectStationTask,
  getLatestCollectorSnapshot,
  listCollectorSnapshots,
  startManualAuthorization,
} from "@/lib/api/collector";
import { listCollectorRuns } from "@/lib/api/collectorRuns";
import { listGroupRateRecords, listStationGroupBindings } from "@/lib/api/groupFacts";
import { queryKeys } from "@/lib/query/queryKeys";
import {
  currentStationBalanceSnapshotsQueryOptions,
  changeEventsQueryOptions,
  stationAssetQueryOptions,
  stationsQueryOptions,
} from "@/lib/query/resourceQueries";
import { useActivityQuery } from "@/lib/query/useActivityQuery";
import type { ChangeEvent } from "@/lib/types/changeEvents";
import { stationKeyStatusLabels, type CreateStationKeyInput, type StationCredentials, type StationKey, type StationKeyStatus, type UpdateStationKeyInput } from "@/lib/types/stationKeys";
import type { CollectorSnapshot } from "@/lib/types/collector";
import type { CollectorRun } from "@/lib/types/collectorRuns";
import type { BalanceSnapshot } from "@/lib/types/economics";
import type { GroupRateRecord, StationGroupBinding } from "@/lib/types/groupFacts";
import {
  stationStatusLabels,
  stationTypeLabels,
  stationTypeOptions,
  type Station,
  type StationInput,
  type StationType,
} from "@/lib/types/stations";
import { cn } from "@/lib/utils";
import {
  buildStationAssetRows,
  filterStationAssetRowsByIssue,
  stationIssueTags,
  STATION_ISSUE_FILTER_OPTIONS,
  type StationAssetRow,
  type StationIssueFilterValue,
} from "./stationAssetViewModels";

type StationFormState = {
  name: string;
  stationType: StationType;
  websiteUrl: string;
  apiBaseUrl: string;
  apiKey: string;
  enabled: boolean;
  creditPerCny: string;
  lowBalanceThresholdCny: string;
  collectionIntervalMinutes: string;
  note: string;
  loginUsername: string;
  loginPassword: string;
  rememberPassword: boolean;
};

type StationKeyFormState = {
  id: string | null;
  name: string;
  apiKey: string;
  enabled: boolean;
  priority: string;
  groupName: string;
  tierLabel: string;
  status: StationKeyStatus;
  note: string;
};

type DialogMode = "create" | "edit" | "detail" | null;
type StationAction = "collect" | "balance" | "authorize";

const emptyForm: StationFormState = {
  name: "",
  stationType: "sub2api",
  websiteUrl: "",
  apiBaseUrl: "",
  apiKey: "",
  enabled: true,
  creditPerCny: "1",
  lowBalanceThresholdCny: "",
  collectionIntervalMinutes: "5",
  note: "",
  loginUsername: "",
  loginPassword: "",
  rememberPassword: false,
};

const emptyKeyForm: StationKeyFormState = {
  id: null,
  name: "",
  apiKey: "",
  enabled: true,
  priority: "0",
  groupName: "",
  tierLabel: "",
  status: "unchecked",
  note: "",
};

const statusTone: Record<Station["status"], "healthy" | "warning" | "error" | "disabled" | "info"> = {
  healthy: "healthy",
  warning: "warning",
  error: "error",
  disabled: "disabled",
  unchecked: "info",
};
const shouldAnimateStationAssetLayoutChanges: AnimateLayoutChanges = ({ isSorting, wasDragging }) =>
  isSorting || wasDragging;

type StationsPageProps = {
  onAddProvider?: () => void;
  onEditProvider?: (stationId: string) => void;
  onOpenStation?: (station: Station) => void;
};

export function StationsPage({ onAddProvider, onEditProvider, onOpenStation }: StationsPageProps) {
  const toast = useToast();
  const queryClient = useQueryClient();
  const refreshEnabled = usePageRefreshEnabled();
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

  const sensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 8 } }));
  const stationsQuery = useActivityQuery(refreshEnabled, stationsQueryOptions());
  const balancesQuery = useActivityQuery(
    refreshEnabled,
    currentStationBalanceSnapshotsQueryOptions(),
  );
  const changesQuery = useActivityQuery(refreshEnabled, changeEventsQueryOptions(false));
  const stations = stationsQuery.data ?? [];
  const balanceSnapshots = balancesQuery.data ?? [];
  const changeEvents = changesQuery.data ?? [];
  const loading = stationsQuery.isPending && stationsQuery.data === undefined;
  const queryError = stationsQuery.error ? readError(stationsQuery.error) : null;
  const loadError = queryError ?? error;
  const balanceFactsReady = balancesQuery.data !== undefined;
  const stationAssetQueries = useQueries({
    queries: stations.map((station) => ({
      ...stationAssetQueryOptions(station.id),
      enabled: refreshEnabled,
      subscribed: refreshEnabled,
    })),
  });
  const assetSnapshotsByStation = useMemo(
    () =>
      new Map(
        stations.map((station, index) => [
          station.id,
          stationAssetQueries[index]?.data ?? null,
        ]),
      ),
    [stationAssetQueries, stations],
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

  return (
    <PageScaffold
      title="中转站资产"
      status={
        <div className="flex min-w-0 flex-wrap items-center gap-1.5" aria-label="中转站资产状态">
          <StatusBadge tone="info" className="bg-surface-subtle text-muted-foreground">
            {`${filteredStationAssetRows.length} 站点`}
          </StatusBadge>
          <StatusBadge tone={collectedBalanceCount > 0 ? "healthy" : "disabled"}>
            {`${collectedBalanceCount} 已有余额`}
          </StatusBadge>
          <StatusBadge tone={attentionCount > 0 ? "warning" : "healthy"}>
            {`${attentionCount} 需关注`}
          </StatusBadge>
        </div>
      }
      actions={
        <div className="flex flex-wrap items-center justify-end gap-2">
          <SelectControl<StationIssueFilterValue>
            ariaLabel="筛选问题标签"
            className={stationAssetSelectClassName}
            value={issueFilter}
            options={STATION_ISSUE_FILTER_OPTIONS}
            onChange={setIssueFilter}
          />
          <Button onClick={onAddProvider ?? openCreate}>
            <Plus className="h-4 w-4" />
            添加供应商
          </Button>
        </div>
      }
    >
      <div className="grid min-w-0 gap-3">
        <div>
          {loading ? (
            <div className="rounded-[var(--surface-radius)] border border-border bg-surface px-4 py-5 text-sm text-muted-foreground shadow-[var(--surface-shadow)]">
              正在读取本地数据...
            </div>
          ) : loadError ? (
            <div className="rounded-[var(--surface-radius)] border border-danger-border bg-danger-surface px-4 py-5 text-sm text-danger-foreground shadow-[var(--surface-shadow)]">
              {loadError}
            </div>
          ) : stations.length === 0 ? (
            <EmptyState
              title="还没有中转站"
              description="添加一个站点开始管理登录账号和多把密钥。"
              action={<Button onClick={onAddProvider ?? openCreate}>添加供应商</Button>}
            />
          ) : filteredStationAssetRows.length === 0 ? (
            <EmptyState
              title="没有匹配的问题站点"
              description="切换问题筛选条件，或继续查看全部中转站资产。"
            />
          ) : (
            <DndContext
              sensors={sensors}
              collisionDetection={closestCenter}
              onDragStart={handleDragStart}
              onDragCancel={handleDragCancel}
              onDragEnd={handleDragEnd}
            >
              <SortableContext items={filteredStationIds} strategy={verticalListSortingStrategy}>
                <div className="space-y-2">
                  {filteredStationAssetRows.map((row) => (
                    <SortableStationAssetListRow
                      key={row.station.id}
                      actionDisabled={stationAction !== null}
                      active={row.station.id === selectedStationId}
                      loadingAction={stationAction?.stationId === row.station.id ? stationAction.action : null}
                      row={row}
                      onAuthorize={(station) => void handleManualAuthorization(station)}
                      onCollect={(station) => void handleRunCollect(station)}
                      onDelete={handleDelete}
                      onEdit={openEdit}
                      onOpen={openDetail}
                      onRefreshBalance={(station) => void handleRefreshBalance(station)}
                    />
                  ))}
                </div>
              </SortableContext>
              <DragOverlay dropAnimation={null}>
                {activeDragRow ? (
                  <StationAssetListRow
                    actionDisabled
                    active
                    loadingAction={stationAction?.stationId === activeDragRow.station.id ? stationAction.action : null}
                    overlay
                    row={activeDragRow}
                    onCollect={() => undefined}
                    onDelete={() => undefined}
                    onEdit={() => undefined}
                    onAuthorize={() => undefined}
                    onOpen={() => undefined}
                    onRefreshBalance={() => undefined}
                  />
                ) : null}
              </DragOverlay>
            </DndContext>
          )}
        </div>
      </div>

      {drawerStationId && detailStation && (
        <div
          className={cn(
            "fixed inset-0 z-40 bg-transparent transition-colors duration-200 ease-out",
            drawerVisible && !drawerClosing && "bg-scrim/20",
          )}
          onMouseDown={closeDrawer}
        >
          <div
            className={cn(
              "absolute inset-y-0 right-0 w-[min(560px,calc(100vw-72px))] border-l border-border bg-surface shadow-surface transition-transform duration-[220ms] ease-[cubic-bezier(0.22,1,0.36,1)] will-change-transform",
              drawerVisible && !drawerClosing ? "translate-x-0" : "translate-x-full",
            )}
            onMouseDown={(event) => event.stopPropagation()}
          >
            <div className="flex h-full min-h-0 flex-col">
              <div className="flex items-start justify-between gap-3 border-b border-border px-4 py-3">
                <div className="min-w-0">
                  <div className="truncate text-sm font-semibold text-foreground">{detailStation.name}</div>
                  <div className="truncate text-xs text-muted-foreground">{detailStation.websiteUrl}</div>
                </div>
                <IconButton
                  className="shrink-0 text-muted-foreground transition-colors hover:bg-muted hover:text-foreground active:bg-hover"
                  label="关闭详情抽屉"
                  onClick={closeDrawer}
                >
                  <X className="h-4 w-4" />
                </IconButton>
              </div>
              <div className="flex items-center justify-between gap-2 border-b border-border px-4 py-2">
                <div className="text-xs text-muted-foreground">{keyCountLabel}</div>
                <div className="flex gap-2">
                  <Button variant="outline" onClick={() => void refreshExtras(detailStation.id)} disabled={actionSaving}>
                    <RefreshCw className="h-4 w-4" />
                    刷新
                  </Button>
                  <Button variant="secondary" onClick={() => {
                    setKeyForm({ ...emptyKeyForm, priority: String(stationKeys.length) });
                    setKeyDialogOpen(true);
                  }}>
                    <Plus className="h-4 w-4" />
                    新增密钥
                  </Button>
                  <Button variant="outline" onClick={() => openEdit(detailStation)}>
                    <Edit3 className="h-4 w-4" />
                    编辑
                  </Button>
                </div>
              </div>
              <div className="min-h-0 flex-1 overflow-auto">
                <DetailBody
                  activeDialogStation={detailStation}
                  changeEvents={changeEvents.filter((event) => event.stationId === detailStation.id)}
                  credentials={credentials}
                  keyCountLabel={keyCountLabel}
                  snapshot={snapshot}
                  snapshots={snapshots}
                  stationKeys={stationKeys}
                  groupBindings={groupBindingsByStation.get(detailStation.id) ?? []}
                  rateRecords={rateRecordsByStation.get(detailStation.id) ?? []}
                  collectorRuns={collectorRunsByStation.get(detailStation.id) ?? []}
                  onDeleteKey={handleDeleteKey}
                  onEditKey={(key) => {
                    setKeyForm(keyToForm(key));
                    setKeyDialogOpen(true);
                  }}
                />
              </div>
            </div>
          </div>
        </div>
      )}

      {(dialogMode || keyDialogOpen) && (
        <StationDialogs
          activeDialogStation={activeDialogStation}
          actionSaving={actionSaving}
          credentials={credentials}
          dialogMode={dialogMode}
          form={form}
          keyDialogOpen={keyDialogOpen}
          keyForm={keyForm}
          onChange={setForm}
          onClose={closeDialog}
          onDeleteKey={handleDeleteKey}
          onEdit={activeDialogStation ? () => openEdit(activeDialogStation) : undefined}
          onKeyDialogOpenChange={setKeyDialogOpen}
          onKeyFormChange={setKeyForm}
          onKeySave={handleSaveKey}
          onRemoveLoginInfo={handleRemoveLoginInfo}
          onSubmit={handleSubmit}
          onRefreshDetail={activeDialogStation ? () => refreshExtras(activeDialogStation.id) : undefined}
          saving={saving}
          snapshots={snapshots}
          stationKeys={stationKeys}
          keyCountLabel={keyCountLabel}
          snapshot={snapshot}
        />
      )}
      <ConfirmDialog
        open={pendingDeleteStation !== null}
        title="删除中转站"
        description={`确定要删除站点 "${pendingDeleteStation?.name ?? ""}" 吗？此操作无法撤销。`}
        confirmLabel="删除"
        confirming={actionSaving}
        onCancel={() => setPendingDeleteStation(null)}
        onConfirm={() => void handleConfirmDeleteStation()}
      />
      <ConfirmDialog
        open={pendingDeleteKey !== null}
        title="删除密钥"
        description={`确定要删除密钥 "${pendingDeleteKey?.name ?? ""}" 吗？此操作无法撤销。`}
        confirming={actionSaving}
        onCancel={() => setPendingDeleteKey(null)}
        onConfirm={() => void handleConfirmDeleteKey()}
      />
    </PageScaffold>
  );
}

type StationAssetListRowProps = {
  row: StationAssetRow;
  active: boolean;
  actionDisabled: boolean;
  loadingAction: StationAction | null;
  overlay?: boolean;
  dragAttributes?: DraggableAttributes;
  dragListeners?: ReturnType<typeof useSortable>["listeners"];
  onOpen: (station: Station) => void;
  onEdit: (station: Station) => void;
  onAuthorize: (station: Station) => void;
  onCollect: (station: Station) => void;
  onDelete: (station: Station) => void;
  onRefreshBalance: (station: Station) => void;
};

function SortableStationAssetListRow(props: StationAssetListRowProps) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({
    id: props.row.station.id,
    animateLayoutChanges: shouldAnimateStationAssetLayoutChanges,
  });

  return (
    <div
      ref={setNodeRef}
      style={{ transform: CSS.Transform.toString(transform), transition }}
      className={cn("will-change-transform", isDragging && "opacity-35")}
    >
      <StationAssetListRow
        {...props}
        dragAttributes={attributes}
        dragListeners={listeners}
      />
    </div>
  );
}

function StationAssetListRow({
  row,
  active,
  actionDisabled,
  loadingAction,
  overlay = false,
  dragAttributes,
  dragListeners,
  onOpen,
  onEdit,
  onAuthorize,
  onCollect,
  onDelete,
  onRefreshBalance,
}: StationAssetListRowProps) {
  const station = row.station;
  const issueTags = stationIssueTags(row);
  const balance = formatStationBalanceParts(row);
  const lastCollectText = formatRelativeTime(
    row.latestBalance?.updatedAt ?? row.latestBalance?.collectedAt ?? station.lastCheckedAt ?? station.updatedAt,
  );

  return (
    <div
      role="button"
      tabIndex={0}
      aria-pressed={active}
      className={cn(
        "group flex min-h-[78px] w-full cursor-pointer flex-wrap items-center gap-3 rounded-[14px] border px-4 py-3 text-left transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/30 md:flex-nowrap",
        active
          ? "border-primary/45 bg-selected"
          : "border-border bg-surface hover:border-info-border hover:bg-surface-subtle",
        overlay && "shadow-surface",
      )}
      onClick={() => onOpen(station)}
      onKeyDown={(event) => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          onOpen(station);
        }
      }}
    >
      <div
        className="shrink-0"
        onClick={(event) => event.stopPropagation()}
        onKeyDown={(event) => event.stopPropagation()}
      >
        <button
          type="button"
          aria-label={`拖拽排序 ${station.name}`}
          className="inline-flex h-7 w-5 cursor-grab items-center justify-center rounded-[6px] text-muted-foreground/45 transition-colors hover:bg-muted hover:text-muted-foreground active:cursor-grabbing focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/30"
          {...dragAttributes}
          {...dragListeners}
        >
          <GripVertical className="h-4 w-4" />
        </button>
      </div>
      <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-full border border-border bg-surface text-xs font-semibold text-muted-foreground shadow-surface">
        {stationAvatarLabel(station.name)}
      </div>

      <div className="min-w-0 flex-[1_1_calc(100%-5rem)] md:flex-1">
        <div className="flex min-w-0 items-center gap-2">
          <div className="truncate text-[15px] font-semibold leading-5 text-foreground">{station.name}</div>
          <span className="hidden rounded-full border border-border bg-surface/80 px-2 py-0.5 text-[11px] font-medium leading-4 text-muted-foreground sm:inline-flex">
            {stationTypeLabels[station.stationType]}
          </span>
          {issueTags.map((tag) => (
            <span
              key={tag.label}
              className={cn(
                "hidden rounded-full border px-2 py-0.5 text-[11px] font-medium leading-4 sm:inline-flex",
                stationIssueTagClassName(tag.tone),
              )}
              title={tag.title ?? tag.label}
            >
              {tag.label}
            </span>
          ))}
        </div>
        <button
          type="button"
          aria-label={`在浏览器打开 ${station.name}`}
          title={station.websiteUrl}
          className="mt-1 block max-w-full truncate text-left text-xs font-medium text-primary hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/30"
          onClick={(event) => {
            event.stopPropagation();
            void openStationWebsite(station.websiteUrl);
          }}
          onKeyDown={(event) => event.stopPropagation()}
        >
          {formatStationDisplayUrl(station.websiteUrl)}
        </button>
      </div>

      <div className="hidden shrink-0 items-center gap-5 md:flex">
        <div className="min-w-[78px] text-right">
          <div className="flex items-center justify-end gap-1 text-[11px] leading-4 text-muted-foreground/70">
            <Clock3 className="h-3 w-3" />
            <span>{lastCollectText}</span>
            <button
              type="button"
              aria-label={`刷新余额 ${station.name}`}
              title={`刷新余额 ${station.name}`}
              disabled={actionDisabled || !station.enabled}
              className="ml-0.5 inline-flex h-4 w-4 cursor-pointer items-center justify-center rounded-[5px] text-muted-foreground/70 transition-colors hover:bg-muted hover:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/30 disabled:cursor-default disabled:opacity-40"
              onClick={(event) => {
                event.stopPropagation();
                onRefreshBalance(station);
              }}
              onKeyDown={(event) => {
                event.stopPropagation();
              }}
            >
              <RefreshCw className={cn("h-3 w-3", loadingAction === "balance" && "animate-spin")} />
            </button>
          </div>
          <div className="mt-1 text-xs leading-4 text-muted-foreground">
            余额：
            <span className={cn("font-semibold", balance.amount === "未采集" ? "text-muted-foreground" : "text-success-foreground")}>
              {balance.amount}
            </span>
            {balance.currency && <span className="ml-1 text-muted-foreground">{balance.currency}</span>}
          </div>
        </div>
      </div>

      <div
        className="ml-auto flex shrink-0 items-center gap-1 opacity-100 transition-opacity md:opacity-0 md:group-hover:opacity-100 md:group-focus-within:opacity-100"
        onClick={(event) => event.stopPropagation()}
        onKeyDown={(event) => event.stopPropagation()}
      >
        <IconButton className="text-muted-foreground hover:text-foreground" label={`编辑 ${station.name}`} onClick={() => onEdit(station)}>
          <Edit3 className="h-4 w-4" />
        </IconButton>
        <IconButton className="text-muted-foreground hover:text-foreground" label={`管理 Key ${station.name}`} onClick={() => onEdit(station)}>
          <KeyRound className="h-4 w-4" />
        </IconButton>
        {supportsManualAuthorization(station) && (
          <IconButton
            className={cn(
              "text-muted-foreground hover:text-foreground",
              rowNeedsManualAuthorization(row) && "text-warning-foreground hover:bg-warning-surface hover:text-warning-foreground",
            )}
            disabled={actionDisabled || !station.enabled}
            label={`重新授权 ${station.name}`}
            onClick={() => onAuthorize(station)}
          >
            <ShieldCheck className={cn("h-4 w-4", loadingAction === "authorize" && "animate-pulse")} />
          </IconButton>
        )}
        <IconButton
          className="text-muted-foreground hover:text-foreground"
          disabled={actionDisabled || !station.enabled}
          label={`采集信息 ${station.name}`}
          onClick={() => onCollect(station)}
        >
          <RefreshCw className={cn("h-4 w-4", loadingAction === "collect" && "animate-spin")} />
        </IconButton>
        <IconButton
          className="text-muted-foreground/70 hover:bg-danger-surface hover:text-danger-foreground"
          label={`删除 ${station.name}`}
          onClick={() => onDelete(station)}
        >
          <Trash2 className="h-4 w-4" />
        </IconButton>
      </div>
    </div>
  );
}

function supportsManualAuthorization(station: Station) {
  return station.stationType === "sub2api" || station.stationType === "newapi";
}

function rowNeedsManualAuthorization(row: StationAssetRow) {
  const summary = row.latestSnapshot?.summaryJson ?? {};
  return (
    row.latestSnapshot?.status === "manual_required" ||
    summary.loginRequired === true ||
    summary.loginStatus === "manual_required"
  );
}

function StationDialogs({
  activeDialogStation,
  actionSaving,
  credentials,
  dialogMode,
  form,
  keyDialogOpen,
  keyForm,
  onChange,
  onClose,
  onDeleteKey,
  onEdit,
  onKeyDialogOpenChange,
  onKeyFormChange,
  onKeySave,
  onRefreshDetail,
  onRemoveLoginInfo,
  onSubmit,
  saving,
  snapshots,
  stationKeys,
  keyCountLabel,
  snapshot,
}: {
  activeDialogStation: Station | null;
  actionSaving: boolean;
  credentials: StationCredentials | null;
  dialogMode: DialogMode;
  form: StationFormState;
  keyDialogOpen: boolean;
  keyForm: StationKeyFormState;
  onChange: (nextForm: StationFormState) => void;
  onClose: () => void;
  onDeleteKey: (key: StationKey) => void;
  onEdit?: () => void;
  onKeyDialogOpenChange: (next: boolean) => void;
  onKeyFormChange: (next: StationKeyFormState) => void;
  onKeySave: (event: FormEvent<HTMLFormElement>) => void;
  onRefreshDetail?: () => void;
  onRemoveLoginInfo: () => Promise<void>;
  onSubmit: (event: FormEvent<HTMLFormElement>) => void;
  saving: boolean;
  snapshots: CollectorSnapshot[];
  stationKeys: StationKey[];
  keyCountLabel: string;
  snapshot: CollectorSnapshot | null;
}) {
  if (dialogMode === "detail" && activeDialogStation) {
    return (
      <>
        {keyDialogOpen && (
          <KeyDialog
            actionSaving={actionSaving}
            keyForm={keyForm}
            onKeyDialogOpenChange={onKeyDialogOpenChange}
            onKeyFormChange={onKeyFormChange}
            onKeySave={onKeySave}
          />
        )}
      </>
    );
  }

  const endpointOriginWarnings =
    dialogMode === "edit" && activeDialogStation
      ? stationEndpointOriginWarnings(activeDialogStation, form)
      : [];

  return (
    <>
      <Dialog
        open
        title={dialogMode === "edit" ? "编辑站点" : "新增站点"}
        description={dialogMode === "edit" ? "密钥留空则保留旧值。登录账号区用于采集。": undefined}
        onClose={onClose}
        footer={
          <div className="flex justify-end gap-2">
            <Button variant="outline" onClick={onClose}>取消</Button>
            <Button disabled={saving} type="submit" form="station-form">保存</Button>
          </div>
        }
      >
        <form id="station-form" className="grid gap-4 p-5" onSubmit={onSubmit}>
          <div className="grid gap-3 md:grid-cols-2">
            <Field label="站点名称">
              <input className={inputClassName} value={form.name} onChange={(event) => onChange({ ...form, name: event.target.value })} required />
            </Field>
            <Field label="站点类型">
              <SelectControl
                ariaLabel="站点类型"
                className={inputClassName}
                value={form.stationType}
                options={stationTypeOptions}
                onChange={(stationType) => onChange({ ...form, stationType })}
              />
            </Field>
          </div>
          <div className="grid gap-3 md:grid-cols-2">
            <Field label="前端网址">
              <input className={inputClassName} value={form.websiteUrl} onChange={(event) => onChange({ ...form, websiteUrl: event.target.value })} placeholder="https://example.com" required />
            </Field>
            <Field label="API Base URL">
              <input className={inputClassName} value={form.apiBaseUrl} onChange={(event) => onChange({ ...form, apiBaseUrl: event.target.value })} placeholder="https://api.example.com/v1" required />
            </Field>
          </div>
          <div className="flex justify-end">
            <Button variant="outline" onClick={() => onChange({ ...form, apiBaseUrl: form.websiteUrl })}>
              复制前端网址
            </Button>
          </div>
          {endpointOriginWarnings.length > 0 && (
            <div className="rounded-[var(--surface-radius)] border border-warning-border bg-warning-surface px-3 py-2 text-xs text-warning-foreground">
              {endpointOriginWarnings.map((warning) => (
                <div key={warning}>{warning}</div>
              ))}
            </div>
          )}
          <Field label={dialogMode === "edit" ? "密钥（留空保留旧值）" : "密钥"}>
            <input className={inputClassName} value={form.apiKey} onChange={(event) => onChange({ ...form, apiKey: event.target.value })} placeholder={dialogMode === "edit" ? "留空保留旧密钥" : "sk-..."} required={dialogMode !== "edit"} />
          </Field>
          <div className="grid gap-3 md:grid-cols-3">
            <Field label="兑换比例">
              <input className={inputClassName} min="0.01" step="0.01" type="number" value={form.creditPerCny} onChange={(event) => onChange({ ...form, creditPerCny: event.target.value })} />
            </Field>
            <Field label="低余额阈值">
              <input className={inputClassName} min="0" step="0.01" type="number" value={form.lowBalanceThresholdCny} onChange={(event) => onChange({ ...form, lowBalanceThresholdCny: event.target.value })} placeholder="使用全局设置" />
            </Field>
            <Field label="采集频率 分钟">
              <input className={inputClassName} min="1" step="1" type="number" value={form.collectionIntervalMinutes} onChange={(event) => onChange({ ...form, collectionIntervalMinutes: event.target.value })} placeholder="5" />
            </Field>
          </div>
          <div className="grid gap-3 md:grid-cols-3">
            <label className="flex items-end gap-2 pb-2 text-sm text-foreground">
              <input checked={form.enabled} className="h-4 w-4 accent-primary" type="checkbox" onChange={(event) => onChange({ ...form, enabled: event.target.checked })} />
              启用站点
            </label>
          </div>
          <Field label="备注">
            <textarea className={`${inputClassName} min-h-20 resize-none py-2`} value={form.note} onChange={(event) => onChange({ ...form, note: event.target.value })} />
          </Field>
          <SectionBlock title="登录账号（用于采集）">
            <div className="grid gap-3 md:grid-cols-2">
              <Field label="登录用户名 / 邮箱">
                <input className={inputClassName} value={form.loginUsername} onChange={(event) => onChange({ ...form, loginUsername: event.target.value })} placeholder="user@example.com" />
              </Field>
              <Field label="登录密码">
                <input className={inputClassName} type="password" value={form.loginPassword} onChange={(event) => onChange({ ...form, loginPassword: event.target.value })} placeholder="留空保留旧密码" />
              </Field>
            </div>
            <div className="mt-2 flex items-center gap-4 text-sm text-foreground">
              <label className="flex items-center gap-2">
                <input checked={form.rememberPassword} className="h-4 w-4 accent-primary" type="checkbox" onChange={(event) => onChange({ ...form, rememberPassword: event.target.checked })} />
                记住密码
              </label>
              <span className="text-xs text-muted-foreground">保存后密码会写入本地加密存储；留空不会覆盖旧密码。</span>
            </div>
            {credentials && (
              <div className="mt-3 rounded-[var(--surface-radius)] border border-border bg-surface p-3 text-xs text-foreground shadow-[var(--surface-shadow)]">
                当前登录状态: {credentials.loginStatus}
                {credentials.loginError ? ` · ${credentials.loginError}` : ""}
              </div>
            )}
            {credentials && (
              <div className="mt-3 flex justify-end">
                <Button variant="outline" onClick={onRemoveLoginInfo} disabled={actionSaving}>清除登录信息</Button>
              </div>
            )}
          </SectionBlock>
        </form>
      </Dialog>

      {keyDialogOpen && (
        <KeyDialog
          actionSaving={actionSaving}
          keyForm={keyForm}
          onKeyDialogOpenChange={onKeyDialogOpenChange}
          onKeyFormChange={onKeyFormChange}
          onKeySave={onKeySave}
        />
      )}
    </>
  );
}

function DetailBody({
  activeDialogStation,
  changeEvents,
  credentials,
  keyCountLabel,
  snapshot,
  snapshots,
  stationKeys,
  groupBindings,
  rateRecords,
  collectorRuns,
  onDeleteKey,
  onEditKey,
}: {
  activeDialogStation: Station;
  changeEvents: ChangeEvent[];
  credentials: StationCredentials | null;
  keyCountLabel: string;
  snapshot: CollectorSnapshot | null;
  snapshots: CollectorSnapshot[];
  stationKeys: StationKey[];
  groupBindings: StationGroupBinding[];
  rateRecords: GroupRateRecord[];
  collectorRuns: CollectorRun[];
  onDeleteKey: (key: StationKey) => void;
  onEditKey: (key: StationKey) => void;
}) {
  return (
    <div className="space-y-4 p-5">
      <PropertyList className="overflow-hidden rounded-[var(--surface-radius)] border border-info-border bg-surface/80">
        <PropertyRow label="站点名称" value={activeDialogStation.name} />
        <PropertyRow label="站点类型" value={stationTypeLabels[activeDialogStation.stationType]} />
        <PropertyRow label="前端网址" value={<code className="text-xs">{activeDialogStation.websiteUrl}</code>} />
        <PropertyRow label="API Base URL" value={<code className="text-xs">{activeDialogStation.apiBaseUrl}</code>} />
        <PropertyRow label="余额" value={activeDialogStation.balanceCny === null ? "未采集" : `¥${activeDialogStation.balanceCny.toFixed(2)}`} />
        <PropertyRow label="密钥数量" value={keyCountLabel} />
        <PropertyRow label="状态" value={stationStatusLabels[activeDialogStation.status]} />
        <PropertyRow label="采集时间" value={activeDialogStation.lastPricingFetchedAt ?? "未采集"} />
        <PropertyRow label="刷新时间" value={activeDialogStation.lastCheckedAt ?? "未检测"} />
      </PropertyList>

      <SectionBlock title="登录账号">
        {credentials ? (
          <PropertyList className="overflow-hidden rounded-[var(--surface-radius)] border border-info-border bg-surface/80">
            <PropertyRow label="登录用户名" value={credentials.loginUsername || "未设置"} />
            <PropertyRow label="密码" value={credentials.passwordPresent ? "已保存" : "未保存"} />
            <PropertyRow label="记住密码" value={credentials.rememberPassword ? "是" : "否"} />
            <PropertyRow label="登录状态" value={credentials.loginStatus} />
            <PropertyRow label="最近登录" value={credentials.lastLoginAt ?? "未登录"} />
            <PropertyRow label="登录错误" value={credentials.loginError ?? "无"} />
          </PropertyList>
        ) : (
          <div className="rounded-[var(--surface-radius)] border border-border bg-surface p-3 text-sm text-muted-foreground shadow-[var(--surface-shadow)]">未保存登录账号。</div>
        )}
      </SectionBlock>

      <SectionBlock title="密钥">
        <div className="space-y-2">
          {stationKeys.length === 0 ? (
            <div className="rounded-[var(--surface-radius)] border border-border bg-surface p-3 text-sm text-muted-foreground shadow-[var(--surface-shadow)]">暂无密钥。</div>
          ) : (
            stationKeys.map((key) => (
              <div key={key.id} className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-3 rounded-[var(--surface-radius)] border border-border bg-surface px-3 py-2.5 shadow-[var(--surface-shadow)]">
                <div className="min-w-0">
                  <div className="flex items-center gap-2">
                    <div className="truncate text-sm font-medium text-foreground">{key.name}</div>
                    <StatusBadge tone={statusTone[key.status]}>{stationKeyStatusLabels[key.status]}</StatusBadge>
                    <span className="text-[11px] text-muted-foreground">P{key.priority}</span>
                  </div>
                  <div className="mt-1 flex flex-wrap gap-2 text-xs text-muted-foreground">
                    <MaskedSecret value={key.apiKeyMasked} present={key.apiKeyPresent} />
                    <span>{key.groupName ?? "未分组"}</span>
                    <span>{key.tierLabel ?? "无档位"}</span>
                    <span>{key.enabled ? "启用" : "停用"}</span>
                  </div>
                </div>
                <div className="flex gap-2">
                  <Button variant="outline" onClick={() => onEditKey(key)}>编辑</Button>
                  <Button variant="danger" onClick={() => onDeleteKey(key)}>删除</Button>
                </div>
              </div>
            ))
          )}
        </div>
      </SectionBlock>

      <SectionBlock title="分组绑定">
        <div className="space-y-2">
          {groupBindings.length === 0 ? (
            <div className="rounded-[var(--surface-radius)] border border-border bg-surface p-3 text-sm text-muted-foreground shadow-[var(--surface-shadow)]">暂无分组绑定事实。</div>
          ) : (
            groupBindings.map((binding) => (
              <div
                key={binding.id}
                className="grid grid-cols-[minmax(0,1fr)_5rem_6rem_7rem] items-center gap-2 rounded-[var(--surface-radius)] border border-border bg-surface px-3 py-2 text-xs shadow-[var(--surface-shadow)]"
              >
                <span className="truncate font-medium text-foreground">{binding.groupName}</span>
                <span>{formatMultiplier(binding.effectiveRateMultiplier ?? binding.defaultRateMultiplier)}</span>
                <StatusBadge tone={binding.bindingStatus === "missing" ? "warning" : "info"}>
                  {groupBindingStatusLabel(binding.bindingStatus)}
                </StatusBadge>
                <span className="truncate text-muted-foreground">{binding.rateSource ?? "未知"}</span>
              </div>
            ))
          )}
        </div>
      </SectionBlock>

      <SectionBlock title="倍率历史">
        <div className="space-y-2">
          {rateRecords.length === 0 ? (
            <div className="rounded-[var(--surface-radius)] border border-border bg-surface p-3 text-sm text-muted-foreground shadow-[var(--surface-shadow)]">暂无倍率历史。</div>
          ) : (
            rateRecords.slice(0, 8).map((record) => (
              <div
                key={record.id}
                className="grid grid-cols-[minmax(0,1fr)_5rem_7rem] items-center gap-2 rounded-[var(--surface-radius)] border border-border bg-surface px-3 py-2 text-xs shadow-[var(--surface-shadow)]"
              >
                <span className="truncate font-medium text-foreground">{record.groupName}</span>
                <span>{formatMultiplier(record.effectiveRateMultiplier)}</span>
                <span className="truncate text-muted-foreground">{formatNullableTime(record.checkedAt)}</span>
              </div>
            ))
          )}
        </div>
      </SectionBlock>

      <SectionBlock title="采集任务">
        <div className="space-y-2">
          {collectorRuns.length === 0 ? (
            <div className="rounded-[var(--surface-radius)] border border-border bg-surface p-3 text-sm text-muted-foreground shadow-[var(--surface-shadow)]">暂无采集任务。</div>
          ) : (
            collectorRuns.slice(0, 8).map((run) => (
              <div
                key={run.id}
                className="grid grid-cols-[5rem_6rem_minmax(0,1fr)_5rem] items-center gap-2 rounded-[var(--surface-radius)] border border-border bg-surface px-3 py-2 text-xs shadow-[var(--surface-shadow)]"
              >
                <span className="font-medium text-foreground">{collectorTaskTypeLabel(run.taskType)}</span>
                <StatusBadge tone={run.status === "success" ? "healthy" : run.status === "failed" ? "error" : run.status === "manual_required" ? "warning" : "info"}>
                  {collectorRunStatusLabel(run.status)}
                </StatusBadge>
                <span className="truncate text-muted-foreground">{run.errorMessage ?? `${run.successCount}/${run.endpointCount} 接口`}</span>
                <span className="text-right text-muted-foreground">{run.durationMs == null ? "-" : `${run.durationMs}ms`}</span>
              </div>
            ))
          )}
        </div>
      </SectionBlock>

      <SectionBlock title="最新采集快照">
        {snapshot ? (
          <div className="space-y-2 rounded-[var(--surface-radius)] border border-border bg-surface p-3 text-sm shadow-[var(--surface-shadow)]">
            <PropertyList>
              <PropertyRow label="来源" value={snapshot.source} />
              <PropertyRow label="状态" value={collectorRunStatusLabel(snapshot.status)} />
              <PropertyRow label="采集时间" value={snapshot.fetchedAt} />
              <PropertyRow label="错误" value={snapshot.errorMessage ?? "无"} />
            </PropertyList>
            <pre className="max-h-40 overflow-auto rounded-[var(--surface-radius)] border border-border bg-surface p-3 text-[11px] text-muted-foreground">{JSON.stringify(snapshot.summaryJson, null, 2)}</pre>
            <div className="text-xs text-muted-foreground">历史快照：{snapshots.length} 条</div>
          </div>
        ) : (
          <div className="rounded-[var(--surface-radius)] border border-border bg-surface p-3 text-sm text-muted-foreground shadow-[var(--surface-shadow)]">暂无快照。</div>
        )}
      </SectionBlock>

      <SectionBlock title="关联变更">
        {changeEvents.length === 0 ? (
          <div className="rounded-[var(--surface-radius)] border border-border bg-surface p-3 text-sm text-muted-foreground shadow-[var(--surface-shadow)]">暂无关联变更。</div>
        ) : (
          <div className="space-y-2">
            {changeEvents.slice(0, 6).map((event) => (
              <div key={event.id} className="rounded-[var(--surface-radius)] border border-border bg-surface p-3 text-sm shadow-[var(--surface-shadow)]">
                <div className="flex items-center justify-between gap-2">
                  <span className="font-medium text-foreground">{event.title}</span>
                  <StatusBadge tone={event.severity === "critical" ? "error" : event.severity === "warning" ? "warning" : "info"}>
                    {event.severity === "critical" ? "严重" : event.severity === "warning" ? "警告" : "信息"}
                  </StatusBadge>
                </div>
                <div className="mt-1 text-xs text-muted-foreground">{event.message}</div>
              </div>
            ))}
          </div>
        )}
      </SectionBlock>

      <div className="rounded-[var(--surface-radius)] border border-border bg-surface p-3 text-xs leading-5 text-foreground shadow-[var(--surface-shadow)]">
        登录账号用于信息采集；保存的密码会加密存储，采集快照和使用记录会统一脱敏。
      </div>
    </div>
  );
}

function SectionBlock({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="rounded-[var(--surface-radius)] border border-border bg-surface p-3 shadow-[var(--surface-shadow)]">
      <div className="mb-2 flex items-center gap-2 text-sm font-semibold text-foreground">
        <ShieldCheck className="h-4 w-4 text-primary" />
        {title}
      </div>
      {children}
    </section>
  );
}

function KeyDialog({
  actionSaving,
  keyForm,
  onKeyDialogOpenChange,
  onKeyFormChange,
  onKeySave,
}: {
  actionSaving: boolean;
  keyForm: StationKeyFormState;
  onKeyDialogOpenChange: (next: boolean) => void;
  onKeyFormChange: (next: StationKeyFormState) => void;
  onKeySave: (event: FormEvent<HTMLFormElement>) => void;
}) {
  return (
    <Dialog
      open
      title={keyForm.id ? "编辑密钥" : "新增密钥"}
      onClose={() => onKeyDialogOpenChange(false)}
      footer={
        <div className="flex justify-end gap-2">
          <Button variant="outline" onClick={() => onKeyDialogOpenChange(false)}>取消</Button>
          <Button type="submit" form="station-key-form" disabled={actionSaving}>{actionSaving ? "保存中" : "保存"}</Button>
        </div>
      }
    >
      <form id="station-key-form" className="grid gap-4 p-5" onSubmit={onKeySave}>
        <div className="grid gap-3 md:grid-cols-2">
          <Field label="名称">
            <input className={inputClassName} value={keyForm.name} onChange={(event) => onKeyFormChange({ ...keyForm, name: event.target.value })} required />
          </Field>
          <Field label="优先级">
            <input className={inputClassName} type="number" value={keyForm.priority} onChange={(event) => onKeyFormChange({ ...keyForm, priority: event.target.value })} />
          </Field>
        </div>
        <Field label="密钥">
          <input className={inputClassName} value={keyForm.apiKey} onChange={(event) => onKeyFormChange({ ...keyForm, apiKey: event.target.value })} placeholder={keyForm.id ? "留空保留旧密钥" : "sk-..."} required={!keyForm.id} />
        </Field>
        <div className="grid gap-3 md:grid-cols-3">
          <Field label="分组">
            <input className={inputClassName} value={keyForm.groupName} onChange={(event) => onKeyFormChange({ ...keyForm, groupName: event.target.value })} />
          </Field>
          <Field label="档位">
            <input className={inputClassName} value={keyForm.tierLabel} onChange={(event) => onKeyFormChange({ ...keyForm, tierLabel: event.target.value })} />
          </Field>
          <Field label="状态">
            <SelectControl
              ariaLabel="密钥状态"
              className={inputClassName}
              value={keyForm.status}
              options={Object.entries(stationKeyStatusLabels).map(([value, label]) => ({
                value: value as StationKeyStatus,
                label,
              }))}
              onChange={(status) => onKeyFormChange({ ...keyForm, status })}
            />
          </Field>
        </div>
        <label className="flex items-center gap-2 text-sm text-foreground">
          <input checked={keyForm.enabled} className="h-4 w-4 accent-primary" type="checkbox" onChange={(event) => onKeyFormChange({ ...keyForm, enabled: event.target.checked })} />
          启用
        </label>
        <Field label="备注">
          <textarea className={`${inputClassName} min-h-20 resize-none py-2`} value={keyForm.note} onChange={(event) => onKeyFormChange({ ...keyForm, note: event.target.value })} />
        </Field>
      </form>
    </Dialog>
  );
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="grid gap-1.5 text-xs font-medium text-muted-foreground">
      {label}
      {children}
    </label>
  );
}

function formToInput(form: StationFormState): StationInput {
  return {
    name: form.name.trim(),
    stationType: form.stationType,
    websiteUrl: form.websiteUrl.trim(),
    apiBaseUrl: form.apiBaseUrl.trim(),
    apiKey: form.apiKey.trim(),
    collectorProxyMode: "inherit",
    collectorProxyUrl: null,
    enabled: form.enabled,
    creditPerCny: Number(form.creditPerCny),
    lowBalanceThresholdCny: form.lowBalanceThresholdCny.trim() ? Number(form.lowBalanceThresholdCny) : null,
    collectionIntervalMinutes: normalizeCollectionIntervalMinutes(form.collectionIntervalMinutes),
    note: form.note.trim() ? form.note.trim() : null,
  };
}

function normalizeCollectionIntervalMinutes(value: string) {
  const interval = Number(value.trim() || "5");
  return Number.isInteger(interval) && interval > 0 ? interval : 5;
}

function toCreateKeyInput(form: StationKeyFormState, stationId: string): CreateStationKeyInput {
  return {
    stationId,
    name: form.name.trim(),
    apiKey: form.apiKey.trim(),
    enabled: form.enabled,
    priority: Number(form.priority),
    groupName: form.groupName.trim() ? form.groupName.trim() : null,
    tierLabel: form.tierLabel.trim() ? form.tierLabel.trim() : null,
    note: form.note.trim() ? form.note.trim() : null,
  };
}

function toUpdateKeyInput(form: StationKeyFormState, stationId: string): UpdateStationKeyInput {
  return {
    id: form.id ?? "",
    stationId,
    name: form.name.trim(),
    apiKey: form.apiKey.trim() ? form.apiKey.trim() : null,
    enabled: form.enabled,
    priority: Number(form.priority),
    groupName: form.groupName.trim() ? form.groupName.trim() : null,
    tierLabel: form.tierLabel.trim() ? form.tierLabel.trim() : null,
    status: form.status,
    note: form.note.trim() ? form.note.trim() : null,
  };
}

function keyToForm(key: StationKey): StationKeyFormState {
  return {
    id: key.id,
    name: key.name,
    apiKey: "",
    enabled: key.enabled,
    priority: String(key.priority),
    groupName: key.groupName ?? "",
    tierLabel: key.tierLabel ?? "",
    status: key.status,
    note: key.note ?? "",
  };
}


function stationAvatarLabel(name: string) {
  const trimmed = name.trim();
  return trimmed ? Array.from(trimmed)[0] : "?";
}

function formatStationDisplayUrl(baseUrl: string) {
  try {
    const url = new URL(baseUrl);
    return `${url.protocol}//${url.host}`;
  } catch {
    return baseUrl.replace(/\/+$/, "");
  }
}

function stationEndpointOriginWarnings(station: Station, form: StationFormState) {
  const warnings: string[] = [];
  if (endpointOriginKey(station.websiteUrl) !== endpointOriginKey(form.websiteUrl)) {
    warnings.push("前端网址 origin 变化后，保存的登录状态会被清除。");
  }
  if (endpointOriginKey(station.apiBaseUrl) !== endpointOriginKey(form.apiBaseUrl)) {
    warnings.push("API origin 变化后，站点会被禁用，现有 Key 将不会路由，直到重新验证并启用。");
  }
  return warnings;
}

function endpointOriginKey(value: string) {
  try {
    const url = new URL(value.trim());
    return `${url.protocol}//${url.host}`;
  } catch {
    return value.trim().replace(/\/+$/, "");
  }
}

function formatStationBalanceParts(row: StationAssetRow) {
  const value = row.latestBalance?.value ?? row.station.balanceCny;
  if (value == null) {
    return { amount: "未采集", currency: "" };
  }
  return {
    amount: value.toFixed(2),
    currency: row.latestBalance?.currency ?? "CNY",
  };
}

function formatRelativeTime(value: string | null) {
  if (!value) {
    return "未采集";
  }
  const date = parseTimestampLikeDate(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  const diffMs = Math.max(0, Date.now() - date.getTime());
  const minute = 60 * 1000;
  const hour = 60 * minute;
  const day = 24 * hour;
  if (diffMs < minute) {
    return "刚刚";
  }
  if (diffMs < hour) {
    return `${Math.floor(diffMs / minute)} 分钟前`;
  }
  if (diffMs < day) {
    return `${Math.floor(diffMs / hour)} 小时前`;
  }
  return `${Math.floor(diffMs / day)} 天前`;
}

function stationIssueTagClassName(tone: "info" | "warning" | "error" | "disabled") {
  if (tone === "error") return "border-danger-border bg-danger-surface text-danger-foreground";
  if (tone === "warning") return "border-warning-border bg-warning-surface text-warning-foreground";
  if (tone === "disabled") return "border-border bg-muted text-muted-foreground";
  return "border-info-border bg-info-surface text-info-foreground";
}

const stationAssetSelectClassName =
  "h-8 min-w-[148px] rounded-[12px] border border-border bg-surface px-3 text-sm text-foreground shadow-surface outline-none transition focus:border-ring focus:ring-2 focus:ring-ring/30";

function formatNullableTime(value: string | null) {
  if (!value) {
    return "未记录";
  }
  const date = parseTimestampLikeDate(value);
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

function formatMultiplier(value: number | null | undefined) {
  return typeof value === "number" && Number.isFinite(value) ? `${value.toFixed(2)}x` : "-";
}

function collectorTaskTypeLabel(value: string) {
  if (value === "detect") return "探测";
  if (value === "balance") return "余额";
  if (value === "groups") return "分组";
  if (value === "models") return "模型";
  if (value === "full") return "完整";
  return value;
}

function collectorRunStatusLabel(status: string) {
  if (status === "success") return "成功";
  if (status === "failed") return "失败";
  if (status === "manual_required") return "需要登录";
  if (status === "running") return "运行中";
  if (status === "partial") return "部分完成";
  return status;
}

function groupBindingStatusLabel(status: string) {
  if (status === "available") return "可用";
  if (status === "missing") return "缺失";
  if (status === "manual") return "手动";
  return status;
}

const inputClassName = "h-8 rounded-[12px] border border-info-border bg-info-surface px-3 text-sm text-foreground outline-none transition focus:border-ring focus:bg-surface focus:ring-2 focus:ring-ring/20";
