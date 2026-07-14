import { useEffect, useLayoutEffect, useMemo, useRef, useState, type RefObject } from "react";
import { createPortal } from "react-dom";
import { ArrowLeft, CalendarDays, ChevronLeft, ChevronRight, Plus, RefreshCw, RotateCcw, Search } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import { Button, Dialog, IconButton, SectionCard, SelectControl, StatusBadge, SwitchControl, useToast } from "@/components/ui";
import { useInteractionActivity } from "@/components/ui/InteractionActivity";
import { listModelBasePrices, resetModelBasePricesToBuiltins, upsertModelBasePrice } from "@/lib/api/economics";
import { readError } from "@/lib/errors";
import type { ModelBasePrice } from "@/lib/types/economics";

type ModelBasePricesPageProps = {
  backLabel: string;
  onBack: () => void;
};

type DraftRow = {
  id?: string;
  provider: string;
  model: string;
  inputPrice: string;
  outputPrice: string;
  currency: string;
  unit: string;
  sourceUrl: string;
  sourceLabel: string;
  sourceCheckedAt: string;
  enabled: boolean;
  builtIn: boolean;
  note: string;
};

type ProviderFilter = "all" | "openai" | "google" | "anthropic" | "xai" | "custom";
type PriceField = "inputPrice" | "outputPrice";
type DatePickerPosition = {
  left: number;
  top: number;
};

const providerFilterOptions: Array<{ value: ProviderFilter; label: string }> = [
  { value: "all", label: "全部厂商" },
  { value: "openai", label: "OpenAI" },
  { value: "google", label: "Google" },
  { value: "anthropic", label: "Anthropic" },
  { value: "xai", label: "xAI" },
  { value: "custom", label: "自定义/其他" },
];

const knownProviderOrder = ["openai", "google", "anthropic", "xai"];

const currencyOptions = [
  { value: "USD", label: "USD" },
  { value: "CNY", label: "CNY" },
  { value: "EUR", label: "EUR" },
  { value: "JPY", label: "JPY" },
  { value: "HKD", label: "HKD" },
];

const unitOptions = [
  { value: "K", label: "K" },
  { value: "M", label: "M" },
  { value: "B", label: "B" },
];

function createEmptyDraft(): DraftRow {
  return {
    provider: "custom",
    model: "",
    inputPrice: "",
    outputPrice: "",
    currency: "USD",
    unit: "M",
    sourceUrl: "",
    sourceLabel: "Manual",
    sourceCheckedAt: formatLocalDate(new Date()),
    enabled: true,
    builtIn: false,
    note: "",
  };
}

export function ModelBasePricesPage({ backLabel, onBack }: ModelBasePricesPageProps) {
  const toast = useToast();
  const [rows, setRows] = useState<ModelBasePrice[]>([]);
  const [query, setQuery] = useState("");
  const [providerFilter, setProviderFilter] = useState<ProviderFilter>("all");
  const [createDialogOpen, setCreateDialogOpen] = useState(false);
  const [createDraft, setCreateDraft] = useState<DraftRow>(() => createEmptyDraft());
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [savingKeys, setSavingKeys] = useState<Set<string>>(() => new Set());
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void refresh();
  }, []);

  async function refresh(showSuccess = false) {
    setLoading(true);
    setError(null);
    try {
      const nextRows = await listModelBasePrices();
      setRows(nextRows);
      if (showSuccess) {
        toast.success("模型基准价格已刷新");
      }
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      toast.error("读取模型基准价格失败", message);
    } finally {
      setLoading(false);
    }
  }

  async function resetBuiltins() {
    setSaving(true);
    setError(null);
    try {
      const nextRows = await resetModelBasePricesToBuiltins();
      setRows(nextRows);
      toast.success("已恢复内置基准价格");
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      toast.error("恢复内置价格失败", message);
    } finally {
      setSaving(false);
    }
  }

  async function saveCreateDraft() {
    if (!createDraft.provider.trim() || !createDraft.model.trim()) {
      toast.error("请填写供应商和模型");
      return;
    }
    setSaving(true);
    setError(null);
    try {
      const saved = await upsertModelBasePrice(draftToInput(createDraft));
      setRows((currentRows) => upsertRow(currentRows, saved));
      setCreateDialogOpen(false);
      setCreateDraft(createEmptyDraft());
      toast.success("模型基准价格已新增");
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      toast.error("新增模型基准价格失败", message);
    } finally {
      setSaving(false);
    }
  }

  async function updateRow(row: ModelBasePrice, patch: Partial<DraftRow>, savingKey: string) {
    setSavingKeys((current) => new Set(current).add(savingKey));
    setError(null);
    try {
      const saved = await upsertModelBasePrice(rowToInput(row, patch));
      setRows((currentRows) => upsertRow(currentRows, saved));
      return saved;
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      toast.error("保存模型基准价格失败", message);
      throw requestError;
    } finally {
      setSavingKeys((current) => {
        const next = new Set(current);
        next.delete(savingKey);
        return next;
      });
    }
  }

  async function updatePrice(row: ModelBasePrice, field: PriceField, draftValue: string) {
    const parsedPrice = parseDraftPrice(draftValue);
    if (parsedPrice.invalid) {
      toast.error("价格必须是非负数字");
      throw new Error("Invalid model base price");
    }
    await updateRow(row, { [field]: parsedPrice.value === null ? "" : String(parsedPrice.value) }, `${row.id}:${field}`);
  }

  const metrics = useMemo(() => {
    const enabled = rows.filter((row) => row.enabled).length;
    const builtIn = rows.filter((row) => row.builtIn).length;
    return { enabled, builtIn, total: rows.length };
  }, [rows]);

  const visibleRows = useMemo(() => {
    const normalizedQuery = query.trim().toLowerCase();
    return rows.filter((row) => {
      const providerGroup = providerGroupValue(row.provider);
      const matchesProvider = providerFilter === "all" || providerGroup === providerFilter;
      if (!matchesProvider) {
        return false;
      }
      if (!normalizedQuery) {
        return true;
      }
      return [row.model, row.provider, row.note ?? ""].some((value) =>
        value.toLowerCase().includes(normalizedQuery),
      );
    });
  }, [providerFilter, query, rows]);

  const groupedRows = useMemo(() => groupRowsByProvider(visibleRows), [visibleRows]);

  function openCreateDialog() {
    setCreateDraft(createEmptyDraft());
    setCreateDialogOpen(true);
  }

  return (
    <PageScaffold
      title="模型基准价格"
      stickyHeader
      backAction={
        <IconButton label={backLabel} onClick={onBack}>
          <ArrowLeft className="h-4 w-4" />
        </IconButton>
      }
      actions={
        <>
          <Button variant="outline" onClick={openCreateDialog}>
            <Plus className="h-4 w-4" />
            新增
          </Button>
          <Button disabled={loading || saving} variant="outline" onClick={() => void refresh(true)}>
            <RefreshCw className="h-4 w-4" />
            刷新
          </Button>
          <Button disabled={saving} variant="outline" onClick={() => void resetBuiltins()}>
            <RotateCcw className="h-4 w-4" />
            恢复内置
          </Button>
        </>
      }
    >
      <div className="grid min-w-0 gap-[var(--shell-page-gap)]">
        <SectionCard
          title="价格清单"
          action={
            <div className="flex items-center gap-2 text-xs text-muted-foreground">
              <span>{metrics.total} 个模型</span>
              <span>{metrics.enabled} 个启用</span>
              <span>{metrics.builtIn} 个内置</span>
            </div>
          }
          contentClassName="overflow-hidden rounded-none border-0 bg-transparent p-0 shadow-none"
        >
          <div className="flex flex-wrap items-center gap-2 border-b border-border bg-surface px-3 py-2">
            <div className="relative min-w-[220px] flex-1">
              <Search className="pointer-events-none absolute left-2.5 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground/70" />
              <input
                aria-label="搜索模型基准价格"
                className="h-8 w-full rounded-[var(--surface-radius)] border border-border bg-surface pl-8 pr-3 text-sm text-foreground outline-none transition focus:border-ring focus:ring-2 focus:ring-ring/30"
                placeholder="搜索模型、厂商或备注"
                value={query}
                onChange={(event) => setQuery(event.target.value)}
              />
            </div>
            <SelectControl
              ariaLabel="按厂商筛选模型基准价格"
              className="min-w-[150px]"
              options={providerFilterOptions}
              value={providerFilter}
              onChange={setProviderFilter}
            />
          </div>

          {!loading && visibleRows.length === 0 ? (
            <div className="px-2.5 py-8 text-center text-sm text-muted-foreground">
              暂无符合条件的模型基准价格
            </div>
          ) : (
            <div className="divide-y divide-border">
              {groupedRows.map((group) => (
                <section key={group.provider} className="grid gap-3 px-4 py-4">
                  <div className="flex flex-wrap items-center justify-between gap-3">
                    <div className="text-sm font-semibold text-foreground">{group.label}</div>
                    <div className="text-xs text-muted-foreground">{group.rows.length} 个模型</div>
                  </div>

                  <div className="overflow-x-auto border-y border-border">
                    <table className="w-full min-w-[820px] table-fixed text-left text-[13px]">
                      <TableColumnHeaderRow />
                      <tbody className="divide-y divide-border">
                        {group.rows.map((row) => (
                          <tr key={row.id} className="h-10 text-foreground hover:bg-surface-subtle">
                            <td className="px-2.5 font-medium text-foreground">{row.model}</td>
                            <td className="px-2 uppercase text-muted-foreground">{row.provider}</td>
                            <td className="px-2 text-right">
                              <EditablePriceCell
                                label={`${row.model} 输入价`}
                                saving={savingKeys.has(`${row.id}:inputPrice`)}
                                value={row.inputPrice}
                                onCommit={(nextValue) => updatePrice(row, "inputPrice", nextValue)}
                              />
                            </td>
                            <td className="px-2 text-right">
                              <EditablePriceCell
                                label={`${row.model} 输出价`}
                                saving={savingKeys.has(`${row.id}:outputPrice`)}
                                value={row.outputPrice}
                                onCommit={(nextValue) => updatePrice(row, "outputPrice", nextValue)}
                              />
                            </td>
                            <td className="px-2">
                              <div className="flex items-center gap-2">
                                <SwitchControl
                                  ariaLabel={`${row.model} 启用状态`}
                                  checked={row.enabled}
                                  className="h-5 w-10 justify-center gap-0 border-transparent bg-transparent p-0 shadow-none"
                                  disabled={savingKeys.has(`${row.id}:enabled`)}
                                  offLabel="停用"
                                  onLabel="启用"
                                  showLabel={false}
                                  onCheckedChange={() => {
                                    void updateRow(row, { enabled: !row.enabled }, `${row.id}:enabled`);
                                  }}
                                />
                                {row.builtIn && <StatusBadge tone="info">内置</StatusBadge>}
                              </div>
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                </section>
              ))}
            </div>
          )}
        </SectionCard>

        {error && <div className="text-sm text-danger-foreground">{error}</div>}
      </div>

      <Dialog
        open={createDialogOpen}
        title="新增基准价格"
        description="新增自定义模型价格；已有内置价格可以直接在表格里点击数字维护。"
        footer={
          <div className="flex justify-end gap-2">
            <Button disabled={saving} variant="outline" onClick={() => setCreateDialogOpen(false)}>
              取消
            </Button>
            <Button disabled={saving || !createDraft.provider.trim() || !createDraft.model.trim()} onClick={() => void saveCreateDraft()}>
              {saving ? "保存中" : "保存"}
            </Button>
          </div>
        }
        onClose={() => setCreateDialogOpen(false)}
      >
        <div className="grid gap-3 p-5 md:grid-cols-2">
          <Field label="供应商" value={createDraft.provider} onChange={(provider) => setCreateDraft({ ...createDraft, provider })} />
          <Field label="模型" value={createDraft.model} onChange={(model) => setCreateDraft({ ...createDraft, model })} />
          <Field label="输入价" numeric value={createDraft.inputPrice} onChange={(inputPrice) => setCreateDraft({ ...createDraft, inputPrice })} />
          <Field label="输出价" numeric value={createDraft.outputPrice} onChange={(outputPrice) => setCreateDraft({ ...createDraft, outputPrice })} />
          <SelectField label="币种" options={currencyOptions} value={createDraft.currency} onChange={(currency) => setCreateDraft({ ...createDraft, currency })} />
          <SelectField label="单位" options={unitOptions} value={createDraft.unit} onChange={(unit) => setCreateDraft({ ...createDraft, unit })} />
          <Field label="来源名称" value={createDraft.sourceLabel} onChange={(sourceLabel) => setCreateDraft({ ...createDraft, sourceLabel })} />
          <DateField label="检查日期" value={createDraft.sourceCheckedAt} onChange={(sourceCheckedAt) => setCreateDraft({ ...createDraft, sourceCheckedAt })} />
          <div className="md:col-span-2">
            <Field label="来源 URL" value={createDraft.sourceUrl} onChange={(sourceUrl) => setCreateDraft({ ...createDraft, sourceUrl })} />
          </div>
          <div className="md:col-span-2">
            <Field label="备注" value={createDraft.note} onChange={(note) => setCreateDraft({ ...createDraft, note })} />
          </div>
        </div>
      </Dialog>
    </PageScaffold>
  );
}

function TableColumnHeaderRow() {
  return (
    <thead>
      <tr className="border-b border-border bg-surface text-[11px] font-medium text-muted-foreground">
        <th className="h-7 px-2.5">模型</th>
        <th className="h-7 px-2">供应商</th>
        <th className="h-7 px-2 text-right">输入价</th>
        <th className="h-7 px-2 text-right">输出价</th>
        <th className="h-7 px-2">状态</th>
      </tr>
    </thead>
  );
}

function EditablePriceCell({
  label,
  saving,
  value,
  onCommit,
}: {
  label: string;
  saving: boolean;
  value: number | null;
  onCommit: (value: string) => Promise<void>;
}) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(formatPriceInput(value));
  const numberText = formatPrice(value);
  const numberBoxWidth = `calc(${Math.max(5, Math.min(8, numberText.length))}ch + 0.5rem)`;

  useEffect(() => {
    if (!editing) {
      setDraft(formatPriceInput(value));
    }
  }, [editing, value]);

  async function commitEdit() {
    const currentValue = formatPriceInput(value);
    const nextValue = draft.trim();
    if (nextValue === currentValue) {
      setEditing(false);
      return;
    }
    try {
      await onCommit(nextValue);
      setEditing(false);
    } catch {
      setDraft(currentValue);
      setEditing(false);
    }
  }

  if (editing) {
    return (
      <span className="inline-flex h-7 items-center justify-end gap-0.5 tabular-nums">
        <input
          aria-label={label}
          autoFocus
          className="h-7 rounded-[7px] border border-ring bg-surface px-1 text-center text-sm text-foreground outline-none ring-2 ring-ring/30"
          inputMode="decimal"
          pattern="[0-9]*[.]?[0-9]*"
          style={{ width: numberBoxWidth }}
          type="text"
          value={draft}
          onBlur={() => void commitEdit()}
          onChange={(event) => setDraft(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              event.preventDefault();
              event.currentTarget.blur();
            }
            if (event.key === "Escape") {
              event.preventDefault();
              setDraft(formatPriceInput(value));
              setEditing(false);
            }
          }}
        />
        <span className="whitespace-nowrap text-xs text-muted-foreground">$/M</span>
      </span>
    );
  }

  return (
    <span className="inline-flex h-7 items-center justify-end gap-0.5 tabular-nums">
      <button
        aria-label={`编辑${label}`}
        className="inline-flex h-7 cursor-pointer items-center justify-center rounded-[7px] px-1 text-center text-foreground transition-colors hover:bg-surface hover:ring-1 hover:ring-ring/30 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/30 disabled:cursor-default disabled:opacity-60"
        disabled={saving}
        style={{ width: numberBoxWidth }}
        type="button"
        onClick={() => setEditing(true)}
      >
        {saving ? "保存中" : numberText}
      </button>
      {value !== null && (
        <span className="whitespace-nowrap text-xs text-muted-foreground" aria-hidden="true">
          $/M
        </span>
      )}
    </span>
  );
}

function SelectField({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: string;
  options: Array<{ value: string; label: string }>;
  onChange: (value: string) => void;
}) {
  return (
    <label className="grid gap-1 text-xs font-medium text-muted-foreground">
      <span>{label}</span>
      <SelectControl
        ariaLabel={label}
        className="w-full"
        options={options}
        value={value}
        onChange={onChange}
      />
    </label>
  );
}

function DateField({
  label,
  value,
  onChange,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
}) {
  const interactionActive = useInteractionActivity();
  const selectedDate = useMemo(() => parseDateValue(value) ?? new Date(), [value]);
  const triggerRef = useRef<HTMLButtonElement | null>(null);
  const panelRef = useRef<HTMLDivElement | null>(null);
  const [open, setOpen] = useState(false);
  const [viewDate, setViewDate] = useState(() => selectedDate);
  const [position, setPosition] = useState<DatePickerPosition | null>(null);

  useLayoutEffect(() => {
    if (interactionActive) {
      return;
    }
    setOpen(false);
    setPosition(null);
  }, [interactionActive]);

  useEffect(() => {
    if (!open) {
      setViewDate(selectedDate);
    }
  }, [open, selectedDate]);

  useLayoutEffect(() => {
    if (!open) {
      return;
    }
    updateDatePickerPosition(triggerRef.current, setPosition);
  }, [open, viewDate]);

  useEffect(() => {
    if (!open) {
      return;
    }
    const handlePointerDown = (event: PointerEvent) => {
      const target = event.target as Node;
      if (triggerRef.current?.contains(target) || panelRef.current?.contains(target)) {
        return;
      }
      setOpen(false);
    };
    const handleViewportChange = () => setOpen(false);

    document.addEventListener("pointerdown", handlePointerDown);
    window.addEventListener("resize", handleViewportChange);
    window.addEventListener("scroll", handleViewportChange, true);
    return () => {
      document.removeEventListener("pointerdown", handlePointerDown);
      window.removeEventListener("resize", handleViewportChange);
      window.removeEventListener("scroll", handleViewportChange, true);
    };
  }, [open]);

  return (
    <label className="grid gap-1 text-xs font-medium text-muted-foreground">
      <span>{label}</span>
      <button
        ref={triggerRef}
        type="button"
        aria-label={label}
        aria-expanded={open}
        className="flex h-8 min-w-0 cursor-pointer items-center justify-between gap-2 rounded-[var(--surface-radius)] border border-border bg-surface px-3 text-left text-sm text-foreground shadow-surface outline-none transition hover:border-ring hover:bg-surface-subtle focus:border-ring focus:ring-2 focus:ring-ring/30"
        onClick={() => setOpen((current) => !current)}
      >
        <span className="tabular-nums">{formatDisplayDate(value)}</span>
        <CalendarDays className="h-4 w-4 shrink-0 text-muted-foreground" />
      </button>
      {interactionActive && open && position ? (
        <DatePickerPanel
          panelRef={panelRef}
          position={position}
          selectedValue={value}
          viewDate={viewDate}
          onMonthChange={setViewDate}
          onSelect={(nextValue) => {
            onChange(nextValue);
            setOpen(false);
          }}
        />
      ) : null}
    </label>
  );
}

function DatePickerPanel({
  panelRef,
  position,
  selectedValue,
  viewDate,
  onMonthChange,
  onSelect,
}: {
  panelRef: RefObject<HTMLDivElement>;
  position: DatePickerPosition;
  selectedValue: string;
  viewDate: Date;
  onMonthChange: (date: Date) => void;
  onSelect: (value: string) => void;
}) {
  const monthDays = getCalendarDays(viewDate);
  const selectedDate = parseDateValue(selectedValue);
  const todayValue = formatLocalDate(new Date());
  const viewMonth = viewDate.getMonth();

  return createPortal(
    <div
      ref={panelRef}
      className="fixed z-[70] w-[236px] rounded-[var(--surface-radius)] border border-border bg-surface p-2 shadow-surface"
      style={{ left: position.left, top: position.top }}
    >
      <div className="mb-2 flex items-center justify-between">
        <div className="px-1 text-xs font-semibold text-foreground">
          {viewDate.getFullYear()}年{viewDate.getMonth() + 1}月
        </div>
        <div className="flex items-center gap-1">
          <button
            type="button"
            aria-label="上个月"
            className="flex h-7 w-7 cursor-pointer items-center justify-center rounded-[7px] text-muted-foreground transition hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/30"
            onClick={() => onMonthChange(addMonths(viewDate, -1))}
          >
            <ChevronLeft className="h-4 w-4" />
          </button>
          <button
            type="button"
            aria-label="下个月"
            className="flex h-7 w-7 cursor-pointer items-center justify-center rounded-[7px] text-muted-foreground transition hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/30"
            onClick={() => onMonthChange(addMonths(viewDate, 1))}
          >
            <ChevronRight className="h-4 w-4" />
          </button>
        </div>
      </div>
      <div className="grid grid-cols-7 gap-1 text-center text-[11px] font-medium text-muted-foreground">
        {["一", "二", "三", "四", "五", "六", "日"].map((day) => (
          <div key={day} className="h-6 leading-6">
            {day}
          </div>
        ))}
      </div>
      <div className="mt-1 grid grid-cols-7 gap-1 text-center text-xs">
        {monthDays.map((date) => {
          const dateValue = formatLocalDate(date);
          const selected = selectedDate ? isSameDate(date, selectedDate) : false;
          const today = dateValue === todayValue;
          const muted = date.getMonth() !== viewMonth;
          return (
            <button
              key={dateValue}
              type="button"
              className={[
                "flex h-7 w-7 cursor-pointer items-center justify-center rounded-[7px] tabular-nums transition focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/30",
                selected
                  ? "bg-primary-solid text-primary-foreground shadow-surface"
                  : today
                    ? "border border-ring bg-selected text-primary"
                    : "text-foreground hover:bg-muted",
                muted && !selected ? "text-muted-foreground/70" : "",
              ].join(" ")}
              onClick={() => onSelect(dateValue)}
            >
              {date.getDate()}
            </button>
          );
        })}
      </div>
      <div className="mt-2 flex justify-between border-t border-border pt-2">
        <button
          type="button"
          className="h-7 cursor-pointer rounded-[7px] px-2 text-xs text-muted-foreground transition hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/30"
          onClick={() => onSelect("")}
        >
          清除
        </button>
        <button
          type="button"
          className="h-7 cursor-pointer rounded-[7px] px-2 text-xs font-medium text-primary transition hover:bg-selected focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/30"
          onClick={() => onSelect(todayValue)}
        >
          今天
        </button>
      </div>
    </div>,
    document.body,
  );
}

function Field({
  label,
  value,
  numeric,
  onChange,
}: {
  label: string;
  value: string;
  numeric?: boolean;
  onChange: (value: string) => void;
}) {
  return (
    <label className="grid gap-1 text-xs font-medium text-muted-foreground">
      <span>{label}</span>
      <input
        className="h-8 min-w-0 rounded-[var(--surface-radius)] border border-border bg-surface px-3 text-sm text-foreground outline-none transition focus:border-ring focus:ring-2 focus:ring-ring/30"
        min={numeric ? "0" : undefined}
        step={numeric ? "0.0001" : undefined}
        type={numeric ? "number" : "text"}
        value={value}
        onChange={(event) => onChange(event.target.value)}
      />
    </label>
  );
}

function draftToInput(draft: DraftRow) {
  return {
    id: draft.id,
    provider: draft.provider.trim(),
    model: draft.model.trim(),
    inputPrice: draft.inputPrice.trim() === "" ? null : Number(draft.inputPrice),
    outputPrice: draft.outputPrice.trim() === "" ? null : Number(draft.outputPrice),
    currency: draft.currency.trim() || "USD",
    unit: draft.unit.trim() || "M",
    sourceUrl: draft.sourceUrl.trim(),
    sourceLabel: draft.sourceLabel.trim() || "Manual",
    sourceCheckedAt: draft.sourceCheckedAt.trim() === "" ? null : draft.sourceCheckedAt,
    enabled: draft.enabled,
    builtIn: draft.builtIn,
    note: draft.note.trim() === "" ? null : draft.note,
  };
}

function rowToInput(row: ModelBasePrice, patch: Partial<DraftRow>) {
  return draftToInput({
    id: row.id,
    provider: patch.provider ?? row.provider,
    model: patch.model ?? row.model,
    inputPrice: patch.inputPrice ?? formatPriceInput(row.inputPrice),
    outputPrice: patch.outputPrice ?? formatPriceInput(row.outputPrice),
    currency: patch.currency ?? row.currency,
    unit: patch.unit ?? row.unit,
    sourceUrl: patch.sourceUrl ?? row.sourceUrl,
    sourceLabel: patch.sourceLabel ?? row.sourceLabel,
    sourceCheckedAt: patch.sourceCheckedAt ?? row.sourceCheckedAt ?? "",
    enabled: patch.enabled ?? row.enabled,
    builtIn: patch.builtIn ?? row.builtIn,
    note: patch.note ?? row.note ?? "",
  });
}

function parseDraftPrice(value: string): { value: number | null; invalid: false } | { invalid: true } {
  const trimmed = value.trim();
  if (trimmed === "") {
    return { value: null, invalid: false };
  }
  const parsed = Number(trimmed);
  if (!Number.isFinite(parsed) || parsed < 0) {
    return { invalid: true };
  }
  return { value: parsed, invalid: false };
}

function upsertRow(rows: ModelBasePrice[], row: ModelBasePrice) {
  const found = rows.some((item) => item.id === row.id);
  const nextRows = found ? rows.map((item) => (item.id === row.id ? row : item)) : [...rows, row];
  return nextRows.sort(compareRows);
}

function groupRowsByProvider(rows: ModelBasePrice[]) {
  const groups = new Map<string, ModelBasePrice[]>();
  for (const row of [...rows].sort(compareRows)) {
    const group = providerGroupValue(row.provider);
    groups.set(group, [...(groups.get(group) ?? []), row]);
  }
  return ["openai", "google", "anthropic", "xai", "custom"]
    .filter((provider) => groups.has(provider))
    .map((provider) => ({
      provider,
      label: providerLabel(provider),
      rows: groups.get(provider) ?? [],
    }));
}

function compareRows(left: ModelBasePrice, right: ModelBasePrice) {
  const providerDelta = providerSortIndex(left.provider) - providerSortIndex(right.provider);
  if (providerDelta !== 0) {
    return providerDelta;
  }
  return left.model.localeCompare(right.model);
}

function providerSortIndex(provider: string) {
  const index = knownProviderOrder.indexOf(provider.toLowerCase());
  return index >= 0 ? index : knownProviderOrder.length;
}

function providerGroupValue(provider: string): ProviderFilter {
  const normalized = provider.toLowerCase();
  if (normalized === "openai" || normalized === "google" || normalized === "anthropic" || normalized === "xai") {
    return normalized;
  }
  return "custom";
}

function providerLabel(provider: string) {
  const option = providerFilterOptions.find((item) => item.value === provider);
  return option?.label ?? provider;
}

function formatPriceInput(value: number | null) {
  return value === null ? "" : String(value);
}

function formatPrice(value: number | null) {
  if (value === null) {
    return "未设";
  }
  return Number.isInteger(value) ? value.toFixed(0) : value.toString();
}

function formatLocalDate(date: Date) {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

function formatDisplayDate(value: string) {
  return value ? value.replace(/-/g, "/") : "未选择";
}

function parseDateValue(value: string) {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value);
  if (!match) {
    return null;
  }
  const year = Number(match[1]);
  const month = Number(match[2]) - 1;
  const day = Number(match[3]);
  const date = new Date(year, month, day);
  if (date.getFullYear() !== year || date.getMonth() !== month || date.getDate() !== day) {
    return null;
  }
  return date;
}

function updateDatePickerPosition(
  trigger: HTMLButtonElement | null,
  setPosition: (position: DatePickerPosition) => void,
) {
  const rect = trigger?.getBoundingClientRect();
  if (!rect) {
    return;
  }
  const gap = 6;
  const viewportPadding = 10;
  const panelWidth = 236;
  const panelHeight = 300;
  const spaceAbove = rect.top - viewportPadding;
  const spaceBelow = window.innerHeight - rect.bottom - viewportPadding;
  const canOpenBelow = spaceBelow >= panelHeight;
  const canOpenAbove = spaceAbove >= panelHeight;
  const openBelow = canOpenBelow || !canOpenAbove;
  const preferredLeft = rect.right - panelWidth;
  const left = Math.max(
    viewportPadding,
    Math.min(preferredLeft, window.innerWidth - panelWidth - viewportPadding),
  );
  const top = openBelow
    ? Math.min(window.innerHeight - viewportPadding - panelHeight, rect.bottom + gap)
    : Math.max(viewportPadding, rect.top - panelHeight - gap);

  setPosition({ left, top });
}

function addMonths(date: Date, delta: number) {
  return new Date(date.getFullYear(), date.getMonth() + delta, 1);
}

function getCalendarDays(viewDate: Date) {
  const firstDay = new Date(viewDate.getFullYear(), viewDate.getMonth(), 1);
  const mondayOffset = (firstDay.getDay() + 6) % 7;
  const startDate = new Date(firstDay);
  startDate.setDate(firstDay.getDate() - mondayOffset);

  return Array.from({ length: 42 }, (_, index) => {
    const date = new Date(startDate);
    date.setDate(startDate.getDate() + index);
    return date;
  });
}

function isSameDate(left: Date, right: Date) {
  return (
    left.getFullYear() === right.getFullYear() &&
    left.getMonth() === right.getMonth() &&
    left.getDate() === right.getDate()
  );
}
