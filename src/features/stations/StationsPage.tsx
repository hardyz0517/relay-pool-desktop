import { useCallback, useEffect, useMemo, useState, type FormEvent, type HTMLAttributes, type ReactNode } from "react";
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
import { ArrowRight, Edit3, GripVertical, Plus, RefreshCw, ShieldCheck, Trash2 } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import { Button, Dialog, EmptyState, PropertyList, PropertyRow, StatusBadge, Toolbar } from "@/components/ui";
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
  detectSub2apiStation,
  getLatestCollectorSnapshot,
  listCollectorSnapshots,
} from "@/lib/api/collector";
import { stationKeyStatusLabels, type CreateStationKeyInput, type StationCredentials, type StationKey, type StationKeyStatus, type UpdateStationKeyInput } from "@/lib/types/stationKeys";
import type { CollectorSnapshot } from "@/lib/types/collector";
import { stationStatusLabels, stationTypeLabels, type Station, type StationInput, type StationType } from "@/lib/types/stations";
import { cn } from "@/lib/utils";
import { StationStatusDot } from "./components/StationStatusDot";

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

export function StationsPage() {
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
  const [keyDialogOpen, setKeyDialogOpen] = useState(false);
  const [keyForm, setKeyForm] = useState<StationKeyFormState>(emptyKeyForm);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [actionSaving, setActionSaving] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const sensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 8 } }));

  useEffect(() => {
    void refreshStations();
  }, []);

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
      const nextStations = await listStations();
      setStations(nextStations);
      setSelectedStationId((current) => {
        if (current && nextStations.some((station) => station.id === current)) {
          return current;
        }
        return nextStations[0]?.id ?? null;
      });
    } catch (requestError) {
      setError(readError(requestError));
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
      if (dialogMode === "edit") {
        setForm((current) => ({
          ...current,
          loginUsername: nextCredentials.loginUsername ?? "",
          rememberPassword: nextCredentials.rememberPassword,
        }));
      }
    } catch (requestError) {
      setError(readError(requestError));
    }
  }, [dialogMode]);

  const openCreate = useCallback(() => {
    setDialogMode("create");
    setEditingStationId(null);
    setDetailStationId(null);
    setForm(emptyForm);
    setCredentials(null);
    setStationKeys([]);
    setSnapshots([]);
    setSnapshot(null);
    setMessage(null);
    setError(null);
  }, []);

  const openCreateWithExample = useCallback(() => {
    setDialogMode("create");
    setEditingStationId(null);
    setDetailStationId(null);
    setForm({
      ...emptyForm,
      name: "Orchid Relay",
      baseUrl: "https://api.orchid-relay.example/v1",
      apiKey: "sk-example-change-me",
      note: "示例站点，仅用于验证本地 SQLite 持久化。",
      loginUsername: "user@example.com",
      rememberPassword: true,
    });
    setCredentials(null);
    setStationKeys([]);
    setSnapshots([]);
    setSnapshot(null);
    setMessage(null);
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
    setMessage(null);
    setError(null);
  }, []);

  const openDetail = useCallback((station: Station) => {
    setDialogMode("detail");
    setDetailStationId(station.id);
    setEditingStationId(null);
    setSelectedStationId(station.id);
    setMessage(null);
    setError(null);
  }, []);

  const closeDialog = useCallback(() => {
    setDialogMode(null);
    setEditingStationId(null);
    setDetailStationId(null);
    setForm(emptyForm);
    setCredentials(null);
    setStationKeys([]);
    setSnapshots([]);
    setSnapshot(null);
    setKeyDialogOpen(false);
    setKeyForm(emptyKeyForm);
  }, []);

  const handleSelect = useCallback((station: Station) => {
    setSelectedStationId(station.id);
  }, []);

  const handleToggleEnabled = useCallback(async (station: Station) => {
    setError(null);
    setMessage(null);
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
      setMessage(station.enabled ? "站点已禁用。" : "站点已启用。");
    } catch (requestError) {
      setError(readError(requestError));
    }
  }, []);

  const handleDelete = useCallback(async (station: Station) => {
    if (!window.confirm(`确认删除站点「${station.name}」？`)) {
      return;
    }
    setError(null);
    setMessage(null);
    try {
      await deleteStation(station.id);
      await refreshStations();
      setMessage("站点已删除。");
    } catch (requestError) {
      setError(readError(requestError));
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
      setMessage("站点排序已保存。");
    } catch (requestError) {
      setStations(previousStations);
      setError(readError(requestError));
    }
  }

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setSaving(true);
    setError(null);
    setMessage(null);
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
        setMessage("站点已更新。");
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
        setMessage("站点已创建。");
      }
      await refreshStations();
      closeDialog();
    } catch (requestError) {
      setError(readError(requestError));
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
      setMessage("登录信息已清除。");
    } catch (requestError) {
      setError(readError(requestError));
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
      setMessage("已执行站点探测。");
    } catch (requestError) {
      setError(readError(requestError));
    } finally {
      setActionSaving(false);
    }
  }

  async function handleRunCollect() {
    if (!selectedStation) {
      return;
    }
    setActionSaving(true);
    setError(null);
    try {
      await collectSub2apiStation(selectedStation.id);
      await refreshStations();
      await refreshExtras(selectedStation.id);
      setMessage("已保存采集快照。");
    } catch (requestError) {
      setError(readError(requestError));
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
      setMessage("API Key 已保存。");
    } catch (requestError) {
      setError(readError(requestError));
    } finally {
      setActionSaving(false);
    }
  }

  async function handleDeleteKey(key: StationKey) {
    if (!activeDialogStation) {
      return;
    }
    if (!window.confirm(`确认删除 Key「${key.name}」？`)) {
      return;
    }
    setActionSaving(true);
    try {
      await deleteStationKey(key.id);
      await refreshExtras(activeDialogStation.id);
      setMessage("API Key 已删除。");
    } catch (requestError) {
      setError(readError(requestError));
    } finally {
      setActionSaving(false);
    }
  }

  const keyCountLabel = activeDialogStation ? `${activeDialogStation.keyCount} keys` : "0 keys";

  return (
    <PageScaffold
      title="中转池"
      description="provider 管理、排序、启用/禁用、登录账号与多 key；详情与编辑均在 Dialog 内。"
      actions={
        <Button onClick={openCreate}>
          <Plus className="h-4 w-4" />
          新增站点
        </Button>
      }
    >
      <div className="min-w-0 overflow-hidden rounded-2xl border border-white/70 bg-white/85 shadow-[0_12px_30px_rgba(33,79,88,0.07)]">
        <Toolbar>
          <div className="min-w-0">
            <div className="text-[13px] font-semibold text-slate-800">provider 列表</div>
            <div className="text-xs text-muted-foreground">
              {stations.length} 个站点，{enabledCount} 个启用，{attentionCount} 个需关注。
            </div>
          </div>
          <Button variant="secondary" onClick={openCreateWithExample}>
            <RefreshCw className="h-4 w-4" />
            示例站点
          </Button>
        </Toolbar>

        <div className="p-3">
          {loading ? (
            <div className="rounded-2xl border border-cyan-100 bg-cyan-50/70 px-4 py-5 text-sm text-muted-foreground">
              正在读取本地 SQLite...
            </div>
          ) : stations.length === 0 ? (
            <EmptyState
              title="还没有中转站"
              description="添加一个站点开始管理登录账号和多把 API Key。"
              action={<Button onClick={openCreateWithExample}>添加示例站点</Button>}
            />
          ) : (
            <DndContext sensors={sensors} collisionDetection={closestCenter} onDragStart={handleDragStart} onDragCancel={handleDragCancel} onDragEnd={handleDragEnd}>
              <SortableContext items={stationIds} strategy={verticalListSortingStrategy}>
                <div className="space-y-2">
                  {stations.map((station) => (
                    <SortableStationRow
                      key={station.id}
                      active={station.id === selectedStation?.id}
                      station={station}
                      onDelete={handleDelete}
                      onEdit={openEdit}
                      onPreview={openDetail}
                      onSelect={handleSelect}
                      onToggleEnabled={handleToggleEnabled}
                    />
                  ))}
                </div>
              </SortableContext>
              <DragOverlay dropAnimation={null}>
                {activeDragStation ? (
                  <StationRowContent
                    active
                    overlay
                    station={activeDragStation}
                    onDelete={handleDelete}
                    onEdit={openEdit}
                    onPreview={openDetail}
                    onSelect={handleSelect}
                    onToggleEnabled={handleToggleEnabled}
                  />
                ) : null}
              </DragOverlay>
            </DndContext>
          )}
        </div>
      </div>

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
        <Dialog
          open
          title="站点详情"
          description="只读详情；编辑仍走独立编辑 Dialog。"
          onClose={onClose}
          footer={
            <div className="flex items-center justify-between gap-2">
              <div className="text-xs text-muted-foreground">{keyCountLabel}</div>
              <div className="flex gap-2">
                <Button variant="outline" onClick={onRefreshDetail} disabled={actionSaving}>
                  <RefreshCw className="h-4 w-4" />
                  刷新
                </Button>
                <Button variant="secondary" onClick={() => {
                  onKeyFormChange({ ...emptyKeyForm, priority: String(stationKeys.length) });
                  onKeyDialogOpenChange(true);
                }}>
                  <Plus className="h-4 w-4" />
                  新增 Key
                </Button>
                <Button variant="outline" onClick={onEdit}>
                  <Edit3 className="h-4 w-4" />
                  编辑
                </Button>
                <Button variant="outline" onClick={onClose}>
                  关闭
                </Button>
              </div>
            </div>
          }
        >
          <DetailBody
            activeDialogStation={activeDialogStation}
            credentials={credentials}
            keyCountLabel={keyCountLabel}
            snapshot={snapshot}
            snapshots={snapshots}
            stationKeys={stationKeys}
            onDeleteKey={onDeleteKey}
            onEditKey={(key) => {
              onKeyFormChange(keyToForm(key));
              onKeyDialogOpenChange(true);
            }}
          />
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

  return (
    <>
      <Dialog
        open
        title={dialogMode === "edit" ? "编辑站点" : "新增站点"}
        description={dialogMode === "edit" ? "API Key 留空则保留旧值。登录账号区用于采集。": "新增 provider 记录到本地 SQLite。"}
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
              <select className={inputClassName} value={form.stationType} onChange={(event) => onChange({ ...form, stationType: event.target.value as StationType })}>
                {Object.entries(stationTypeLabels).map(([value, label]) => (
                  <option key={value} value={value}>{label}</option>
                ))}
              </select>
            </Field>
          </div>
          <Field label="Base URL">
            <input className={inputClassName} value={form.baseUrl} onChange={(event) => onChange({ ...form, baseUrl: event.target.value })} placeholder="https://example.com/v1" required />
          </Field>
          <Field label={dialogMode === "edit" ? "API Key（留空保留旧值）" : "API Key"}>
            <input className={inputClassName} value={form.apiKey} onChange={(event) => onChange({ ...form, apiKey: event.target.value })} placeholder={dialogMode === "edit" ? "留空保留旧 key" : "sk-..."} required={dialogMode !== "edit"} />
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
              <span className="text-xs text-muted-foreground">P3 阶段密码暂存本地 SQLite，后续迁移到系统密钥链。</span>
            </div>
            {credentials && (
              <div className="mt-3 rounded-2xl border border-cyan-100 bg-white/80 p-3 text-xs text-slate-700">
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
  credentials,
  keyCountLabel,
  snapshot,
  snapshots,
  stationKeys,
  onDeleteKey,
  onEditKey,
}: {
  activeDialogStation: Station;
  credentials: StationCredentials | null;
  keyCountLabel: string;
  snapshot: CollectorSnapshot | null;
  snapshots: CollectorSnapshot[];
  stationKeys: StationKey[];
  onDeleteKey: (key: StationKey) => Promise<void>;
  onEditKey: (key: StationKey) => void;
}) {
  return (
    <div className="space-y-4 p-5">
      <PropertyList className="overflow-hidden rounded-2xl border border-cyan-100 bg-white/80">
        <PropertyRow label="站点名称" value={activeDialogStation.name} />
        <PropertyRow label="站点类型" value={stationTypeLabels[activeDialogStation.stationType]} />
        <PropertyRow label="Base URL" value={<code className="text-xs">{activeDialogStation.baseUrl}</code>} />
        <PropertyRow label="余额" value={activeDialogStation.balanceCny === null ? "未采集" : `¥${activeDialogStation.balanceCny.toFixed(2)}`} />
        <PropertyRow label="key 数量" value={keyCountLabel} />
        <PropertyRow label="状态" value={stationStatusLabels[activeDialogStation.status]} />
        <PropertyRow label="采集时间" value={activeDialogStation.lastPricingFetchedAt ?? "未采集"} />
        <PropertyRow label="刷新时间" value={activeDialogStation.lastCheckedAt ?? "未检测"} />
      </PropertyList>

      <SectionBlock title="登录账号">
        {credentials ? (
          <PropertyList className="overflow-hidden rounded-2xl border border-cyan-100 bg-white/80">
            <PropertyRow label="登录用户名" value={credentials.loginUsername || "未设置"} />
            <PropertyRow label="密码" value={credentials.passwordPresent ? "已保存" : "未保存"} />
            <PropertyRow label="记住密码" value={credentials.rememberPassword ? "是" : "否"} />
            <PropertyRow label="登录状态" value={credentials.loginStatus} />
            <PropertyRow label="最近登录" value={credentials.lastLoginAt ?? "未登录"} />
            <PropertyRow label="登录错误" value={credentials.loginError ?? "无"} />
          </PropertyList>
        ) : (
          <div className="rounded-2xl border border-cyan-100 bg-cyan-50/50 p-3 text-sm text-muted-foreground">未保存登录账号。</div>
        )}
      </SectionBlock>

      <SectionBlock title="API Keys">
        <div className="space-y-2">
          {stationKeys.length === 0 ? (
            <div className="rounded-2xl border border-cyan-100 bg-cyan-50/50 p-3 text-sm text-muted-foreground">暂无 Key。</div>
          ) : (
            stationKeys.map((key) => (
              <div key={key.id} className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-3 rounded-2xl border border-cyan-100 bg-white/90 px-3 py-2.5">
                <div className="min-w-0">
                  <div className="flex items-center gap-2">
                    <div className="truncate text-sm font-medium text-slate-800">{key.name}</div>
                    <StatusBadge tone={statusTone[key.status]}>{stationKeyStatusLabels[key.status]}</StatusBadge>
                    <span className="text-[11px] text-muted-foreground">P{key.priority}</span>
                  </div>
                  <div className="mt-1 flex flex-wrap gap-2 text-xs text-muted-foreground">
                    <span>{key.apiKeyMasked}</span>
                    <span>{key.groupName ?? "未分组"}</span>
                    <span>{key.tierLabel ?? "无 tier"}</span>
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

      <SectionBlock title="最新采集快照">
        {snapshot ? (
          <div className="space-y-2 rounded-2xl border border-cyan-100 bg-white/80 p-3 text-sm">
            <PropertyList>
              <PropertyRow label="source" value={snapshot.source} />
              <PropertyRow label="status" value={snapshot.status} />
              <PropertyRow label="fetchedAt" value={snapshot.fetchedAt} />
              <PropertyRow label="error" value={snapshot.errorMessage ?? "无"} />
            </PropertyList>
            <pre className="max-h-40 overflow-auto rounded-xl bg-slate-50 p-3 text-[11px] text-slate-600">{JSON.stringify(snapshot.summaryJson, null, 2)}</pre>
            <div className="text-xs text-muted-foreground">历史快照：{snapshots.length} 条</div>
          </div>
        ) : (
          <div className="rounded-2xl border border-cyan-100 bg-cyan-50/50 p-3 text-sm text-muted-foreground">暂无快照。</div>
        )}
      </SectionBlock>

      <div className="rounded-2xl border border-amber-200 bg-amber-50/80 p-3 text-xs leading-5 text-amber-800">
        登录捕获将在 P4 接入 WebView。当前密码仅在本地 SQLite 临时保存，后续将迁移到系统密钥链或本地加密。
      </div>
    </div>
  );
}

function SortableStationRow({
  station,
  active,
  onSelect,
  onEdit,
  onPreview,
  onDelete,
  onToggleEnabled,
}: {
  station: Station;
  active: boolean;
  onSelect: (station: Station) => void;
  onEdit: (station: Station) => void;
  onPreview: (station: Station) => void;
  onDelete: (station: Station) => void;
  onToggleEnabled: (station: Station) => void;
}) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({ id: station.id });
  return (
    <div ref={setNodeRef} style={{ transform: CSS.Transform.toString(transform), transition }} className={cn("will-change-transform", isDragging && "opacity-35")}>
      <StationRowContent
        active={active}
        station={station}
        dragAttributes={attributes}
        dragListeners={listeners}
        onDelete={onDelete}
        onEdit={onEdit}
        onPreview={onPreview}
        onSelect={onSelect}
        onToggleEnabled={onToggleEnabled}
      />
    </div>
  );
}

function StationRowContent({
  station,
  active = false,
  overlay = false,
  dragAttributes,
  dragListeners,
  onSelect,
  onEdit,
  onPreview,
  onDelete,
  onToggleEnabled,
}: {
  station: Station;
  active?: boolean;
  overlay?: boolean;
  dragAttributes?: HTMLAttributes<HTMLButtonElement>;
  dragListeners?: ReturnType<typeof useSortable>["listeners"];
  onSelect?: (station: Station) => void;
  onEdit?: (station: Station) => void;
  onPreview?: (station: Station) => void;
  onDelete?: (station: Station) => void;
  onToggleEnabled?: (station: Station) => void;
}) {
  const balanceText = station.balanceCny === null ? "未采集" : `¥${station.balanceCny.toFixed(2)}`;
  return (
    <div className={cn("group grid min-h-[80px] grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-3 rounded-2xl border bg-white px-3 py-2 text-left shadow-[0_8px_18px_rgba(33,79,88,0.055)] transition-colors", active ? "border-teal-300 bg-teal-50/40 ring-2 ring-teal-100" : "border-cyan-100 hover:border-teal-200 hover:bg-teal-50/25", overlay && "border-teal-300 shadow-[0_14px_28px_rgba(13,148,136,0.18)]")}>
      <button type="button" className="flex h-9 w-9 cursor-grab items-center justify-center rounded-xl border border-cyan-100 bg-cyan-50 text-slate-400 transition active:cursor-grabbing group-hover:text-teal-700" aria-label="拖拽排序" {...dragAttributes} {...dragListeners}>
        <GripVertical className="h-4 w-4" />
      </button>
      <button type="button" onClick={() => onSelect?.(station)} className="min-w-0 text-left">
        <div className="flex min-w-0 items-center gap-2.5">
          <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-xl bg-cyan-50 text-[11px] font-semibold text-teal-700">{station.name.slice(0, 2).toUpperCase()}</div>
          <div className="min-w-0">
            <div className="flex min-w-0 items-center gap-2">
              <StationStatusDot status={station.status} />
              <div className="truncate text-[13px] font-semibold text-slate-800">{station.name}</div>
            </div>
            <div className="mt-0.5 flex min-w-0 items-center gap-2 text-xs text-muted-foreground">
              <span className="shrink-0">{stationTypeLabels[station.stationType]}</span>
              <span className="truncate">{station.baseUrl}</span>
              <span className="shrink-0 rounded-full border border-cyan-100 bg-cyan-50 px-2 py-0.5 text-[11px] text-slate-600">{station.keyCount} keys</span>
            </div>
          </div>
        </div>
      </button>
      <div className="flex min-w-0 items-center gap-2 justify-self-end">
        <div className="hidden min-w-[86px] text-right md:block">
          <div className="text-[11px] text-muted-foreground">余额</div>
          <div className="truncate text-sm font-semibold text-slate-800">{balanceText}</div>
        </div>
        <div className="hidden min-w-[92px] text-right lg:block">
          <div className="text-[11px] text-muted-foreground">刷新</div>
          <div className="truncate text-sm font-semibold text-slate-800">{station.lastCheckedAt ?? "未检测"}</div>
        </div>
        <StatusBadge tone={statusTone[station.status]}>{stationStatusLabels[station.status]}</StatusBadge>
        <Button variant={station.enabled ? "secondary" : "outline"} className="h-8" onClick={() => onToggleEnabled?.(station)}>
          {station.enabled ? "启用" : "停用"}
        </Button>
        <RowAction title="编辑" onClick={() => onEdit?.(station)}><Edit3 className="h-4 w-4" /></RowAction>
        <RowAction title="详情" onClick={() => onPreview?.(station)}><ArrowRight className="h-4 w-4" /></RowAction>
        <RowAction title="删除" danger onClick={() => onDelete?.(station)}><Trash2 className="h-4 w-4" /></RowAction>
      </div>
    </div>
  );
}

function SectionBlock({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="rounded-2xl border border-cyan-100 bg-cyan-50/35 p-3">
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
      title={keyForm.id ? "编辑 API Key" : "新增 API Key"}
      description="一个站点下面可以管理多把 Key。"
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
        <Field label="API Key">
          <input className={inputClassName} value={keyForm.apiKey} onChange={(event) => onKeyFormChange({ ...keyForm, apiKey: event.target.value })} placeholder={keyForm.id ? "留空保留旧 key" : "sk-..."} required={!keyForm.id} />
        </Field>
        <div className="grid gap-3 md:grid-cols-3">
          <Field label="分组">
            <input className={inputClassName} value={keyForm.groupName} onChange={(event) => onKeyFormChange({ ...keyForm, groupName: event.target.value })} />
          </Field>
          <Field label="Tier">
            <input className={inputClassName} value={keyForm.tierLabel} onChange={(event) => onKeyFormChange({ ...keyForm, tierLabel: event.target.value })} />
          </Field>
          <Field label="状态">
            <select className={inputClassName} value={keyForm.status} onChange={(event) => onKeyFormChange({ ...keyForm, status: event.target.value as StationKeyStatus })}>
              {Object.entries(stationKeyStatusLabels).map(([value, label]) => (
                <option key={value} value={value}>{label}</option>
              ))}
            </select>
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

function RowAction({ title, onClick, children, danger = false }: { title: string; onClick: () => void; children: ReactNode; danger?: boolean; }) {
  return (
    <Button variant={danger ? "danger" : "outline"} className="h-8 w-8 px-0" title={title} onClick={(event) => { event.stopPropagation(); onClick(); }}>
      {children}
    </Button>
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

const inputClassName = "h-8 rounded-xl border border-cyan-100 bg-cyan-50/40 px-3 text-sm text-slate-800 outline-none transition focus:border-teal-300 focus:bg-white focus:ring-2 focus:ring-teal-100";
