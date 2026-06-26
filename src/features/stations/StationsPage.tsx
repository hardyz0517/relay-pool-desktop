import {
  memo,
  useCallback,
  useEffect,
  useMemo,
  useState,
  type FormEvent,
  type HTMLAttributes,
  type ReactNode,
} from "react";
import {
  closestCenter,
  DndContext,
  DragOverlay,
  type DragEndEvent,
  type DragStartEvent,
  PointerSensor,
  useSensor,
  useSensors,
} from "@dnd-kit/core";
import {
  SortableContext,
  useSortable,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { ArrowRight, Edit3, GripVertical, Plus, Trash2 } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import {
  Button,
  Dialog,
  EmptyState,
  PropertyList,
  PropertyRow,
  StatusBadge,
  Toolbar,
} from "@/components/ui";
import {
  createStation,
  deleteStation,
  listStations,
  reorderStations,
  updateStation,
} from "@/lib/api/stations";
import {
  stationStatusLabels,
  stationTypeLabels,
  type Station,
  type StationInput,
  type StationType,
} from "@/lib/types/stations";
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
};

const exampleStation: StationFormState = {
  name: "Orchid Relay",
  stationType: "sub2api",
  baseUrl: "https://api.orchid-relay.example/v1",
  apiKey: "sk-example-change-me",
  enabled: true,
  creditPerCny: "1",
  lowBalanceThresholdCny: "15",
  note: "示例站点，仅用于验证本地 SQLite 持久化。",
};

const statusTone = {
  healthy: "healthy",
  warning: "warning",
  error: "error",
  disabled: "disabled",
  unchecked: "info",
} as const;

export function StationsPage() {
  const [stations, setStations] = useState<Station[]>([]);
  const [selectedStationId, setSelectedStationId] = useState<string | null>(null);
  const [activeDragId, setActiveDragId] = useState<string | null>(null);
  const [dialogMode, setDialogMode] = useState<DialogMode>(null);
  const [editingStationId, setEditingStationId] = useState<string | null>(null);
  const [detailStationId, setDetailStationId] = useState<string | null>(null);
  const [form, setForm] = useState<StationFormState>(emptyForm);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const sensors = useSensors(
    useSensor(PointerSensor, {
      activationConstraint: { distance: 10 },
    }),
  );

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

  const activeDragStation = useMemo(
    () => stations.find((station) => station.id === activeDragId) ?? null,
    [activeDragId, stations],
  );

  const enabledCount = useMemo(
    () => stations.filter((station) => station.enabled).length,
    [stations],
  );

  const attentionCount = useMemo(
    () => stations.filter((station) => station.status === "warning" || station.status === "error").length,
    [stations],
  );

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

  const openCreate = useCallback(() => {
    setDialogMode("create");
    setEditingStationId(null);
    setDetailStationId(null);
    setForm(emptyForm);
    setMessage(null);
    setError(null);
  }, []);

  const openCreateWithExample = useCallback(() => {
    setDialogMode("create");
    setEditingStationId(null);
    setDetailStationId(null);
    setForm(exampleStation);
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
      lowBalanceThresholdCny:
        station.lowBalanceThresholdCny === null ? "" : String(station.lowBalanceThresholdCny),
      note: station.note ?? "",
    });
    setMessage(null);
    setError(null);
  }, []);

  const openDetail = useCallback((station: Station) => {
    setDialogMode("detail");
    setDetailStationId(station.id);
    setSelectedStationId(station.id);
    setMessage(null);
    setError(null);
  }, []);

  const closeDialog = useCallback(() => {
    setDialogMode(null);
    setEditingStationId(null);
    setDetailStationId(null);
    setForm(emptyForm);
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
    setSelectedStationId(String(active.id));

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
        setMessage("站点已更新。");
      } else {
        await createStation(input);
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

  return (
    <PageScaffold
      title="中转池"
      description="provider 管理、余额、优先级和启用状态；详情通过 Dialog 查看。"
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
            <div className="text-[13px] font-semibold text-slate-800">中转站</div>
            <div className="text-xs text-muted-foreground">
              {stations.length} 个站点，{enabledCount} 个启用，{attentionCount} 个需要关注
            </div>
          </div>
          <Button variant="secondary" onClick={openCreateWithExample}>
            添加示例
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
              description="添加一个示例站点或新建站点，用来验证本地 SQLite 持久化。"
              action={<Button onClick={openCreateWithExample}>添加示例站点</Button>}
            />
          ) : (
            <DndContext
              sensors={sensors}
              collisionDetection={closestCenter}
              onDragStart={handleDragStart}
              onDragCancel={handleDragCancel}
              onDragEnd={handleDragEnd}
            >
              <SortableContext items={stationIds} strategy={verticalListSortingStrategy}>
                <div className="space-y-2">
                  {stations.map((station, index) => (
                    <SortableStationRow
                      key={station.id}
                      station={station}
                      active={station.id === selectedStation?.id}
                      index={index}
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
                  <StationRowContent station={activeDragStation} index={stations.findIndex((station) => station.id === activeDragStation.id)} overlay />
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

      {dialogMode && (
        <StationDialog
          mode={dialogMode}
          station={detailStation}
          form={form}
          saving={saving}
          onChange={setForm}
          onClose={closeDialog}
          onSubmit={handleSubmit}
          onEdit={detailStation ? () => openEdit(detailStation) : undefined}
        />
      )}
    </PageScaffold>
  );
}

type StationRowProps = {
  station: Station;
  active: boolean;
  index: number;
  onSelect: (station: Station) => void;
  onEdit: (station: Station) => void;
  onPreview: (station: Station) => void;
  onDelete: (station: Station) => void;
  onToggleEnabled: (station: Station) => void;
};

const SortableStationRow = memo(function SortableStationRow({
  station,
  active,
  index,
  onSelect,
  onEdit,
  onPreview,
  onDelete,
  onToggleEnabled,
}: StationRowProps) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } =
    useSortable({ id: station.id });

  const style = {
    transform: CSS.Transform.toString(transform),
    transition,
  };

  return (
    <div
      ref={setNodeRef}
      style={style}
      className={cn(
        "will-change-transform",
        isDragging && "opacity-35",
      )}
    >
      <StationRowContent
        active={active}
        index={index}
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
});

type StationRowContentProps = {
  station: Station;
  index: number;
  active?: boolean;
  overlay?: boolean;
  dragAttributes?: HTMLAttributes<HTMLButtonElement>;
  dragListeners?: ReturnType<typeof useSortable>["listeners"];
  onSelect?: (station: Station) => void;
  onEdit?: (station: Station) => void;
  onPreview?: (station: Station) => void;
  onDelete?: (station: Station) => void;
  onToggleEnabled?: (station: Station) => void;
};

function StationRowContent({
  station,
  index,
  active = false,
  overlay = false,
  dragAttributes,
  dragListeners,
  onSelect,
  onEdit,
  onPreview,
  onDelete,
  onToggleEnabled,
}: StationRowContentProps) {
  const balanceText = station.balanceCny === null ? "未采集" : `¥${station.balanceCny.toFixed(2)}`;
  const priorityText = index < 0 ? "" : `priority ${index + 1}`;

  return (
    <div
      className={cn(
        "group grid min-h-[76px] grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-3 rounded-2xl border bg-white px-3 py-2 text-left shadow-[0_8px_18px_rgba(33,79,88,0.055)] transition-colors",
        active ? "border-teal-300 bg-teal-50/40 ring-2 ring-teal-100" : "border-cyan-100 hover:border-teal-200 hover:bg-teal-50/25",
        overlay && "border-teal-300 shadow-[0_14px_28px_rgba(13,148,136,0.18)]",
      )}
    >
      <button
        type="button"
        className="flex h-9 w-9 cursor-grab items-center justify-center rounded-xl border border-cyan-100 bg-cyan-50 text-slate-400 transition active:cursor-grabbing group-hover:text-teal-700"
        aria-label="拖拽排序"
        {...dragAttributes}
        {...dragListeners}
      >
        <GripVertical className="h-4 w-4" />
      </button>

      <button
        type="button"
        onClick={() => onSelect?.(station)}
        className="min-w-0 text-left"
      >
        <div className="flex min-w-0 items-center gap-2.5">
          <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-xl bg-cyan-50 text-[11px] font-semibold text-teal-700">
            {station.name.slice(0, 2).toUpperCase()}
          </div>
          <div className="min-w-0">
            <div className="flex min-w-0 items-center gap-2">
              <StationStatusDot status={station.status} />
              <div className="truncate text-[13px] font-semibold text-slate-800">
                {station.name}
              </div>
            </div>
            <div className="mt-0.5 flex min-w-0 items-center gap-2 text-xs text-muted-foreground">
              <span className="shrink-0">{stationTypeLabels[station.stationType]}</span>
              <span className="truncate">{station.baseUrl}</span>
              {priorityText && (
                <span className="hidden shrink-0 rounded-full border border-cyan-100 bg-cyan-50 px-2 py-0.5 text-[11px] text-slate-600 md:inline-flex">
                  {priorityText}
                </span>
              )}
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
          <div className="truncate text-sm font-semibold text-slate-800">
            {station.lastCheckedAt ?? "未检测"}
          </div>
        </div>
        <StatusBadge tone={statusTone[station.status]} className="shrink-0">
          {stationStatusLabels[station.status]}
        </StatusBadge>
        <Button
          variant={station.enabled ? "secondary" : "outline"}
          className="h-8"
          onClick={() => onToggleEnabled?.(station)}
        >
          {station.enabled ? "启用" : "禁用"}
        </Button>
        <RowAction title="编辑" onClick={() => onEdit?.(station)}>
          <Edit3 className="h-4 w-4" />
        </RowAction>
        <RowAction title="详情" onClick={() => onPreview?.(station)}>
          <ArrowRight className="h-4 w-4" />
        </RowAction>
        <RowAction title="删除" danger onClick={() => onDelete?.(station)}>
          <Trash2 className="h-4 w-4" />
        </RowAction>
      </div>
    </div>
  );
}

function StationDialog({
  mode,
  station,
  form,
  saving,
  onChange,
  onClose,
  onSubmit,
  onEdit,
}: {
  mode: DialogMode;
  station: Station | null;
  form: StationFormState;
  saving: boolean;
  onChange: (nextForm: StationFormState) => void;
  onClose: () => void;
  onSubmit: (event: FormEvent<HTMLFormElement>) => void;
  onEdit?: () => void;
}) {
  const title =
    mode === "edit" ? "编辑站点" : mode === "detail" ? "站点详情" : "新增站点";
  const description =
    mode === "edit"
      ? "更新 provider 配置；API Key 留空则保持原值。"
      : mode === "detail"
        ? "只读详情，不在列表下方展开。"
        : "新增 provider 记录到本地 SQLite。";

  if (mode === "detail") {
    return (
      <Dialog
        open
        title={title}
        description={description}
        onClose={onClose}
        footer={
          <div className="flex justify-end gap-2">
            <Button variant="outline" onClick={onClose}>关闭</Button>
            {onEdit && (
              <Button onClick={onEdit}>
                <Edit3 className="h-4 w-4" />
                编辑
              </Button>
            )}
          </div>
        }
      >
        {station && <StationDetail station={station} />}
      </Dialog>
    );
  }

  return (
    <Dialog
      open
      title={title}
      description={description}
      onClose={onClose}
      footer={
        <div className="flex justify-end gap-2">
          <Button variant="outline" onClick={onClose}>取消</Button>
          <Button disabled={saving} type="submit" form="station-form">
            保存
          </Button>
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
        <Field label={mode === "edit" ? "API Key（留空保持不变）" : "API Key"}>
          <input className={inputClassName} value={form.apiKey} onChange={(event) => onChange({ ...form, apiKey: event.target.value })} placeholder={mode === "edit" ? "留空保持原 key" : "sk-..."} required={mode !== "edit"} />
        </Field>
        <div className="grid gap-3 md:grid-cols-3">
          <Field label="兑换比例">
            <input className={inputClassName} min="0.01" step="0.01" type="number" value={form.creditPerCny} onChange={(event) => onChange({ ...form, creditPerCny: event.target.value })} />
          </Field>
          <Field label="低余额阈值">
            <input className={inputClassName} min="0" step="0.01" type="number" value={form.lowBalanceThresholdCny} onChange={(event) => onChange({ ...form, lowBalanceThresholdCny: event.target.value })} placeholder="使用全局" />
          </Field>
          <label className="flex items-end gap-2 pb-2 text-sm text-slate-700">
            <input checked={form.enabled} className="h-4 w-4 accent-teal-600" type="checkbox" onChange={(event) => onChange({ ...form, enabled: event.target.checked })} />
            启用站点
          </label>
        </div>
        <Field label="备注">
          <textarea className={`${inputClassName} min-h-20 resize-none py-2`} value={form.note} onChange={(event) => onChange({ ...form, note: event.target.value })} />
        </Field>
      </form>
    </Dialog>
  );
}

function StationDetail({ station }: { station: Station }) {
  return (
    <div className="space-y-4 p-5">
      <div className="grid grid-cols-2 gap-3">
        <SummaryStat label="余额" value={station.balanceCny === null ? "未采集" : `¥${station.balanceCny.toFixed(2)}`} />
        <SummaryStat label="低余额阈值" value={station.lowBalanceThresholdCny === null ? "使用全局" : `¥${station.lowBalanceThresholdCny}`} />
      </div>
      <PropertyList className="overflow-hidden rounded-2xl border border-cyan-100 bg-white/75">
        <PropertyRow label="站点名称" value={station.name} />
        <PropertyRow label="站点类型" value={stationTypeLabels[station.stationType]} />
        <PropertyRow label="Base URL" value={<code className="text-xs">{station.baseUrl}</code>} />
        <PropertyRow label="API Key" value={station.apiKeyMasked} />
        <PropertyRow label="兑换比例" value={`1 元 = ${station.creditPerCny} credit`} />
        <PropertyRow label="采集状态" value={station.lastPricingFetchedAt ?? "未采集"} />
        <PropertyRow label="健康状态" value={stationStatusLabels[station.status]} />
        <PropertyRow label="最近错误" value={station.note ?? "后续待接入"} />
      </PropertyList>
      <div className="rounded-2xl border border-cyan-100 bg-cyan-50/55 p-3 text-xs leading-5 text-slate-700">
        <div className="font-semibold text-slate-800">后续待接入</div>
        <div className="mt-1">真实健康检测、采集诊断和模型倍率会在后续阶段接入。</div>
      </div>
    </div>
  );
}

function RowAction({
  title,
  onClick,
  children,
  danger = false,
}: {
  title: string;
  onClick: () => void;
  children: ReactNode;
  danger?: boolean;
}) {
  return (
    <Button
      variant={danger ? "danger" : "outline"}
      className="h-8 w-8 px-0"
      title={title}
      onClick={(event) => {
        event.stopPropagation();
        onClick();
      }}
    >
      {children}
    </Button>
  );
}

function SummaryStat({ label, value }: { label: string; value: ReactNode }) {
  return (
    <div className="rounded-2xl border border-cyan-100 bg-cyan-50/60 p-3">
      <div className="text-[11px] text-muted-foreground">{label}</div>
      <div className="mt-1 text-lg font-semibold text-slate-800">{value}</div>
    </div>
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

function readError(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

const inputClassName =
  "h-8 rounded-xl border border-cyan-100 bg-cyan-50/40 px-3 text-sm text-slate-800 outline-none transition focus:border-teal-300 focus:bg-white focus:ring-2 focus:ring-teal-100";
