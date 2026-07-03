import { useEffect, useMemo, useState } from "react";
import { BadgeDollarSign, Layers3, TrendingDown, RefreshCw } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import {
  Button,
  DataTableLite,
  EmptyState,
  InspectorPanel,
  MetricCard,
  SegmentedControl,
  SectionCard,
  SelectControl,
  Toolbar,
  useToast,
} from "@/components/ui";
import { getLatestCollectorSnapshot } from "@/lib/api/collector";
import { listPricingRules } from "@/lib/api/economics";
import { listStations } from "@/lib/api/stations";
import type { PricingRule } from "@/lib/types/economics";
import type { Station } from "@/lib/types/stations";
import { buildPriceMatrix, buildRateMatrix } from "./pricingMatrix";
import { parseRateMultipliers, type RateMultiplierRow } from "./rateSnapshotParser";

type MatrixTone = "good" | "warning" | "neutral" | "muted";

export function PricingPage() {
  const toast = useToast();
  const [pricingRules, setPricingRules] = useState<PricingRule[]>([]);
  const [stations, setStations] = useState<Station[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [selectedStationId, setSelectedStationId] = useState<string>("all");
  const [selectedSource, setSelectedSource] = useState<string>("all");
  const [selectedModel, setSelectedModel] = useState<string | null>(null);
  const [viewMode, setViewMode] = useState<"prices" | "rates" | "availability">("prices");
  const [rateRows, setRateRows] = useState<RateMultiplierRow[]>([]);

  useEffect(() => {
    void refresh();
  }, []);

  async function refresh(showSuccess = false) {
    setLoading(true);
    setError(null);
    try {
      const [nextPricing, nextStations] = await Promise.all([listPricingRules(), listStations()]);
      const snapshots = await Promise.all(nextStations.map((station) => getLatestCollectorSnapshot(station.id)));
      setPricingRules(nextPricing);
      setStations(nextStations);
      setRateRows(snapshots.flatMap(parseRateMultipliers));
      setSelectedModel((current) => current ?? nextPricing[0]?.model ?? null);
      if (showSuccess) {
        toast.success("价格表已刷新");
      }
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      toast.error("刷新价格表失败", message);
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

  const modelGroups = useMemo(() => {
    const map = new Map<string, PricingRule[]>();
    for (const row of filteredRows) {
      const list = map.get(row.model) ?? [];
      list.push(row);
      map.set(row.model, list);
    }
    return Array.from(map.entries()).map(([model, rows]) => ({ model, rows }));
  }, [filteredRows]);
  const priceMatrix = useMemo(() => buildPriceMatrix(filteredRows, stations), [filteredRows, stations]);
  const rateMatrix = useMemo(() => buildRateMatrix(rateRows, stations), [rateRows, stations]);

  return (
    <PageScaffold
      title="价格 / 倍率"
      description="跨站点比较模型价格、分组倍率和模型可用性，不再按数据库表视角浏览 pricing_rules。"
      actions={<Button variant="secondary" onClick={() => void refresh(true)}><RefreshCw className="h-4 w-4" />刷新</Button>}
    >
      <div className="grid gap-[var(--shell-page-gap)] md:grid-cols-3">
        <MetricCard icon={BadgeDollarSign} label="最低输出价" value={cheapest ? formatMoney(cheapest.outputPrice, cheapest.currency) : "暂无"} detail={cheapest?.model ?? "暂无数据"} />
        <MetricCard icon={Layers3} label="覆盖模型" value={`${modelGroups.length}`} detail="真实 pricing_rules" />
        <MetricCard icon={TrendingDown} label="价格记录" value={`${pricingRules.length}`} detail="按站点 / 分组归一化" />
      </div>

      <div className="grid gap-[var(--shell-page-gap)]">
        <SectionCard title="跨站点对比" description="按模型、分组倍率和可用性比较中转站。" contentClassName="p-0">
          <Toolbar>
            <div className="flex flex-wrap items-center gap-2">
              <SegmentedControl
                value={viewMode}
                options={[
                  { value: "prices", label: "模型价格" },
                  { value: "rates", label: "分组倍率" },
                  { value: "availability", label: "模型可用性" },
                ]}
                onChange={setViewMode}
              />
              <input className={inputClassName} value={query} onChange={(event) => setQuery(event.target.value)} placeholder="搜索模型 / 站点 / 分组" />
              <SelectControl
                ariaLabel="按中转站筛选价格"
                className={inputClassName}
                value={selectedStationId}
                options={[
                  { value: "all", label: "全部中转站" },
                  ...stations.map((station) => ({ value: station.id, label: station.name })),
                ]}
                onChange={setSelectedStationId}
              />
              <SelectControl
                ariaLabel="按来源筛选价格"
                className={inputClassName}
                value={selectedSource}
                options={[
                  { value: "all", label: "全部来源" },
                  { value: "manual", label: "manual" },
                  { value: "collector", label: "collector" },
                  { value: "snapshot", label: "snapshot" },
                  { value: "unknown", label: "unknown" },
                ]}
                onChange={setSelectedSource}
              />
            </div>
          </Toolbar>
          {error && <div className="border-b border-rose-100 bg-rose-50 px-3 py-2 text-sm text-rose-700">{error}</div>}
          {loading ? (
            <div className="px-4 py-5 text-sm text-muted-foreground">正在读取价格表...</div>
          ) : filteredRows.length === 0 ? (
            <EmptyState title="暂无价格数据" description="先在中转站采集价格快照，或手动写入 pricing_rules。" />
          ) : viewMode === "prices" ? (
            <MatrixTable
              rowHeader="模型"
              stations={stations}
              rows={priceMatrix.map((row) => ({
                key: row.model,
                label: row.model,
                cells: row.cells.map((cell) => ({
                  stationId: cell.stationId,
                  content: cell.available ? formatPriceCell(cell) : "不可用",
                  tone: cell.isCheapestOutput ? "good" : cell.available ? "neutral" : "muted",
                })),
              }))}
            />
          ) : viewMode === "rates" ? (
            <MatrixTable
              rowHeader="分组"
              stations={stations}
              rows={rateMatrix.map((row) => ({
                key: row.groupName,
                label: row.groupName,
                cells: row.cells.map((cell) => ({
                  stationId: cell.stationId,
                  content: cell.multiplier == null ? "未采集" : `${cell.multiplier.toFixed(2)}x`,
                  tone: cell.multiplier == null ? "muted" : cell.multiplier > 1 ? "warning" : cell.multiplier < 1 ? "good" : "neutral",
                })),
              }))}
            />
          ) : (
            <MatrixTable
              rowHeader="模型"
              stations={stations}
              rows={priceMatrix.map((row) => ({
                key: row.model,
                label: row.model,
                cells: row.cells.map((cell) => ({
                  stationId: cell.stationId,
                  content: cell.available ? "可用" : "不可用",
                  tone: cell.available ? "good" : "muted",
                })),
              }))}
            />
          )}
        </SectionCard>

        <InspectorPanel title={selected ? `${selected.model} 对比详情` : "对比详情"} description="展示归一化后的真实价格与来源。">
          <div className="space-y-3 p-4">
            {selected ? (
              <>
                <div className="rounded-[var(--surface-radius)] border border-border bg-slate-50 p-3 text-sm text-slate-700">
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
                <div className="rounded-[var(--surface-radius)] border border-border bg-white p-3 text-xs leading-5 text-muted-foreground">
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

function MatrixTable({
  rowHeader,
  stations,
  rows,
}: {
  rowHeader: string;
  stations: Station[];
  rows: Array<{
    key: string;
    label: string;
    cells: Array<{ stationId: string; content: string; tone: MatrixTone }>;
  }>;
}) {
  if (rows.length === 0) {
    return <EmptyState title="暂无对比数据" description="采集价格、倍率或模型后，这里会显示跨站点矩阵。" />;
  }
  return (
    <div className="overflow-auto">
      <div
        className="grid min-w-[760px] border-b border-border bg-slate-50 text-xs font-semibold text-muted-foreground"
        style={{ gridTemplateColumns: `180px repeat(${Math.max(stations.length, 1)}, minmax(132px, 1fr))` }}
      >
        <div className="px-3 py-2">{rowHeader}</div>
        {stations.map((station) => (
          <div key={station.id} className="truncate px-3 py-2">{station.name}</div>
        ))}
      </div>
      <div className="min-w-[760px] divide-y divide-border">
        {rows.map((row) => (
          <div
            key={row.key}
            className="grid text-sm"
            style={{ gridTemplateColumns: `180px repeat(${Math.max(stations.length, 1)}, minmax(132px, 1fr))` }}
          >
            <div className="truncate px-3 py-2.5 font-semibold text-slate-800">{row.label}</div>
            {stations.map((station) => {
              const cell = row.cells.find((item) => item.stationId === station.id);
              return (
                <div key={`${row.key}-${station.id}`} className={`px-3 py-2.5 ${matrixToneClassName(cell?.tone ?? "muted")}`}>
                  {cell?.content ?? "无"}
                </div>
              );
            })}
          </div>
        ))}
      </div>
    </div>
  );
}

function matrixToneClassName(tone: MatrixTone) {
  if (tone === "good") {
    return "bg-emerald-50 text-emerald-700";
  }
  if (tone === "warning") {
    return "bg-amber-50 text-amber-700";
  }
  if (tone === "muted") {
    return "text-muted-foreground";
  }
  return "text-slate-700";
}

function formatPriceCell(cell: {
  inputPrice: number | null;
  outputPrice: number | null;
  fixedPrice: number | null;
  currency: string;
}) {
  const output = cell.outputPrice ?? cell.inputPrice ?? cell.fixedPrice;
  return output == null ? "暂无价格" : `${cell.currency} ${output.toFixed(4)}`;
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
