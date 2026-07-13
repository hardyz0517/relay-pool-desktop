import { useEffect, useMemo, useState, type ReactNode } from "react";
import { ChevronDown, RotateCcw, Save } from "lucide-react";
import { Button, SectionCard, SelectControl, StatusBadge, useToast } from "@/components/ui";
import { getSettings, SETTINGS_UPDATED_EVENT, updateSettings } from "@/lib/api/settings";
import { readError } from "@/lib/errors";
import { appSettingsToUpdateInput, type AppSettings } from "@/lib/types/settings";
import { cn } from "@/lib/utils";
import {
  applyCollectorFrequencyPreset,
  createCollectorSettingsDraft,
  createRecommendedCollectorSettingsDraft,
  detectCollectorFrequencyPreset,
  parseCollectorSettingsDraft,
  type CollectorFrequencyPreset,
  type CollectorSettingsDraft,
  type CollectorSettingsErrors,
  type CollectorSettingsField,
} from "./collectorSettingsForm";

const presetOptions = [
  { value: "timely", label: "及时" },
  { value: "balanced", label: "均衡" },
  { value: "resource_saver", label: "节省资源" },
  { value: "custom", label: "自定义" },
] satisfies Array<{ value: CollectorFrequencyPreset; label: string }>;

export function CollectorAdvancedSettings() {
  const toast = useToast();
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [draft, setDraft] = useState<CollectorSettingsDraft | null>(null);
  const [savedDraft, setSavedDraft] = useState<CollectorSettingsDraft | null>(null);
  const [errors, setErrors] = useState<CollectorSettingsErrors>({});
  const [saving, setSaving] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);

  useEffect(() => {
    void load();
  }, []);

  const dirty = useMemo(
    () => Boolean(draft && savedDraft && JSON.stringify(draft) !== JSON.stringify(savedDraft)),
    [draft, savedDraft],
  );
  const preset = draft ? detectCollectorFrequencyPreset(draft) : "custom";

  async function load() {
    setLoadError(null);
    try {
      const nextSettings = await getSettings();
      const nextDraft = createCollectorSettingsDraft(nextSettings);
      setSettings(nextSettings);
      setDraft(nextDraft);
      setSavedDraft(nextDraft);
      setErrors({});
    } catch (requestError) {
      setLoadError(readError(requestError));
    }
  }

  function updateField(field: CollectorSettingsField, value: string) {
    setErrors((current) => {
      const next = { ...current };
      delete next[field];
      return next;
    });
    setDraft((current) => (current ? { ...current, [field]: value } : current));
  }

  function selectPreset(value: CollectorFrequencyPreset) {
    if (!draft || value === "custom") {
      return;
    }
    setDraft(applyCollectorFrequencyPreset(draft, value));
    setErrors({});
  }

  async function save() {
    if (!settings || !draft || saving) {
      return;
    }
    const parsed = parseCollectorSettingsDraft(draft);
    if (!parsed.ok) {
      setErrors(parsed.errors);
      toast.error("保存采集设置失败", "请修正标记的参数");
      return;
    }
    setSaving(true);
    try {
      const nextSettings = await updateSettings({
        ...appSettingsToUpdateInput(settings),
        ...parsed.value,
      });
      const nextDraft = createCollectorSettingsDraft(nextSettings);
      setSettings(nextSettings);
      setDraft(nextDraft);
      setSavedDraft(nextDraft);
      setErrors({});
      window.dispatchEvent(new Event(SETTINGS_UPDATED_EVENT));
      toast.success("采集设置已保存");
    } catch (requestError) {
      toast.error("保存采集设置失败", readError(requestError));
    } finally {
      setSaving(false);
    }
  }

  if (!draft) {
    return (
      <SectionCard title="采集调度">
        <div className="flex items-center justify-between gap-3 text-sm text-muted-foreground">
          <span>{loadError ?? "正在读取采集设置..."}</span>
          {loadError ? <Button variant="outline" onClick={() => void load()}>重试</Button> : null}
        </div>
      </SectionCard>
    );
  }

  return (
    <SectionCard
      title="采集调度"
      contentClassName="p-0"
      action={
        <div className="flex flex-wrap items-center justify-end gap-2">
          <StatusBadge tone={dirty ? "warning" : "healthy"}>
            {dirty ? "待保存" : "已同步"}
          </StatusBadge>
          <Button
            type="button"
            variant="outline"
            disabled={saving}
            onClick={() => {
              setDraft(createRecommendedCollectorSettingsDraft());
              setErrors({});
            }}
          >
            <RotateCcw className="h-4 w-4" />
            恢复推荐值
          </Button>
          <Button type="button" disabled={saving || !dirty} onClick={() => void save()}>
            <Save className="h-4 w-4" />
            保存采集设置
          </Button>
        </div>
      }
    >
      <SettingRow label="采集频率">
        <SelectControl<CollectorFrequencyPreset>
          ariaLabel="采集频率"
          className="w-full sm:w-[220px]"
          value={preset}
          options={presetOptions}
          onChange={selectPreset}
        />
      </SettingRow>
      <details className="group border-t border-border">
        <summary className="flex cursor-pointer list-none items-center justify-between gap-2 px-3 py-3 text-sm font-medium text-slate-700">
          自定义周期与执行参数
          <ChevronDown className="h-4 w-4 text-muted-foreground transition group-open:rotate-180" />
        </summary>
        <div className="grid gap-3 border-t border-border p-3 sm:grid-cols-2 lg:grid-cols-3">
          <NumberField label="余额周期" field="balanceIntervalMinutes" suffix="分钟" draft={draft} errors={errors} onChange={updateField} />
          <NumberField label="分组 / 倍率周期" field="groupRateIntervalMinutes" suffix="分钟" draft={draft} errors={errors} onChange={updateField} />
          <NumberField label="模型周期" field="modelListIntervalMinutes" suffix="分钟" draft={draft} errors={errors} onChange={updateField} />
          <NumberField label="价格周期" field="pricingRefreshIntervalMinutes" suffix="分钟" draft={draft} errors={errors} onChange={updateField} />
          <NumberField label="采集超时" field="collectorTimeoutSeconds" suffix="秒" draft={draft} errors={errors} onChange={updateField} />
          <NumberField label="采集并发数" field="collectorMaxConcurrency" draft={draft} errors={errors} onChange={updateField} />
        </div>
      </details>
    </SectionCard>
  );
}

function SettingRow({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="grid min-h-12 items-center gap-2 px-3 py-2 sm:grid-cols-[minmax(0,1fr)_minmax(180px,260px)] sm:gap-4">
      <span className="text-sm font-medium text-slate-800">{label}</span>
      <div className="min-w-0 w-full justify-self-stretch sm:w-auto sm:justify-self-end">
        {children}
      </div>
    </div>
  );
}

function NumberField({
  label,
  field,
  suffix,
  draft,
  errors,
  onChange,
}: {
  label: string;
  field: CollectorSettingsField;
  suffix?: string;
  draft: CollectorSettingsDraft;
  errors: CollectorSettingsErrors;
  onChange: (field: CollectorSettingsField, value: string) => void;
}) {
  const id = `collector-setting-${field}`;
  const errorId = `${id}-error`;
  const min = field === "collectorTimeoutSeconds" ? 3 : 1;
  const max = field === "collectorMaxConcurrency" ? 8 : undefined;
  const error = errors[field];

  return (
    <label className="grid min-w-0 gap-1.5" htmlFor={id}>
      <span className="text-xs font-medium text-slate-700">{label}</span>
      <div className="relative min-w-0">
        <input
          id={id}
          aria-describedby={error ? errorId : undefined}
          aria-invalid={Boolean(error)}
          className={cn(
            "h-8 w-full rounded-[var(--surface-radius)] border border-border bg-white px-2.5 text-sm text-slate-800 outline-none focus:border-[hsl(var(--accent)/0.5)] focus:ring-2 focus:ring-[hsl(var(--accent)/0.18)]",
            suffix && "pr-12",
            error && "border-rose-300 focus:border-rose-400 focus:ring-rose-100",
          )}
          min={min}
          max={max}
          step="1"
          type="number"
          value={draft[field]}
          onChange={(event) => onChange(field, event.target.value)}
        />
        {suffix ? (
          <span className="pointer-events-none absolute inset-y-0 right-2.5 flex items-center text-xs text-muted-foreground">
            {suffix}
          </span>
        ) : null}
      </div>
      {error ? <span id={errorId} className="text-xs text-rose-700">{error}</span> : null}
    </label>
  );
}
