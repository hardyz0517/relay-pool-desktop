import type { ReactNode } from "react";
import {
  SelectControl,
  SwitchControl,
  type SelectOption,
} from "@/components/ui";
import { cn } from "@/lib/utils";
import {
  SCHEDULER_BOOLEAN_FIELD_META,
  SCHEDULER_NUMERIC_FIELD_META,
  type LocalRoutingSettingsDraft,
  type LocalRoutingSettingsErrors,
  type RoutingGroupPreset,
  type SchedulerBooleanField,
  type SchedulerFieldGroup,
  type SchedulerNumericField,
  type SchedulerVisibleBooleanField,
} from "./localRoutingSettingsForm";

type NumericChangeHandler = (field: SchedulerNumericField, value: string) => void;
type BooleanChangeHandler = (field: SchedulerBooleanField) => void;

const inputClassName =
  "h-8 w-full min-w-0 rounded-[var(--surface-radius)] border border-border bg-white px-2.5 text-sm text-slate-800 outline-none transition-colors hover:border-slate-300 focus:border-[hsl(var(--accent)/0.5)] focus:ring-2 focus:ring-[hsl(var(--accent)/0.18)] disabled:cursor-not-allowed disabled:bg-slate-50 disabled:text-slate-500";

const numericFieldEntries = Object.entries(SCHEDULER_NUMERIC_FIELD_META) as Array<[
  SchedulerNumericField,
  (typeof SCHEDULER_NUMERIC_FIELD_META)[SchedulerNumericField],
]>;

const booleanFieldEntries = Object.entries(SCHEDULER_BOOLEAN_FIELD_META) as Array<[
  SchedulerVisibleBooleanField,
  { label: string; group: SchedulerFieldGroup },
]>;

const PROMOTED_BOOLEAN_FIELDS = new Set<SchedulerVisibleBooleanField>(["stickyWeighted"]);

export function LocalRoutingBoundaryFields({
  draft,
  disabled,
  errors,
  groupOptions,
  onMaxRateMultiplierChange,
  onGroupPresetChange,
  onNumericChange,
  onBooleanChange,
}: {
  draft: LocalRoutingSettingsDraft;
  disabled: boolean;
  errors: LocalRoutingSettingsErrors;
  groupOptions: SelectOption<RoutingGroupPreset>[];
  onMaxRateMultiplierChange: (value: string) => void;
  onGroupPresetChange: (value: RoutingGroupPreset) => void;
  onNumericChange: NumericChangeHandler;
  onBooleanChange: BooleanChangeHandler;
}) {
  const boundaryNumericFields = numericFieldEntries.filter(([, meta]) => meta.group === "boundary");
  const boundaryBooleanFields = booleanFieldEntries.filter(([, meta]) => meta.group === "boundary");

  return (
    <>
      <CompactSettingRow label="倍率上限">
        <LabeledNumberInput
          hideLabel
          id="routing-max-rate-multiplier"
          label="倍率上限"
          value={draft.maxRateMultiplier}
          error={errors.maxRateMultiplier}
          disabled={disabled}
          min="0"
          step="0.01"
          placeholder="未设置"
          onChange={onMaxRateMultiplierChange}
        />
      </CompactSettingRow>
      <CompactSettingRow label="分组筛选">
        <SelectControl<RoutingGroupPreset>
          ariaLabel="分组筛选"
          className="w-full sm:w-[220px]"
          disabled={disabled}
          value={draft.defaultRoutingGroupPreset}
          options={groupOptions}
          onChange={onGroupPresetChange}
        />
      </CompactSettingRow>
      {boundaryNumericFields.map(([field, meta]) => (
        <CompactSettingRow key={field} label={meta.label}>
          <SchedulerNumberInput
            hideLabel
            field={field}
            draft={draft}
            disabled={disabled}
            error={errors[field]}
            onChange={onNumericChange}
          />
        </CompactSettingRow>
      ))}
      {boundaryBooleanFields.map(([field, meta]) => (
        <CompactSettingRow key={field} label={meta.label}>
          <SwitchControl
            ariaLabel={meta.label}
            checked={draft.scheduler[field]}
            disabled={disabled}
            onCheckedChange={() => onBooleanChange(field)}
          />
        </CompactSettingRow>
      ))}
    </>
  );
}

export function LocalRoutingSchedulerFields({
  draft,
  disabled,
  errors,
  onNumericChange,
  onBooleanChange,
}: {
  draft: LocalRoutingSettingsDraft;
  disabled: boolean;
  errors: LocalRoutingSettingsErrors;
  onNumericChange: NumericChangeHandler;
  onBooleanChange: BooleanChangeHandler;
}) {
  return (
    <>
      <PromotedBooleanSettingRow
        field="stickyWeighted"
        draft={draft}
        disabled={disabled}
        onBooleanChange={onBooleanChange}
      />
      {errors.baseWeights ? (
        <div className="border-b border-rose-100 bg-rose-50 px-4 py-2 text-xs text-rose-700">
          {errors.baseWeights}
        </div>
      ) : null}
      <SchedulerFieldGroup
        title="综合评分"
        group="score"
        draft={draft}
        disabled={disabled}
        errors={errors}
        onNumericChange={onNumericChange}
        onBooleanChange={onBooleanChange}
      />
      <SchedulerFieldGroup
        title="粘性与逃逸"
        group="sticky"
        draft={draft}
        disabled={disabled}
        errors={errors}
        onNumericChange={onNumericChange}
        onBooleanChange={onBooleanChange}
      />
      <SchedulerFieldGroup
        title="等待与兜底"
        group="waiting"
        draft={draft}
        disabled={disabled}
        errors={errors}
        onNumericChange={onNumericChange}
        onBooleanChange={onBooleanChange}
      />
    </>
  );
}

function CompactSettingRow({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="grid min-h-12 grid-cols-1 items-center gap-2 border-b border-border px-3 py-2 last:border-b-0 sm:grid-cols-[minmax(0,1fr)_minmax(180px,260px)] sm:gap-4">
      <div className="text-sm font-medium text-slate-800">{label}</div>
      <div className="min-w-0 w-full justify-self-stretch sm:w-auto sm:justify-self-end">
        {children}
      </div>
    </div>
  );
}

function PromotedBooleanSettingRow({
  field,
  draft,
  disabled,
  onBooleanChange,
}: {
  field: SchedulerVisibleBooleanField;
  draft: LocalRoutingSettingsDraft;
  disabled: boolean;
  onBooleanChange: BooleanChangeHandler;
}) {
  const meta = SCHEDULER_BOOLEAN_FIELD_META[field];
  return (
    <div className="flex min-h-12 items-center justify-between gap-4 border-b border-border px-4 py-2">
      <span className="text-sm font-medium text-slate-800">{meta.label}</span>
      <SwitchControl
        ariaLabel={meta.label}
        checked={draft.scheduler[field]}
        disabled={disabled}
        onCheckedChange={() => onBooleanChange(field)}
        showLabel={false}
        className="h-6 min-w-0 border-0 bg-transparent p-0 shadow-none hover:bg-transparent"
      />
    </div>
  );
}

function SchedulerFieldGroup({
  title,
  group,
  draft,
  disabled,
  errors,
  onNumericChange,
  onBooleanChange,
}: {
  title: string;
  group: "score" | "sticky" | "waiting";
  draft: LocalRoutingSettingsDraft;
  disabled: boolean;
  errors: LocalRoutingSettingsErrors;
  onNumericChange: NumericChangeHandler;
  onBooleanChange: BooleanChangeHandler;
}) {
  const numericFields = numericFieldEntries.filter(([, meta]) => meta.group === group);
  const booleanFields = booleanFieldEntries.filter(
    ([field, meta]) => meta.group === group && !PROMOTED_BOOLEAN_FIELDS.has(field),
  );

  return (
    <div
      role="group"
      aria-label={title}
      className="border-b border-border px-4 py-3 last:border-b-0"
    >
      <h3 className="mb-3 text-xs font-semibold text-slate-700">{title}</h3>
      <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
        {numericFields.map(([field]) => (
          <SchedulerNumberInput
            key={field}
            field={field}
            draft={draft}
            disabled={disabled}
            error={errors[field]}
            onChange={onNumericChange}
          />
        ))}
        {booleanFields.map(([field, meta]) => (
          <label key={field} className="grid min-w-0 content-start gap-1.5">
            <span className="text-xs font-medium text-slate-600">{meta.label}</span>
            <SwitchControl
              ariaLabel={meta.label}
              checked={draft.scheduler[field]}
              disabled={disabled}
              onCheckedChange={() => onBooleanChange(field)}
            />
          </label>
        ))}
      </div>
    </div>
  );
}

function SchedulerNumberInput({
  field,
  draft,
  disabled,
  error,
  hideLabel = false,
  onChange,
}: {
  field: SchedulerNumericField;
  draft: LocalRoutingSettingsDraft;
  disabled: boolean;
  error?: string;
  hideLabel?: boolean;
  onChange: NumericChangeHandler;
}) {
  const meta = SCHEDULER_NUMERIC_FIELD_META[field];
  return (
    <LabeledNumberInput
      hideLabel={hideLabel}
      id={`routing-scheduler-${field}`}
      label={meta.label}
      value={draft.scheduler[field]}
      error={error}
      disabled={disabled}
      min="0"
      max={"max" in meta ? meta.max : undefined}
      step={meta.step}
      onChange={(value) => onChange(field, value)}
    />
  );
}

function LabeledNumberInput({
  id,
  label,
  value,
  error,
  disabled,
  min,
  max,
  step,
  placeholder,
  hideLabel = false,
  onChange,
}: {
  id: string;
  label: string;
  value: string;
  error?: string;
  disabled: boolean;
  min?: string;
  max?: string;
  step?: string;
  placeholder?: string;
  hideLabel?: boolean;
  onChange: (value: string) => void;
}) {
  const errorId = `${id}-error`;
  return (
    <label className="grid min-w-0 content-start gap-1.5" htmlFor={id}>
      <span className={hideLabel ? "sr-only" : "text-xs font-medium text-slate-600"}>{label}</span>
      <input
        id={id}
        aria-describedby={error ? errorId : undefined}
        aria-invalid={Boolean(error)}
        className={cn(inputClassName, error && "border-rose-300 focus:border-rose-400 focus:ring-rose-100")}
        disabled={disabled}
        max={max}
        min={min}
        placeholder={placeholder}
        step={step}
        type="number"
        value={value}
        onChange={(event) => onChange(event.target.value)}
      />
      {error ? <span id={errorId} className="text-xs text-rose-700">{error}</span> : null}
    </label>
  );
}

export function isBaseWeightField(field: SchedulerNumericField) {
  return ["multiplier", "priority", "load", "queue", "errorRate", "ttft", "quotaHeadroom"].includes(field);
}
