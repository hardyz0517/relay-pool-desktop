import { useEffect, useMemo, useState } from "react";
import { Layers3, RefreshCw, ShieldCheck, TrendingDown } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import {
  Button,
  EmptyState,
  MetricCard,
  SegmentedControl,
  SectionCard,
  SelectControl,
  StatusBadge,
  Toolbar,
  useToast,
  type StatusTone,
} from "@/components/ui";
import { listPricingRules } from "@/lib/api/economics";
import { listGroupRateRecords, listStationGroupBindings } from "@/lib/api/groupFacts";
import { listStations } from "@/lib/api/stations";
import type { PricingRule } from "@/lib/types/economics";
import type { GroupRateRecord, StationGroupBinding } from "@/lib/types/groupFacts";
import type { Station } from "@/lib/types/stations";
import {
  enabledOfficialModelCatalog,
  normalizeCatalogText,
  type OfficialModelProvider,
} from "./officialModelCatalog";
import {
  buildPricingComparisonViewModel,
  type PricingModelEvidence,
  type PricingComparisonRow,
  type PricingComparisonViewModel,
  type PricingModelSection,
} from "./pricingComparisonViewModel";

type ProviderFilter = OfficialModelProvider | "all";
type EmptyReason = PricingComparisonViewModel["emptyReason"];

export function PricingPage() {
  const toast = useToast();
  const [pricingRules, setPricingRules] = useState<PricingRule[]>([]);
  const [stations, setStations] = useState<Station[]>([]);
  const [groupBindings, setGroupBindings] = useState<StationGroupBinding[]>([]);
  const [groupRates, setGroupRates] = useState<GroupRateRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [providerFilter, setProviderFilter] = useState<ProviderFilter>("all");
  const [modelQuery, setModelQuery] = useState("");
  const [selectedStationId, setSelectedStationId] = useState<string>("all");
  const [verifiedOnly, setVerifiedOnly] = useState(false);

  useEffect(() => {
    void refresh();
  }, []);

  async function refresh(showSuccess = false) {
    setLoading(true);
    setError(null);
    try {
      const [nextPricingRules, nextStations] = await Promise.all([
        listPricingRules(),
        listStations(),
      ]);
      const [bindingLists, rateRecordLists] = await Promise.all([
        Promise.all(nextStations.map((station) => listStationGroupBindings(station.id))),
        Promise.all(nextStations.map((station) => listGroupRateRecords(station.id))),
      ]);

      setPricingRules(nextPricingRules);
      setStations(nextStations);
      setGroupBindings(bindingLists.flat());
      setGroupRates(rateRecordLists.flat());

      if (showSuccess) {
        toast.success("价格倍率已刷新");
      }
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      toast.error("刷新价格倍率失败", message);
    } finally {
      setLoading(false);
    }
  }

  const catalogModels = useMemo(() => enabledOfficialModelCatalog(), []);
  const modelEvidence = useMemo<PricingModelEvidence[]>(() => {
    const modelIdByNormalizedName = new Map<string, string>();
    for (const model of catalogModels) {
      for (const name of [model.modelId, ...model.aliases]) {
        modelIdByNormalizedName.set(normalizeCatalogText(name), model.modelId);
      }
    }

    const seen = new Set<string>();
    const evidence: PricingModelEvidence[] = [];
    for (const rule of pricingRules) {
      if (!rule.enabled) {
        continue;
      }
      const modelId = modelIdByNormalizedName.get(normalizeCatalogText(rule.model));
      if (!modelId) {
        continue;
      }
      const evidenceKey = `${rule.stationId}\u0000${modelId}`;
      if (seen.has(evidenceKey)) {
        continue;
      }
      seen.add(evidenceKey);
      evidence.push({
        stationId: rule.stationId,
        modelId,
        status: "discovered",
      });
    }
    return evidence;
  }, [catalogModels, pricingRules]);

  const viewModel = useMemo(
    () =>
      buildPricingComparisonViewModel({
        models: catalogModels,
        stations,
        groupBindings,
        groupRates,
        pricingRules,
        modelEvidence,
        filters: {
          provider: providerFilter,
          modelQuery,
          stationId: selectedStationId,
          verifiedOnly,
        },
      }),
    [
      catalogModels,
      groupBindings,
      groupRates,
      modelEvidence,
      modelQuery,
      pricingRules,
      providerFilter,
      selectedStationId,
      stations,
      verifiedOnly,
    ],
  );

  return (
    <PageScaffold
      title="价格 / 倍率"
      actions={
        <Button variant="secondary" onClick={() => void refresh(true)}>
          <RefreshCw className="h-4 w-4" />
          刷新
        </Button>
      }
    >
      <div className="grid gap-[var(--shell-page-gap)] md:grid-cols-3">
        <MetricCard
          icon={Layers3}
          label="覆盖模型"
          value={`${viewModel.metrics.coveredModelCount}`}
          detail="已有可比价分组的官方模型"
        />
        <MetricCard
          icon={ShieldCheck}
          label="可比价分组"
          value={`${viewModel.metrics.comparableGroupCount}`}
          detail="可折算输入 / 输出价格"
        />
        <MetricCard
          icon={TrendingDown}
          label="最低折算倍率"
          value={
            viewModel.metrics.lowestEffectiveMultiplier === null
              ? "暂无"
              : formatMultiplier(viewModel.metrics.lowestEffectiveMultiplier)
          }
          detail={viewModel.metrics.lowestEffectiveMultiplierLabel || "暂无可比价分组"}
          tone={viewModel.metrics.lowestEffectiveMultiplier === null ? "neutral" : "good"}
        />
      </div>

      <SectionCard title="模型比价" contentClassName="p-0">
        <Toolbar className="items-start">
          <div className="flex w-full flex-wrap items-center gap-2">
            <SegmentedControl
              ariaLabel="按模型提供方筛选"
              value={providerFilter}
              options={[
                { value: "all", label: "全部" },
                { value: "openai", label: "OpenAI" },
                { value: "anthropic", label: "Anthropic" },
                { value: "google", label: "Google" },
              ]}
              onChange={setProviderFilter}
              className="w-full max-w-[360px] sm:w-auto"
            />
            <label className="sr-only" htmlFor="pricing-model-search">
              搜索模型
            </label>
            <input
              id="pricing-model-search"
              className={inputClassName}
              value={modelQuery}
              onChange={(event) => setModelQuery(event.target.value)}
              placeholder="搜索模型"
            />
            <SelectControl
              ariaLabel="按中转站筛选"
              className="w-[180px]"
              value={selectedStationId}
              options={[
                { value: "all", label: "全部中转站" },
                ...stations.map((station) => ({ value: station.id, label: station.name })),
              ]}
              onChange={setSelectedStationId}
            />
            <label className="inline-flex h-8 items-center gap-2 rounded-[var(--surface-radius)] border border-slate-200 bg-white px-3 text-xs font-medium text-slate-700">
              <input
                type="checkbox"
                className="h-3.5 w-3.5 rounded border-slate-300 text-[hsl(var(--accent))] focus:ring-[hsl(var(--accent)/0.25)]"
                checked={verifiedOnly}
                onChange={(event) => setVerifiedOnly(event.target.checked)}
              />
              仅已发现
            </label>
          </div>
        </Toolbar>

        {error && (
          <div className="border-b border-rose-100 bg-rose-50 px-3 py-2 text-sm text-rose-700">
            {error}
          </div>
        )}

        {loading ? (
          <div className="px-4 py-5 text-sm text-muted-foreground">正在读取模型价格与分组倍率...</div>
        ) : viewModel.sections.length === 0 ? (
          <div className="p-4">
            <PricingEmptyState reason={viewModel.emptyReason} />
          </div>
        ) : (
          <div className="divide-y divide-border">
            {viewModel.sections.map((section) => (
              <ModelPricingSection key={section.modelId} section={section} />
            ))}
          </div>
        )}
      </SectionCard>
    </PageScaffold>
  );
}

function ModelPricingSection({ section }: { section: PricingModelSection }) {
  return (
    <section className="grid gap-3 px-4 py-4">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-2">
            <h3 className="text-sm font-semibold text-slate-900">{section.displayName}</h3>
            <StatusBadge tone="info" className="h-5 px-1.5">
              {providerLabel(section.provider)}
            </StatusBadge>
          </div>
          <div className="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
            <span>官方输入 {formatUsd(section.officialInputPrice)} / 1M tokens</span>
            <span>官方输出 {formatUsd(section.officialOutputPrice)} / 1M tokens</span>
            {section.priceSourceUrl ? (
              <a
                className="text-sky-700 underline-offset-2 hover:underline"
                href={section.priceSourceUrl}
                target="_blank"
                rel="noreferrer"
              >
                {section.priceSourceLabel}
              </a>
            ) : (
              <span>{section.priceSourceLabel}</span>
            )}
          </div>
        </div>
        <div className="text-xs text-muted-foreground">{section.modelId}</div>
      </div>

      {section.rows.length === 0 ? (
        <EmptyState title="暂无可比价分组" description="当前筛选下还没有匹配到该模型的分组倍率。" />
      ) : (
        <PricingRowsTable rows={section.rows} />
      )}
    </section>
  );
}

function PricingRowsTable({ rows }: { rows: PricingComparisonRow[] }) {
  return (
    <div className={tableScrollClassName}>
      <table className={tableClassName}>
        <thead>
          <tr className="border-b border-border bg-slate-50">
            <th className={tableHeaderClassName}>中转站</th>
            <th className={tableHeaderClassName}>分组</th>
            <th className={`${tableHeaderClassName} text-right`}>分组倍率</th>
            <th className={`${tableHeaderClassName} text-right`}>充值倍率</th>
            <th className={`${tableHeaderClassName} text-right`}>折算倍率</th>
            <th className={`${tableHeaderClassName} text-right`}>输入价</th>
            <th className={`${tableHeaderClassName} text-right`}>输出价</th>
            <th className={tableHeaderClassName}>证据</th>
            <th className={tableHeaderClassName}>更新时间</th>
          </tr>
        </thead>
        <tbody className="divide-y divide-border">
          {rows.map((row) => (
            <tr key={row.id} className={row.isCheapest ? cheapestRowClassName : undefined}>
              <td className={tableCellClassName}>
                <div className="font-medium text-slate-800">{row.stationName}</div>
                <div className="text-xs text-muted-foreground">{row.source || "未知来源"}</div>
              </td>
              <td className={tableCellClassName}>
                <div className="font-medium text-slate-800">{row.groupName}</div>
                {row.isCheapest && (
                  <div className="mt-0.5 text-xs font-medium text-emerald-700">当前最低</div>
                )}
              </td>
              <td className={`${tableCellClassName} text-right tabular-nums`}>
                {formatNullableMultiplier(row.groupMultiplier)}
              </td>
              <td className={`${tableCellClassName} text-right tabular-nums`}>
                {formatRecharge(row.creditPerCny)}
              </td>
              <td className={`${tableCellClassName} text-right tabular-nums font-semibold text-slate-800`}>
                {formatNullableMultiplier(row.effectiveMultiplier)}
              </td>
              <td className={`${tableCellClassName} text-right tabular-nums`}>
                {formatCny(row.estimatedInputCny)}
              </td>
              <td className={`${tableCellClassName} text-right tabular-nums`}>
                {formatCny(row.estimatedOutputCny)}
              </td>
              <td className={tableCellClassName}>
                <StatusBadge tone={evidenceTone(row.evidenceStatus)} className="h-5 px-1.5">
                  {row.evidenceLabel}
                </StatusBadge>
              </td>
              <td className={`${tableCellClassName} whitespace-nowrap text-muted-foreground`}>
                {formatTime(row.checkedAt)}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function PricingEmptyState({ reason }: { reason: EmptyReason }) {
  if (reason === "no_catalog_models") {
    return (
      <EmptyState
        title="暂无官方模型目录"
        description="启用官方模型目录后，这里会按具体模型展示分组倍率折算。"
      />
    );
  }

  if (reason === "no_group_rates") {
    return (
      <EmptyState
        title="暂无分组倍率"
        description="先采集中转站分组与倍率记录，再按官方模型折算可比价。"
      />
    );
  }

  if (reason === "filtered_empty") {
    return (
      <EmptyState
        title="没有匹配的模型分组"
        description="调整提供方、模型关键词、中转站或已发现筛选后再试。"
      />
    );
  }

  return (
    <EmptyState
      title="暂无模型比价"
      description="采集分组倍率后，这里会显示按具体模型整理的可比价表。"
    />
  );
}

function evidenceTone(status: PricingComparisonRow["evidenceStatus"]): StatusTone {
  if (status === "discovered") {
    return "healthy";
  }
  if (status === "unavailable") {
    return "error";
  }
  return "info";
}

function formatNullableMultiplier(value: number | null) {
  return value === null ? "倍率未知" : formatMultiplier(value);
}

function formatMultiplier(value: number) {
  return `${formatDecimal(value, 6)}x`;
}

function formatRecharge(value: number) {
  return formatMultiplier(value);
}

function formatCny(value: number | null) {
  return value === null ? "暂无" : `¥${formatDecimal(value, 4)}`;
}

function formatUsd(value: number) {
  return `$${formatDecimal(value, 4)}`;
}

function formatTime(value: string | null) {
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

function readError(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function formatDecimal(value: number, fractionDigits: number) {
  return value.toFixed(fractionDigits).replace(/0+$/, "").replace(/\.$/, "");
}

function providerLabel(provider: OfficialModelProvider) {
  if (provider === "openai") {
    return "OpenAI";
  }
  if (provider === "anthropic") {
    return "Anthropic";
  }
  return "Google";
}

const inputClassName =
  "h-8 w-[180px] rounded-[var(--surface-radius)] border border-slate-200 bg-white px-3 text-sm text-slate-800 outline-none transition placeholder:text-slate-400 focus:border-[hsl(var(--accent)/0.45)] focus:ring-2 focus:ring-[hsl(var(--accent)/0.16)]";

const tableScrollClassName = "overflow-x-auto rounded-[var(--surface-radius)] border border-border";
const tableClassName = "min-w-[980px] w-full table-fixed text-left text-sm";
const tableHeaderClassName = "px-3 py-2 text-xs font-semibold text-muted-foreground";
const tableCellClassName = "px-3 py-2.5 align-top text-sm text-slate-700";
const cheapestRowClassName = "bg-emerald-50/70";
