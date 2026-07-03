import { useMemo, useState, type FormEvent } from "react";
import { ArrowLeft, Check, KeyRound, Server, Sparkles } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import { Button, Card, IconButton, PageForm, SectionCard, SelectControl, StatusBadge, useToast } from "@/components/ui";
import { createStation } from "@/lib/api/stations";
import { stationTypeLabels, type StationType } from "@/lib/types/stations";
import { cn } from "@/lib/utils";
import { providerPresets, type ProviderPresetId } from "./providerPresets";

type AddProviderPageProps = {
  onBack: () => void;
  onCreated: () => void;
};

type AddProviderFormState = {
  presetId: ProviderPresetId;
  name: string;
  stationType: StationType;
  baseUrl: string;
  apiKey: string;
  lowBalanceThresholdCny: string;
  note: string;
};

const defaultPreset = providerPresets[1];

const inputClassName =
  "h-8 rounded-[var(--surface-radius)] border border-border bg-white px-3 text-sm text-slate-800 outline-none transition focus:border-[hsl(var(--accent)/0.5)] focus:ring-2 focus:ring-[hsl(var(--accent)/0.18)]";

export function AddProviderPage({ onBack, onCreated }: AddProviderPageProps) {
  const toast = useToast();
  const [form, setForm] = useState<AddProviderFormState>({
    presetId: defaultPreset.id,
    name: defaultPreset.name,
    stationType: defaultPreset.stationType,
    baseUrl: defaultPreset.baseUrl,
    apiKey: "",
    lowBalanceThresholdCny: "",
    note: "",
  });
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const activePreset = useMemo(
    () => providerPresets.find((preset) => preset.id === form.presetId) ?? defaultPreset,
    [form.presetId],
  );

  function applyPreset(presetId: ProviderPresetId) {
    const preset = providerPresets.find((item) => item.id === presetId) ?? defaultPreset;
    setForm((current) => ({
      ...current,
      presetId: preset.id,
      name: preset.name,
      stationType: preset.stationType,
      baseUrl: preset.baseUrl,
    }));
    setError(null);
  }

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!form.name.trim()) {
      toast.info("请填写 Provider 名称");
      return;
    }
    if (!form.baseUrl.trim()) {
      toast.info("请填写 Base URL");
      return;
    }
    if (!form.apiKey.trim()) {
      toast.info("请填写 API Key");
      return;
    }

    setSaving(true);
    setError(null);
    try {
      await createStation({
        name: form.name.trim(),
        stationType: form.stationType,
        baseUrl: form.baseUrl.trim(),
        apiKey: form.apiKey.trim(),
        enabled: true,
        creditPerCny: 1,
        lowBalanceThresholdCny: form.lowBalanceThresholdCny.trim()
          ? Number(form.lowBalanceThresholdCny)
          : null,
        note: form.note.trim() ? form.note.trim() : null,
      });
      toast.success("Provider 已添加");
      onCreated();
    } catch (requestError) {
      const message = requestError instanceof Error ? requestError.message : String(requestError);
      setError(message);
      toast.error("添加 Provider 失败", message);
    } finally {
      setSaving(false);
    }
  }

  return (
    <PageScaffold
      title="添加 Provider"
      description="先选择常见预设，再补齐 Base URL 和 API Key。"
      backAction={
        <IconButton label="返回中转站" onClick={onBack}>
          <ArrowLeft className="h-4 w-4" />
        </IconButton>
      }
      status={<StatusBadge tone="info">{stationTypeLabels[form.stationType]}</StatusBadge>}
    >
      <PageForm
        className="w-full"
        onSubmit={handleSubmit}
        footer={
          <>
            <Button variant="secondary" onClick={onBack} disabled={saving}>
              取消
            </Button>
            <Button type="submit" disabled={saving}>
              <Check className="h-4 w-4" />
              {saving ? "添加中" : "添加 Provider"}
            </Button>
          </>
        }
      >
        <section className="grid gap-[var(--shell-page-gap)]">
          <div className="grid gap-[var(--shell-page-gap)]">
            <SectionCard
              title="预设供应商"
              description="选择后会自动填充站点类型和 Base URL；自定义配置可手动补齐。"
            >
              <div className="grid grid-cols-[repeat(auto-fit,minmax(min(100%,9rem),1fr))] gap-2">
                {providerPresets.map((preset) => {
                  const selected = preset.id === form.presetId;
                  return (
                    <button
                      key={preset.id}
                      type="button"
                      className={cn(
                        "relative flex h-8 min-w-0 cursor-pointer items-center gap-2 rounded-[var(--surface-radius)] px-2.5 text-left text-xs font-medium transition-colors",
                        selected
                          ? "bg-[hsl(var(--accent))] text-white shadow-sm"
                          : "bg-slate-100 text-slate-600 hover:bg-slate-200 hover:text-slate-900",
                      )}
                      onClick={() => applyPreset(preset.id)}
                      title={preset.description}
                    >
                      <span
                        className={cn(
                          "flex h-4.5 w-4.5 shrink-0 items-center justify-center rounded-[5px] bg-white text-[10px] font-semibold text-slate-600",
                          selected && "text-[hsl(var(--accent))]",
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

            <SectionCard title="连接信息" description="这些字段会写入现有中转站配置，不改变后端数据结构。">
              <div className="grid gap-3 md:grid-cols-2">
                <Field label="Provider 名称">
                  <input
                    className={inputClassName}
                    value={form.name}
                    onChange={(event) => setForm({ ...form, name: event.target.value })}
                    placeholder="例如 DeepSeek"
                  />
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
                    onChange={(stationType) => setForm({ ...form, stationType })}
                  />
                </Field>
              </div>
              <div className="mt-3 grid gap-3">
                <Field label="Base URL">
                  <input
                    className={inputClassName}
                    value={form.baseUrl}
                    onChange={(event) => setForm({ ...form, baseUrl: event.target.value })}
                    placeholder="https://api.example.com/v1"
                  />
                </Field>
                <Field label="API Key">
                  <input
                    className={inputClassName}
                    type="password"
                    value={form.apiKey}
                    onChange={(event) => setForm({ ...form, apiKey: event.target.value })}
                    placeholder="sk-..."
                  />
                </Field>
              </div>
              {error && (
                <div className="mt-3 rounded-[var(--surface-radius)] border border-rose-200 bg-rose-50 px-3 py-2 text-sm text-rose-700">
                  {error}
                </div>
              )}
            </SectionCard>
          </div>

          <aside className="grid content-start gap-[var(--shell-page-gap)]">
            <Card className="p-4">
              <div className="flex items-start gap-3">
                <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-[var(--surface-radius)] bg-slate-100 text-slate-700">
                  <Sparkles className="h-4 w-4" />
                </div>
                <div className="min-w-0">
                  <div className="text-[13px] font-semibold text-slate-900">{activePreset.name}</div>
                  <div className="mt-1 text-xs leading-5 text-muted-foreground">{activePreset.description}</div>
                </div>
              </div>
              <div className="mt-4 grid gap-2 text-xs">
                <SummaryRow icon={<Server className="h-3.5 w-3.5" />} label="类型" value={stationTypeLabels[form.stationType]} />
                <SummaryRow icon={<KeyRound className="h-3.5 w-3.5" />} label="Key" value={form.apiKey.trim() ? "已填写" : "待填写"} />
              </div>
            </Card>

            <SectionCard title="可选项" description="低余额阈值和备注可稍后在中转站详情里调整。">
              <div className="grid gap-3">
                <Field label="低余额阈值 CNY">
                  <input
                    className={inputClassName}
                    min="0"
                    step="0.01"
                    type="number"
                    value={form.lowBalanceThresholdCny}
                    onChange={(event) => setForm({ ...form, lowBalanceThresholdCny: event.target.value })}
                    placeholder="使用全局设置"
                  />
                </Field>
                <Field label="备注">
                  <textarea
                    className={`${inputClassName} min-h-24 resize-none py-2`}
                    value={form.note}
                    onChange={(event) => setForm({ ...form, note: event.target.value })}
                    placeholder="登录方式、模型限制或计费说明"
                  />
                </Field>
              </div>
            </SectionCard>
          </aside>
        </section>
      </PageForm>
    </PageScaffold>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <label className="grid gap-1.5 text-xs font-medium text-muted-foreground">
      {label}
      {children}
    </label>
  );
}

function SummaryRow({ icon, label, value }: { icon: React.ReactNode; label: string; value: string }) {
  return (
    <div className="flex items-center justify-between gap-3 rounded-[var(--surface-radius)] border border-border bg-slate-50 px-3 py-2">
      <div className="flex items-center gap-2 text-muted-foreground">
        {icon}
        <span>{label}</span>
      </div>
      <span className="min-w-0 truncate text-right font-medium text-slate-800">{value}</span>
    </div>
  );
}
