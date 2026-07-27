import type { ReactNode } from "react";
import { Check } from "lucide-react";
import { SectionCard, SelectControl } from "@/components/ui";
import { DEFAULT_MANUAL_PROXY_URL, withManualProxyDefault } from "@/lib/proxyDefaults";
import { stationProxyModeLabels, type StationProxyMode } from "@/lib/types/stations";
import { cn } from "@/lib/utils";
import { providerPresets, type ProviderPresetId } from "../../providerPresets";
import { inputClassName, type AddProviderFormState } from "./formModel";

export function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="grid gap-1.5 text-xs font-medium text-muted-foreground">
      {label}
      {children}
    </label>
  );
}

type ProviderPresetSectionProps = {
  presetId: ProviderPresetId;
  onApplyPreset: (presetId: ProviderPresetId) => void;
};

export function ProviderPresetSection({ presetId, onApplyPreset }: ProviderPresetSectionProps) {
  return (
    <SectionCard title="预设供应商">
      <div className="grid grid-cols-[repeat(auto-fit,minmax(min(100%,9rem),1fr))] gap-2">
        {providerPresets.map((preset) => {
          const selected = preset.id === presetId;
          return (
            <button
              key={preset.id}
              type="button"
              className={cn(
                "relative flex h-8 min-w-0 cursor-pointer items-center gap-2 rounded-[var(--surface-radius)] px-2.5 text-left text-xs font-medium transition-colors",
                selected
                  ? "bg-primary-solid text-primary-foreground shadow-sm"
                  : "bg-muted text-muted-foreground hover:bg-hover hover:text-foreground",
              )}
              onClick={() => onApplyPreset(preset.id)}
              title={preset.description}
            >
              <span
                className={cn(
                  "flex h-4.5 w-4.5 shrink-0 items-center justify-center rounded-[5px] bg-surface text-[10px] font-semibold text-muted-foreground",
                  selected && "text-primary",
                )}
              >
                {preset.name.slice(0, 1)}
              </span>
              <span className="min-w-0 truncate">{preset.name}</span>
              {selected && <Check className="ml-auto h-3.5 w-3.5 shrink-0" />}
            </button>
          );
        })}
      </div>
    </SectionCard>
  );
}

type ProviderOptionsSectionProps = {
  form: AddProviderFormState;
  onFormChange: (form: AddProviderFormState) => void;
};

export function ProviderOptionsSection({ form, onFormChange }: ProviderOptionsSectionProps) {
  return (
    <SectionCard title="可选项">
      <div className="grid gap-3">
        <Field label="低余额阈值 CNY">
          <input
            className={inputClassName}
            min="0"
            step="0.01"
            type="number"
            value={form.lowBalanceThresholdCny}
            onChange={(event) => onFormChange({ ...form, lowBalanceThresholdCny: event.target.value })}
            placeholder="使用全局设置"
          />
        </Field>
        <Field label="兑换比例">
          <input
            className={inputClassName}
            min="0.01"
            step="0.01"
            type="number"
            value={form.creditPerCny}
            onChange={(event) => onFormChange({ ...form, creditPerCny: event.target.value })}
          />
        </Field>
        <Field label="采集频率 分钟">
          <input
            className={inputClassName}
            min="1"
            step="1"
            type="number"
            value={form.collectionIntervalMinutes}
            onChange={(event) => onFormChange({ ...form, collectionIntervalMinutes: event.target.value })}
            placeholder="5"
          />
        </Field>
        <Field label="采集代理">
          <div className="grid gap-2">
            <SelectControl
              ariaLabel="站点采集代理"
              className={inputClassName}
              value={form.collectorProxyMode}
              options={Object.entries(stationProxyModeLabels).map(([value, label]) => ({
                value: value as StationProxyMode,
                label,
              }))}
              onChange={(collectorProxyMode) => {
                const nextForm = { ...form, collectorProxyMode };
                onFormChange(
                  collectorProxyMode === "manual"
                    ? withManualProxyDefault(nextForm)
                    : nextForm,
                );
              }}
            />
            {form.collectorProxyMode === "manual" && (
              <input
                className={inputClassName}
                placeholder={DEFAULT_MANUAL_PROXY_URL}
                value={form.collectorProxyUrl}
                onChange={(event) => onFormChange({ ...form, collectorProxyUrl: event.target.value })}
              />
            )}
            <p className="text-xs text-muted-foreground">
              登录刷新、余额/分组采集、远端 Key 和本地 key 路由都会使用该站点的有效代理。
            </p>
          </div>
        </Field>
        <Field label="备注">
          <textarea
            className={`${inputClassName} min-h-24 resize-none py-2`}
            value={form.note}
            onChange={(event) => onFormChange({ ...form, note: event.target.value })}
            placeholder="登录方式、模型限制或计费说明"
          />
        </Field>
      </div>
    </SectionCard>
  );
}
