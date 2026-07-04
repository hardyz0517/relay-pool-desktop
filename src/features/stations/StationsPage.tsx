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
import { SortableContext, useSortable, verticalListSortingStrategy } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { ArrowRight, Clock3, Edit3, GripVertical, Plus, RefreshCw, ShieldCheck, Trash2, X } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import { Button, Dialog, EmptyState, IconButton, MaskedSecret, ObjectRow, PropertyList, PropertyRow, SelectControl, StatusBadge, type StatusTone, useToast } from "@/components/ui";
import { createStation, deleteStation, listStations, reorderStations, updateStation } from "@/lib/api/stations";
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
  detectSub2apiStation,
  getLatestCollectorSnapshot,
  listCollectorSnapshots,
} from "@/lib/api/collector";
import { listCollectorRuns } from "@/lib/api/collectorRuns";
import { listChangeEvents } from "@/lib/api/changeEvents";
import { listBalanceSnapshots } from "@/lib/api/economics";
import { listGroupRateRecords, listStationGroupBindings } from "@/lib/api/groupFacts";
import type { ChangeEvent } from "@/lib/types/changeEvents";
import { stationKeyStatusLabels, type CreateStationKeyInput, type StationCredentials, type StationKey, type StationKeyStatus, type UpdateStationKeyInput } from "@/lib/types/stationKeys";
import type { CollectorSnapshot } from "@/lib/types/collector";
import type { CollectorRun } from "@/lib/types/collectorRuns";
import type { BalanceSnapshot } from "@/lib/types/economics";
import type { GroupRateRecord, StationGroupBinding } from "@/lib/types/groupFacts";
import { stationStatusLabels, stationTypeLabels, type Station, type StationInput, type StationType } from "@/lib/types/stations";
import { cn } from "@/lib/utils";
import { StationStatusDot } from "./components/StationStatusDot";
import {
  buildStationAssetRows,
  formatStationBalance,
  stationRiskTone,
  type StationAssetRow,
} from "./stationAssetViewModels";

type StationFormState = {
  name: string;
  stationType: StationType;
  baseUrl: string;
  apiKey: string;
  enabled: boolean;
  creditPerCny: string;
  lowBalanceThresholdCny: string;
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

const emptyForm: StationFormState = {
  name: "",
  stationType: "sub2api",
  baseUrl: "",
  apiKey: "",
  enabled: true,
  creditPerCny: "1",
  lowBalanceThresholdCny: "",
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

type StationsPageProps = {
  onAddProvider?: () => void;
};

export function StationsPage({ onAddProvider }: StationsPageProps) {
  const toast = useToast();
  const [stations, setStations] = useState<Station[]>([]);
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
  const [assetSnapshotsByStation, setAssetSnapshotsByStation] = useState<
    Map<string, CollectorSnapshot | null>
  >(new Map());
  const [groupBindingsByStation, setGroupBindingsByStation] = useState(new Map<string, StationGroupBinding[]>());
  const [rateRecordsByStation, setRateRecordsByStation] = useState(new Map<string, GroupRateRecord[]>());
  const [collectorRunsByStation, setCollectorRunsByStation] = useState(new Map<string, CollectorRun[]>());
  const [balanceSnapshots, setBalanceSnapshots] = useState<BalanceSnapshot[]>([]);
  const [changeEvents, setChangeEvents] = useState<ChangeEvent[]>([]);
  const [drawerStationId, setDrawerStationId] = useState<string | null>(null);
  const [drawerVisible, setDrawerVisible] = useState(false);
  const [drawerClosing, setDrawerClosing] = useState(false);
  const [keyDialogOpen, setKeyDialogOpen] = useState(false);
  const [keyForm, setKeyForm] = useState<StationKeyFormState>(emptyKeyForm);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [actionSaving, setActionSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const sensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 8 } }));

  useEffect(() => {
    void refreshStations();
  }, []);

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

  const stationIds = useMemo(() => stations.map((station) => station.id), [stations]);
  const selectedStation = useMemo(
    () => stations.find((station) => station.id === selectedStationId) ?? stations[0] ?? null,
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
  const enabledCount = useMemo(() => stations.filter((station) => station.enabled).length, [stations]);
  const attentionCount = useMemo(
    () => stations.filter((station) => station.status === "warning" || station.status === "error").length,
    [stations],
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
        snapshotsByStation,
        groupBindingsByStation,
        changes: changeEvents,
      }),
    [balanceSnapshots, changeEvents, groupBindingsByStation, keysByStation, snapshotsByStation, stations],
  );

  useEffect(() => {
    if (!activeDialogStation) {
      return;
    }
    void refreshExtras(activeDialogStation.id);
  }, [activeDialogStation?.id]);

  async function refreshStations() {
    setLoading(true);
    setError(null);
    try {
      const [nextStations, nextBalances, nextChanges] = await Promise.all([
        listStations(),
        listBalanceSnapshots(),
        listChangeEvents(),
      ]);
      const nextSnapshotEntries = await Promise.all(
        nextStations.map(async (station) => {
          try {
            return [station.id, await getLatestCollectorSnapshot(station.id)] as const;
          } catch {
            return [station.id, null] as const;
          }
        }),
      );
      setStations(nextStations);
      setBalanceSnapshots(nextBalances);
      setChangeEvents(nextChanges);
      setAssetSnapshotsByStation(new Map(nextSnapshotEntries));
      setSelectedStationId((current) => {
        if (current && nextStations.some((station) => station.id === current)) {
          return current;
        }
        return nextStations[0]?.id ?? null;
      });
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      toast.error("读取中转站失败", message);
    } finally {
      setLoading(false);
    }
  }

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
      setAssetSnapshotsByStation((current) => {
        const next = new Map(current);
        next.set(stationId, nextSnapshot);
        return next;
      });
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
  }, [dialogMode]);

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
    setDialogMode("edit");
    setEditingStationId(station.id);
    setDetailStationId(null);
    setForm({
      name: station.name,
      stationType: station.stationType,
      baseUrl: station.baseUrl,
      apiKey: "",
      enabled: station.enabled,
      creditPerCny: String(station.creditPerCny),
      lowBalanceThresholdCny: station.lowBalanceThresholdCny === null ? "" : String(station.lowBalanceThresholdCny),
      note: station.note ?? "",
      loginUsername: "",
      loginPassword: "",
      rememberPassword: false,
    });
    setError(null);
  }, []);

  const openDetail = useCallback((station: Station) => {
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
  }, [drawerStationId, refreshExtras]);

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
    setKeyForm(emptyKeyForm);
  }, []);

  const handleSelect = useCallback((station: Station) => {
    setSelectedStationId(station.id);
  }, []);

  const handleToggleEnabled = useCallback(async (station: Station) => {
    setError(null);
    try {
      await updateStation({
        id: station.id,
        name: station.name,
        stationType: station.stationType,
        baseUrl: station.baseUrl,
        apiKey: null,
        enabled: !station.enabled,
        creditPerCny: station.creditPerCny,
        lowBalanceThresholdCny: station.lowBalanceThresholdCny,
        note: station.note,
      });
      await refreshStations();
      toast.success(station.enabled ? "站点已禁用" : "站点已启用");
    } catch (requestError) {
      toast.error("更新站点状态失败", readError(requestError));
    }
  }, []);

  const handleDelete = useCallback(async (station: Station) => {
    if (!window.confirm(`确认删除站点「${station.name}」？`)) {
      return;
    }
    setError(null);
    try {
      await deleteStation(station.id);
      await refreshStations();
      toast.success("站点已删除");
    } catch (requestError) {
      toast.error("删除站点失败", readError(requestError));
    }
  }, []);

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
    setStations(nextStations);
    try {
      const savedStations = await reorderStations(nextStations.map((station) => station.id));
      setStations(savedStations);
      toast.success("站点排序已保存");
    } catch (requestError) {
      setStations(previousStations);
      toast.error("保存站点排序失败", readError(requestError));
    }
  }

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setSaving(true);
    setError(null);
    try {
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
        toast.success("站点已创建");
      }
      await refreshStations();
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

  async function handleRunDetect() {
    if (!selectedStation) {
      return;
    }
    setActionSaving(true);
    setError(null);
    try {
      await detectSub2apiStation(selectedStation.id);
      await refreshStations();
      await refreshExtras(selectedStation.id);
      toast.success("已执行站点探测");
    } catch (requestError) {
      toast.error("站点探测失败", readError(requestError));
    } finally {
      setActionSaving(false);
    }
  }

  async function handleRunCollect(station = selectedStation) {
    if (!station) {
      return;
    }
    setActionSaving(true);
    setError(null);
    try {
      await collectSub2apiStation(station.id);
      await refreshStations();
      if (station.id === selectedStationId || station.id === drawerStationId) {
        await refreshExtras(station.id);
      }
      await refreshStationFacts(station.id);
      toast.success("已保存采集快照");
    } catch (requestError) {
      toast.error("保存采集快照失败", readError(requestError));
    } finally {
      setActionSaving(false);
    }
  }

  async function handleRefreshBalance(station: Station) {
    setActionSaving(true);
    setError(null);
    try {
      await collectStationTask(station.id, "balance");
      await refreshStations();
      if (station.id === selectedStationId || station.id === drawerStationId) {
        await refreshExtras(station.id);
      }
      await refreshStationFacts(station.id);
      toast.success("余额已刷新");
    } catch (requestError) {
      toast.error("刷新余额失败", readError(requestError));
    } finally {
      setActionSaving(false);
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
      if (keyForm.id) {
        await updateStationKey(toUpdateKeyInput(keyForm, activeDialogStation.id));
      } else {
        await createStationKey(toCreateKeyInput(keyForm, activeDialogStation.id));
      }
      setKeyDialogOpen(false);
      setKeyForm(emptyKeyForm);
      await refreshExtras(activeDialogStation.id);
      toast.success("密钥已保存");
    } catch (requestError) {
      toast.error("保存密钥失败", readError(requestError));
    } finally {
      setActionSaving(false);
    }
  }

  async function handleDeleteKey(key: StationKey) {
    if (!activeDialogStation) {
      return;
    }
    if (!window.confirm(`确认删除密钥「${key.name}」？`)) {
      return;
    }
    setActionSaving(true);
    try {
      await deleteStationKey(key.id);
      await refreshExtras(activeDialogStation.id);
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
      actions={
        <Button onClick={onAddProvider ?? openCreate}>
          <Plus className="h-4 w-4" />
          添加供应商
        </Button>
      }
    >
      <div className="grid min-w-0 gap-3">
        <div className="flex min-h-8 flex-wrap items-center justify-between gap-3">
          <div className="min-w-0">
            <div className="text-[13px] font-semibold text-slate-800">中转站列表</div>
            <div className="text-xs text-muted-foreground">
              {stations.length} 个站点，{enabledCount} 个启用，{attentionCount} 个需关注。
            </div>
          </div>
        </div>

        <div>
          {loading ? (
            <div className="rounded-[var(--surface-radius)] border border-border bg-white px-4 py-5 text-sm text-muted-foreground shadow-[var(--surface-shadow)]">
              正在读取本地数据...
            </div>
          ) : stations.length === 0 ? (
            <EmptyState
              title="还没有中转站"
              description="添加一个站点开始管理登录账号和多把密钥。"
              action={<Button onClick={onAddProvider ?? openCreate}>添加供应商</Button>}
            />
          ) : (
            <div className="space-y-2">
              {stationAssetRows.map((row) => (
                <StationAssetListRow
                  key={row.station.id}
                  actionSaving={actionSaving}
                  active={row.station.id === selectedStation?.id}
                  row={row}
                  onCollect={(station) => void handleRunCollect(station)}
                  onDelete={handleDelete}
                  onEdit={openEdit}
                  onOpen={openDetail}
                  onRefreshBalance={(station) => void handleRefreshBalance(station)}
                />
              ))}
            </div>
          )}
        </div>
      </div>

      {drawerStationId && detailStation && (
        <div
          className={cn(
            "fixed inset-0 z-40 bg-slate-900/0 transition-colors duration-200 ease-out",
            drawerVisible && !drawerClosing && "bg-slate-900/10",
          )}
          onMouseDown={closeDrawer}
        >
          <div
            className={cn(
              "absolute inset-y-0 right-0 w-[min(560px,calc(100vw-72px))] border-l border-border bg-white shadow-[0_24px_80px_rgba(15,23,42,0.18)] transition-transform duration-[220ms] ease-[cubic-bezier(0.22,1,0.36,1)] will-change-transform",
              drawerVisible && !drawerClosing ? "translate-x-0" : "translate-x-full",
            )}
            onMouseDown={(event) => event.stopPropagation()}
          >
            <div className="flex h-full min-h-0 flex-col">
              <div className="flex items-start justify-between gap-3 border-b border-border px-4 py-3">
                <div className="min-w-0">
                  <div className="truncate text-sm font-semibold text-slate-900">{detailStation.name}</div>
                  <div className="truncate text-xs text-muted-foreground">{detailStation.baseUrl}</div>
                </div>
                <IconButton
                  className="shrink-0 text-slate-500 transition-colors hover:bg-slate-100 hover:text-slate-950 active:bg-slate-200"
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
    </PageScaffold>
  );
}

function StationAssetListRow({
  row,
  active,
  actionSaving,
  onOpen,
  onEdit,
  onCollect,
  onDelete,
  onRefreshBalance,
}: {
  row: StationAssetRow;
  active: boolean;
  actionSaving: boolean;
  onOpen: (station: Station) => void;
  onEdit: (station: Station) => void;
  onCollect: (station: Station) => void;
  onDelete: (station: Station) => void;
  onRefreshBalance: (station: Station) => void;
}) {
  const station = row.station;
  const statusToneValue = stationRiskTone(row);
  const balance = formatStationBalanceParts(row);
  const lastCollectText = formatRelativeTime(station.lastPricingFetchedAt ?? station.updatedAt);

  return (
    <div
      role="button"
      tabIndex={0}
      aria-pressed={active}
      className={cn(
        "group flex min-h-[78px] w-full cursor-pointer items-center gap-3 rounded-[14px] border px-4 py-3 text-left transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[hsl(var(--accent)/0.28)]",
        active
          ? "border-[hsl(var(--accent))] bg-[linear-gradient(90deg,#eff6ff_0%,#f7fbff_42%,#ffffff_100%)]"
          : "border-border bg-white hover:border-sky-200 hover:bg-slate-50/60",
      )}
      onClick={() => onOpen(station)}
      onKeyDown={(event) => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          onOpen(station);
        }
      }}
    >
      <GripVertical className="h-4 w-4 shrink-0 text-slate-300 transition-colors group-hover:text-slate-400" />
      <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-full border border-slate-200 bg-white text-xs font-semibold text-slate-600 shadow-[0_1px_2px_rgba(15,23,42,0.04)]">
        {stationAvatarLabel(station.name)}
      </div>

      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 items-center gap-2">
          <div className="truncate text-[15px] font-semibold leading-5 text-slate-950">{station.name}</div>
          <span className="hidden rounded-full border border-slate-200 bg-white/80 px-2 py-0.5 text-[11px] font-medium leading-4 text-slate-500 sm:inline-flex">
            {stationTypeLabels[station.stationType]}
          </span>
          <span className={cn("h-1.5 w-1.5 shrink-0 rounded-full", statusDotClassName(statusToneValue))} />
        </div>
        <div className="mt-1 truncate text-xs font-medium text-[hsl(var(--accent))]">
          {formatStationDisplayUrl(station.baseUrl)}
        </div>
      </div>

      <div className="hidden shrink-0 items-center gap-5 md:flex">
        <div className="min-w-[78px] text-right">
          <div className="flex items-center justify-end gap-1 text-[11px] leading-4 text-slate-400">
            <Clock3 className="h-3 w-3" />
            <span>{lastCollectText}</span>
            <button
              type="button"
              aria-label={`刷新余额 ${station.name}`}
              title={`刷新余额 ${station.name}`}
              disabled={actionSaving || !station.enabled}
              className="ml-1 inline-flex h-5 w-5 cursor-pointer items-center justify-center rounded-[6px] text-slate-400 transition-colors hover:bg-slate-100 hover:text-slate-700 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[hsl(var(--accent)/0.28)] disabled:cursor-default disabled:opacity-40"
              onClick={(event) => {
                event.stopPropagation();
                onRefreshBalance(station);
              }}
              onKeyDown={(event) => {
                event.stopPropagation();
              }}
            >
              <RefreshCw className={cn("h-3.5 w-3.5", actionSaving && "animate-spin")} />
            </button>
          </div>
          <div className="mt-1 text-xs leading-4 text-slate-500">
            余额：
            <span className={cn("font-semibold", balance.amount === "未采集" ? "text-slate-500" : "text-emerald-600")}>
              {balance.amount}
            </span>
            {balance.currency && <span className="ml-1 text-slate-500">{balance.currency}</span>}
          </div>
        </div>
      </div>

      <div
        className="flex shrink-0 items-center gap-1 opacity-100 transition-opacity md:opacity-0 md:group-hover:opacity-100 md:group-focus-within:opacity-100"
        onClick={(event) => event.stopPropagation()}
        onKeyDown={(event) => event.stopPropagation()}
      >
        <IconButton className="text-slate-500 hover:text-slate-900" label={`编辑 ${station.name}`} onClick={() => onEdit(station)}>
          <Edit3 className="h-4 w-4" />
        </IconButton>
        <IconButton
          className="text-slate-500 hover:text-slate-900"
          disabled={actionSaving || !station.enabled}
          label={`采集信息 ${station.name}`}
          onClick={() => onCollect(station)}
        >
          <RefreshCw className={cn("h-4 w-4", actionSaving && "animate-spin")} />
        </IconButton>
        <IconButton
          className="text-slate-400 hover:bg-rose-50 hover:text-rose-600"
          label={`删除 ${station.name}`}
          onClick={() => onDelete(station)}
        >
          <Trash2 className="h-4 w-4" />
        </IconButton>
      </div>
    </div>
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
  onDeleteKey: (key: StationKey) => Promise<void>;
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
                options={Object.entries(stationTypeLabels).map(([value, label]) => ({
                  value: value as StationType,
                  label,
                }))}
                onChange={(stationType) => onChange({ ...form, stationType })}
              />
            </Field>
          </div>
          <Field label="基础地址">
            <input className={inputClassName} value={form.baseUrl} onChange={(event) => onChange({ ...form, baseUrl: event.target.value })} placeholder="https://example.com/v1" required />
          </Field>
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
            <label className="flex items-end gap-2 pb-2 text-sm text-slate-700">
              <input checked={form.enabled} className="h-4 w-4 accent-teal-600" type="checkbox" onChange={(event) => onChange({ ...form, enabled: event.target.checked })} />
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
            <div className="mt-2 flex items-center gap-4 text-sm text-slate-700">
              <label className="flex items-center gap-2">
                <input checked={form.rememberPassword} className="h-4 w-4 accent-teal-600" type="checkbox" onChange={(event) => onChange({ ...form, rememberPassword: event.target.checked })} />
                记住密码
              </label>
              <span className="text-xs text-muted-foreground">保存后密码会写入本地加密存储；留空不会覆盖旧密码。</span>
            </div>
            {credentials && (
              <div className="mt-3 rounded-[var(--surface-radius)] border border-border bg-white p-3 text-xs text-slate-700 shadow-[var(--surface-shadow)]">
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
  onDeleteKey: (key: StationKey) => Promise<void>;
  onEditKey: (key: StationKey) => void;
}) {
  return (
    <div className="space-y-4 p-5">
      <PropertyList className="overflow-hidden rounded-[var(--surface-radius)] border border-cyan-100 bg-white/80">
        <PropertyRow label="站点名称" value={activeDialogStation.name} />
        <PropertyRow label="站点类型" value={stationTypeLabels[activeDialogStation.stationType]} />
        <PropertyRow label="基础地址" value={<code className="text-xs">{activeDialogStation.baseUrl}</code>} />
        <PropertyRow label="余额" value={activeDialogStation.balanceCny === null ? "未采集" : `¥${activeDialogStation.balanceCny.toFixed(2)}`} />
        <PropertyRow label="密钥数量" value={keyCountLabel} />
        <PropertyRow label="状态" value={stationStatusLabels[activeDialogStation.status]} />
        <PropertyRow label="采集时间" value={activeDialogStation.lastPricingFetchedAt ?? "未采集"} />
        <PropertyRow label="刷新时间" value={activeDialogStation.lastCheckedAt ?? "未检测"} />
      </PropertyList>

      <SectionBlock title="登录账号">
        {credentials ? (
          <PropertyList className="overflow-hidden rounded-[var(--surface-radius)] border border-cyan-100 bg-white/80">
            <PropertyRow label="登录用户名" value={credentials.loginUsername || "未设置"} />
            <PropertyRow label="密码" value={credentials.passwordPresent ? "已保存" : "未保存"} />
            <PropertyRow label="记住密码" value={credentials.rememberPassword ? "是" : "否"} />
            <PropertyRow label="登录状态" value={credentials.loginStatus} />
            <PropertyRow label="最近登录" value={credentials.lastLoginAt ?? "未登录"} />
            <PropertyRow label="登录错误" value={credentials.loginError ?? "无"} />
          </PropertyList>
        ) : (
          <div className="rounded-[var(--surface-radius)] border border-border bg-white p-3 text-sm text-muted-foreground shadow-[var(--surface-shadow)]">未保存登录账号。</div>
        )}
      </SectionBlock>

      <SectionBlock title="密钥">
        <div className="space-y-2">
          {stationKeys.length === 0 ? (
            <div className="rounded-[var(--surface-radius)] border border-border bg-white p-3 text-sm text-muted-foreground shadow-[var(--surface-shadow)]">暂无密钥。</div>
          ) : (
            stationKeys.map((key) => (
              <div key={key.id} className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-3 rounded-[var(--surface-radius)] border border-border bg-white px-3 py-2.5 shadow-[var(--surface-shadow)]">
                <div className="min-w-0">
                  <div className="flex items-center gap-2">
                    <div className="truncate text-sm font-medium text-slate-800">{key.name}</div>
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
                  <Button variant="danger" onClick={() => void onDeleteKey(key)}>删除</Button>
                </div>
              </div>
            ))
          )}
        </div>
      </SectionBlock>

      <SectionBlock title="分组绑定">
        <div className="space-y-2">
          {groupBindings.length === 0 ? (
            <div className="rounded-[var(--surface-radius)] border border-border bg-white p-3 text-sm text-muted-foreground shadow-[var(--surface-shadow)]">暂无分组绑定事实。</div>
          ) : (
            groupBindings.map((binding) => (
              <div
                key={binding.id}
                className="grid grid-cols-[minmax(0,1fr)_5rem_6rem_7rem] items-center gap-2 rounded-[var(--surface-radius)] border border-border bg-white px-3 py-2 text-xs shadow-[var(--surface-shadow)]"
              >
                <span className="truncate font-medium text-slate-700">{binding.groupName}</span>
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
            <div className="rounded-[var(--surface-radius)] border border-border bg-white p-3 text-sm text-muted-foreground shadow-[var(--surface-shadow)]">暂无倍率历史。</div>
          ) : (
            rateRecords.slice(0, 8).map((record) => (
              <div
                key={record.id}
                className="grid grid-cols-[minmax(0,1fr)_5rem_7rem] items-center gap-2 rounded-[var(--surface-radius)] border border-border bg-white px-3 py-2 text-xs shadow-[var(--surface-shadow)]"
              >
                <span className="truncate font-medium text-slate-700">{record.groupName}</span>
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
            <div className="rounded-[var(--surface-radius)] border border-border bg-white p-3 text-sm text-muted-foreground shadow-[var(--surface-shadow)]">暂无采集任务。</div>
          ) : (
            collectorRuns.slice(0, 8).map((run) => (
              <div
                key={run.id}
                className="grid grid-cols-[5rem_6rem_minmax(0,1fr)_5rem] items-center gap-2 rounded-[var(--surface-radius)] border border-border bg-white px-3 py-2 text-xs shadow-[var(--surface-shadow)]"
              >
                <span className="font-medium text-slate-700">{collectorTaskTypeLabel(run.taskType)}</span>
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
          <div className="space-y-2 rounded-[var(--surface-radius)] border border-border bg-white p-3 text-sm shadow-[var(--surface-shadow)]">
            <PropertyList>
              <PropertyRow label="来源" value={snapshot.source} />
              <PropertyRow label="状态" value={collectorRunStatusLabel(snapshot.status)} />
              <PropertyRow label="采集时间" value={snapshot.fetchedAt} />
              <PropertyRow label="错误" value={snapshot.errorMessage ?? "无"} />
            </PropertyList>
            <pre className="max-h-40 overflow-auto rounded-[var(--surface-radius)] border border-border bg-white p-3 text-[11px] text-slate-600">{JSON.stringify(snapshot.summaryJson, null, 2)}</pre>
            <div className="text-xs text-muted-foreground">历史快照：{snapshots.length} 条</div>
          </div>
        ) : (
          <div className="rounded-[var(--surface-radius)] border border-border bg-white p-3 text-sm text-muted-foreground shadow-[var(--surface-shadow)]">暂无快照。</div>
        )}
      </SectionBlock>

      <SectionBlock title="关联变更">
        {changeEvents.length === 0 ? (
          <div className="rounded-[var(--surface-radius)] border border-border bg-white p-3 text-sm text-muted-foreground shadow-[var(--surface-shadow)]">暂无关联变更。</div>
        ) : (
          <div className="space-y-2">
            {changeEvents.slice(0, 6).map((event) => (
              <div key={event.id} className="rounded-[var(--surface-radius)] border border-border bg-white p-3 text-sm shadow-[var(--surface-shadow)]">
                <div className="flex items-center justify-between gap-2">
                  <span className="font-medium text-slate-800">{event.title}</span>
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

      <div className="rounded-[var(--surface-radius)] border border-border bg-white p-3 text-xs leading-5 text-slate-700 shadow-[var(--surface-shadow)]">
        登录账号用于信息采集；保存的密码会加密存储，采集快照和请求日志会统一脱敏。
      </div>
    </div>
  );
}

function SectionBlock({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="rounded-[var(--surface-radius)] border border-border bg-white p-3 shadow-[var(--surface-shadow)]">
      <div className="mb-2 flex items-center gap-2 text-sm font-semibold text-slate-800">
        <ShieldCheck className="h-4 w-4 text-teal-600" />
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
        <label className="flex items-center gap-2 text-sm text-slate-700">
          <input checked={keyForm.enabled} className="h-4 w-4 accent-teal-600" type="checkbox" onChange={(event) => onKeyFormChange({ ...keyForm, enabled: event.target.checked })} />
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
    baseUrl: form.baseUrl.trim(),
    apiKey: form.apiKey.trim(),
    enabled: form.enabled,
    creditPerCny: Number(form.creditPerCny),
    lowBalanceThresholdCny: form.lowBalanceThresholdCny.trim() ? Number(form.lowBalanceThresholdCny) : null,
    note: form.note.trim() ? form.note.trim() : null,
  };
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

function readError(error: unknown) {
  return error instanceof Error ? error.message : String(error);
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
  const numeric = Number(value);
  const date = Number.isFinite(numeric) && numeric > 1000000000000 ? new Date(numeric) : new Date(value);
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

function statusDotClassName(tone: StatusTone) {
  if (tone === "healthy") return "bg-emerald-500";
  if (tone === "warning") return "bg-amber-500";
  if (tone === "error") return "bg-rose-500";
  if (tone === "disabled") return "bg-slate-400";
  return "bg-blue-500";
}

function formatNullableTime(value: string | null) {
  if (!value) {
    return "未记录";
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

const inputClassName = "h-8 rounded-[12px] border border-cyan-100 bg-cyan-50/40 px-3 text-sm text-slate-800 outline-none transition focus:border-teal-300 focus:bg-white focus:ring-2 focus:ring-teal-100";
