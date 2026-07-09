import { useEffect, useMemo, useState } from "react";
import { ArrowLeft, Plus, RefreshCw, RotateCcw, Save } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import { Button, IconButton, SectionCard, StatusBadge, SwitchControl, useToast } from "@/components/ui";
import { listModelBasePrices, resetModelBasePricesToBuiltins, upsertModelBasePrice } from "@/lib/api/economics";
import { readError } from "@/lib/errors";
import type { ModelBasePrice } from "@/lib/types/economics";

type ModelBasePricesPageProps = {
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

const emptyDraft: DraftRow = {
  provider: "custom",
  model: "",
  inputPrice: "",
  outputPrice: "",
  currency: "USD",
  unit: "per_1m_tokens",
  sourceUrl: "",
  sourceLabel: "Manual",
  sourceCheckedAt: new Date().toISOString().slice(0, 10),
  enabled: true,
  builtIn: false,
  note: "",
};

export function ModelBasePricesPage({ onBack }: ModelBasePricesPageProps) {
  const toast = useToast();
  const [rows, setRows] = useState<ModelBasePrice[]>([]);
  const [draft, setDraft] = useState<DraftRow>(emptyDraft);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
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
      if (selectedId) {
        const selected = nextRows.find((row) => row.id === selectedId);
        if (selected) {
          setDraft(priceToDraft(selected));
        }
      }
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

  async function saveDraft() {
    setSaving(true);
    setError(null);
    try {
      const saved = await upsertModelBasePrice(draftToInput(draft));
      toast.success("模型基准价格已保存");
      setSelectedId(saved.id);
      setDraft(priceToDraft(saved));
      await refresh();
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      toast.error("保存模型基准价格失败", message);
    } finally {
      setSaving(false);
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

  const metrics = useMemo(() => {
    const enabled = rows.filter((row) => row.enabled).length;
    const builtIn = rows.filter((row) => row.builtIn).length;
    return { enabled, builtIn, total: rows.length };
  }, [rows]);

  return (
    <PageScaffold
      title="模型基准价格"
      stickyHeader
      backAction={
        <IconButton label="返回价格 / 倍率" onClick={onBack}>
          <ArrowLeft className="h-4 w-4" />
        </IconButton>
      }
      actions={
        <>
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
          contentClassName="overflow-auto rounded-none border-0 bg-transparent p-0 shadow-none"
        >
          <table className="w-full min-w-[900px] border-collapse bg-white text-left text-[13px]">
            <thead className="bg-teal-50/70 text-[11px] font-medium uppercase tracking-wide text-slate-500">
              <tr>
                <th className="h-8 px-2.5">模型</th>
                <th className="h-8 px-2.5">供应商</th>
                <th className="h-8 px-2.5">输入 / 输出</th>
                <th className="h-8 px-2.5">来源</th>
                <th className="h-8 px-2.5">状态</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-border">
              {rows.map((row) => (
                <tr
                  key={row.id}
                  className="h-10 cursor-pointer text-slate-700 hover:bg-teal-50/55"
                  onClick={() => {
                    setSelectedId(row.id);
                    setDraft(priceToDraft(row));
                  }}
                >
                  <td className="px-2.5 font-medium text-slate-800">{row.model}</td>
                  <td className="px-2.5 uppercase text-slate-600">{row.provider}</td>
                  <td className="px-2.5 tabular-nums text-slate-700">
                    ${formatPrice(row.inputPrice)} / ${formatPrice(row.outputPrice)}
                  </td>
                  <td className="px-2.5">
                    <a
                      className="text-[hsl(var(--accent))] hover:underline"
                      href={row.sourceUrl}
                      rel="noreferrer"
                      target="_blank"
                    >
                      {row.sourceLabel}
                    </a>
                  </td>
                  <td className="px-2.5">
                    <div className="flex items-center gap-2">
                      <StatusBadge tone={row.enabled ? "healthy" : "disabled"}>
                        {row.enabled ? "启用" : "停用"}
                      </StatusBadge>
                      {row.builtIn && <StatusBadge tone="info">内置</StatusBadge>}
                    </div>
                  </td>
                </tr>
              ))}
              {!loading && rows.length === 0 && (
                <tr>
                  <td className="px-2.5 py-8 text-center text-sm text-muted-foreground" colSpan={5}>
                    暂无模型基准价格
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </SectionCard>

        <SectionCard
          title={draft.id ? "编辑基准价格" : "新增基准价格"}
          action={
            <Button
              variant="outline"
              onClick={() => {
                setSelectedId(null);
                setDraft(emptyDraft);
              }}
            >
              <Plus className="h-4 w-4" />
              新增
            </Button>
          }
        >
          <div className="grid gap-3 md:grid-cols-2">
            <Field label="供应商" value={draft.provider} onChange={(provider) => setDraft({ ...draft, provider })} />
            <Field label="模型" value={draft.model} onChange={(model) => setDraft({ ...draft, model })} />
            <Field label="输入价" numeric value={draft.inputPrice} onChange={(inputPrice) => setDraft({ ...draft, inputPrice })} />
            <Field label="输出价" numeric value={draft.outputPrice} onChange={(outputPrice) => setDraft({ ...draft, outputPrice })} />
            <Field label="币种" value={draft.currency} onChange={(currency) => setDraft({ ...draft, currency })} />
            <Field label="单位" value={draft.unit} onChange={(unit) => setDraft({ ...draft, unit })} />
            <Field label="来源名称" value={draft.sourceLabel} onChange={(sourceLabel) => setDraft({ ...draft, sourceLabel })} />
            <Field label="检查日期" value={draft.sourceCheckedAt} onChange={(sourceCheckedAt) => setDraft({ ...draft, sourceCheckedAt })} />
            <div className="md:col-span-2">
              <Field label="来源 URL" value={draft.sourceUrl} onChange={(sourceUrl) => setDraft({ ...draft, sourceUrl })} />
            </div>
            <div className="md:col-span-2">
              <Field label="备注" value={draft.note} onChange={(note) => setDraft({ ...draft, note })} />
            </div>
            <div className="flex items-center justify-between rounded-[var(--surface-radius)] border border-border bg-slate-50 px-3 py-2">
              <div className="text-sm font-medium text-slate-800">启用</div>
              <SwitchControl
                ariaLabel="启用模型基准价格"
                checked={draft.enabled}
                offLabel="停用"
                onLabel="启用"
                onCheckedChange={() => setDraft({ ...draft, enabled: !draft.enabled })}
              />
            </div>
          </div>
          <div className="mt-4 flex justify-end">
            <Button disabled={saving || !draft.model.trim()} onClick={() => void saveDraft()}>
              <Save className="h-4 w-4" />
              {saving ? "保存中" : "保存"}
            </Button>
          </div>
        </SectionCard>

        {error && <div className="text-sm text-rose-700">{error}</div>}
      </div>
    </PageScaffold>
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
    <label className="grid gap-1 text-xs font-medium text-slate-600">
      <span>{label}</span>
      <input
        className="h-8 min-w-0 rounded-[var(--surface-radius)] border border-border bg-white px-3 text-sm text-slate-800 outline-none transition focus:border-[hsl(var(--accent)/0.5)] focus:ring-2 focus:ring-[hsl(var(--accent)/0.18)]"
        step={numeric ? "0.0001" : undefined}
        type={numeric ? "number" : "text"}
        value={value}
        onChange={(event) => onChange(event.target.value)}
      />
    </label>
  );
}

function priceToDraft(price: ModelBasePrice): DraftRow {
  return {
    id: price.id,
    provider: price.provider,
    model: price.model,
    inputPrice: price.inputPrice === null ? "" : String(price.inputPrice),
    outputPrice: price.outputPrice === null ? "" : String(price.outputPrice),
    currency: price.currency,
    unit: price.unit,
    sourceUrl: price.sourceUrl,
    sourceLabel: price.sourceLabel,
    sourceCheckedAt: price.sourceCheckedAt ?? "",
    enabled: price.enabled,
    builtIn: price.builtIn,
    note: price.note ?? "",
  };
}

function draftToInput(draft: DraftRow) {
  return {
    id: draft.id,
    provider: draft.provider,
    model: draft.model,
    inputPrice: draft.inputPrice.trim() === "" ? null : Number(draft.inputPrice),
    outputPrice: draft.outputPrice.trim() === "" ? null : Number(draft.outputPrice),
    currency: draft.currency,
    unit: draft.unit,
    sourceUrl: draft.sourceUrl,
    sourceLabel: draft.sourceLabel,
    sourceCheckedAt: draft.sourceCheckedAt.trim() === "" ? null : draft.sourceCheckedAt,
    enabled: draft.enabled,
    builtIn: draft.builtIn,
    note: draft.note.trim() === "" ? null : draft.note,
  };
}

function formatPrice(value: number | null) {
  if (value === null) {
    return "未设";
  }
  return Number.isInteger(value) ? value.toFixed(0) : value.toString();
}
