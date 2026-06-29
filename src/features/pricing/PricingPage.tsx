import { useEffect, useMemo, useState } from "react";
import { BadgeDollarSign, Layers3, TrendingDown, RefreshCw } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import {
  Button,
  DataTableLite,
  EmptyState,
  InspectorPanel,
  MetricCard,
  SectionCard,
  StatusBadge,
  Toolbar,
  type DataTableColumn,
} from "@/components/ui";
import { listPricingRules } from "@/lib/api/economics";
import { listStations } from "@/lib/api/stations";
import type { PricingRule } from "@/lib/types/economics";
import type { Station } from "@/lib/types/stations";

const sourceTone = {
  manual: "healthy",
  collector: "warning",
  snapshot: "info",
  unknown: "disabled",
} as const;

export function PricingPage() {
  const [pricingRules, setPricingRules] = useState<PricingRule[]>([]);
  const [stations, setStations] = useState<Station[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [selectedStationId, setSelectedStationId] = useState<string>("all");
  const [selectedSource, setSelectedSource] = useState<string>("all");
  const [selectedModel, setSelectedModel] = useState<string | null>(null);

  useEffect(() => {
    void refresh();
  }, []);

  async function refresh() {
    setLoading(true);
    setError(null);
    try {
      const [nextPricing, nextStations] = await Promise.all([listPricingRules(), listStations()]);
      setPricingRules(nextPricing);
      setStations(nextStations);
      setSelectedModel((current) => current ?? nextPricing[0]?.model ?? null);
    } catch (requestError) {
      setError(readError(requestError));
    } finally {
      setLoading(false);
    }
  }

  const stationById = useMemo(() => new Map(stations.map((station) => [station.id, station] as const)), [stations]);

  const filteredRows = useMemo(() => {
    return pricingRules.filter((row) => {
      if (selectedStationId !== "all" && row.stationId !== selectedStationId) {
        return false;
      }
      if (selectedSource !== "all" && row.source !== selectedSource) {
        return false;
      }
      if (query.trim()) {
        const text = `${row.model} ${stationName(row.stationId, stationById)} ${row.groupName ?? ""} ${row.tierLabel ?? ""} ${row.source}`.toLowerCase();
        if (!text.includes(query.trim().toLowerCase())) {
          return false;
        }
      }
      return true;
    });
  }, [pricingRules, query, selectedSource, selectedStationId, stationById]);

  const selected = filteredRows.find((row) => row.model === selectedModel) ?? filteredRows[0] ?? null;
  const cheapest = filteredRows.reduce<PricingRule | null>((min, row) => {
    if (!min) {
      return row;
    }
    const minPrice = estimateOutputPrice(min);
    const rowPrice = estimateOutputPrice(row);
    return rowPrice < minPrice ? row : min;
  }, null);

  const pricingColumns: DataTableColumn<PricingRule>[] = [
    { key: "model", header: "模型", render: (row) => <span className="font-semibold text-slate-800">{row.model}</span> },
    { key: "station", header: "中转站", render: (row) => stationName(row.stationId, stationById) },
    { key: "group", header: "分组/Tier", render: (row) => `${row.groupName ?? "-"} / ${row.tierLabel ?? "-"}` },
    { key: "input", header: "输入 / 1M", className: "text-right", render: (row) => formatMoney(row.inputPrice, row.currency) },
    { key: "output", header: "输出 / 1M", className: "text-right", render: (row) => formatMoney(row.outputPrice, row.currency) },
    { key: "source", header: "来源", render: (row) => <StatusBadge tone={sourceTone[row.source as keyof typeof sourceTone] ?? "info"}>{row.source}</StatusBadge> },
    { key: "confidence", header: "可信度", className: "w-20 text-right", render: (row) => `${Math.round(row.confidence * 100)}%` },
    { key: "updated", header: "更新时间", render: (row) => formatTime(row.updatedAt) },
  ];

  const modelGroups = useMemo(() => {
    const map = new Map<string, PricingRule[]>();
    for (const row of filteredRows) {
      const list = map.get(row.model) ?? [];
      list.push(row);
      map.set(row.model, list);
    }
    return Array.from(map.entries()).map(([model, rows]) => ({ model, rows }));
  }, [filteredRows]);

  return (
    <PageScaffold
      title="价格表"
      description="统一查看已归一化的模型价格与来源；当前展示的是数据库里的真实 pricing_rules。"
      actions={<Button variant="secondary" onClick={() => void refresh()}><RefreshCw className="h-4 w-4" />刷新</Button>}
    >
      <div className="grid gap-[var(--shell-page-gap)] md:grid-cols-3">
        <MetricCard icon={BadgeDollarSign} label="最低输出价" value={cheapest ? formatMoney(cheapest.outputPrice, cheapest.currency) : "暂无"} detail={cheapest?.model ?? "暂无数据"} />
        <MetricCard icon={Layers3} label="覆盖模型" value={`${modelGroups.length}`} detail="真实 pricing_rules" />
        <MetricCard icon={TrendingDown} label="价格记录" value={`${pricingRules.length}`} detail="按站点 / 分组归一化" />
      </div>

      <div className="grid gap-[var(--shell-page-gap)] xl:grid-cols-[minmax(0,1fr)_390px]">
        <SectionCard title="模型价格" description="支持搜索、按站点筛选和按来源筛选。" contentClassName="p-0">
          <Toolbar>
            <div className="flex flex-wrap items-center gap-2">
              <input className={inputClassName} value={query} onChange={(event) => setQuery(event.target.value)} placeholder="搜索模型 / 站点 / 分组" />
              <select className={inputClassName} value={selectedStationId} onChange={(event) => setSelectedStationId(event.target.value)}>
                <option value="all">全部中转站</option>
                {stations.map((station) => <option key={station.id} value={station.id}>{station.name}</option>)}
              </select>
              <select className={inputClassName} value={selectedSource} onChange={(event) => setSelectedSource(event.target.value)}>
                <option value="all">全部来源</option>
                <option value="manual">manual</option>
                <option value="collector">collector</option>
                <option value="snapshot">snapshot</option>
                <option value="unknown">unknown</option>
              </select>
            </div>
          </Toolbar>
          {error && <div className="border-b border-rose-100 bg-rose-50 px-3 py-2 text-sm text-rose-700">{error}</div>}
          {loading ? (
            <div className="px-4 py-5 text-sm text-muted-foreground">正在读取价格表...</div>
          ) : filteredRows.length === 0 ? (
            <EmptyState title="暂无价格数据" description="先在中转站采集价格快照，或手动写入 pricing_rules。" />
          ) : (
            <DataTableLite
              columns={pricingColumns}
              rows={filteredRows}
              getRowKey={(row) => row.id}
              selectedKey={selected?.id}
              onRowClick={(row) => setSelectedModel(row.model)}
              className="rounded-none border-0 shadow-none"
            />
          )}
        </SectionCard>

        <InspectorPanel title={selected ? `${selected.model} inspector` : "价格详情"} description="展示归一化后的真实价格与来源。">
          <div className="space-y-3 p-4">
            {selected ? (
              <>
                <div className="rounded-[var(--surface-radius)] border border-cyan-100 bg-cyan-50/60 p-3 text-sm text-slate-700">
                  推荐：{stationName(selected.stationId, stationById)} · {selected.source} · {selected.enabled ? "启用" : "禁用"}
                </div>
                <DataTableLite
                  columns={[
                    { key: "station", header: "中转站", render: (row: PricingRule) => stationName(row.stationId, stationById) },
                    { key: "input", header: "输入", className: "text-right", render: (row: PricingRule) => formatMoney(row.inputPrice, row.currency) },
                    { key: "output", header: "输出", className: "text-right", render: (row: PricingRule) => formatMoney(row.outputPrice, row.currency) },
                    { key: "unit", header: "单位", render: (row: PricingRule) => `${row.unit} / ${row.priceType}` },
                    { key: "updated", header: "更新时间", render: (row: PricingRule) => formatTime(row.updatedAt) },
                  ]}
                  rows={filteredRows.filter((row) => row.model === selected.model)}
                  getRowKey={(row) => row.id}
                  className="shadow-none"
                />
                <div className="rounded-[var(--surface-radius)] border border-cyan-100 bg-white/80 p-3 text-xs leading-5 text-muted-foreground">
                  价格表只展示归一化后的结果，不直接推导真实计费。余额和单位仍保留原始语义。
                </div>
              </>
            ) : (
              <EmptyState title="选择一条价格记录" description="选择一行后会显示同模型的站点对比。" />
            )}
          </div>
        </InspectorPanel>
      </div>
    </PageScaffold>
  );
}

function stationName(stationId: string, stationById: Map<string, Station>) {
  return stationById.get(stationId)?.name ?? stationId;
}

function estimateOutputPrice(rule: PricingRule) {
  return rule.outputPrice ?? rule.inputPrice ?? rule.fixedPrice ?? Number.POSITIVE_INFINITY;
}

function formatMoney(value: number | null, currency: string) {
  if (value == null) {
    return "暂无";
  }
  return `${currency} ${value.toFixed(4)}`;
}

function formatTime(value: string) {
  const numeric = Number(value);
  const date = Number.isFinite(numeric) && numeric > 1000000000000 ? new Date(numeric) : new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return date.toLocaleString("zh-CN", { month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit" });
}

function readError(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

const inputClassName =
  "h-8 rounded-[12px] border border-cyan-100 bg-cyan-50/45 px-3 text-sm text-slate-800 outline-none transition focus:border-teal-300 focus:bg-white focus:ring-2 focus:ring-teal-100";
