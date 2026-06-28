import { BadgeDollarSign, Layers3, TrendingDown } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import {
  DataTableLite,
  InspectorPanel,
  MetricCard,
  SectionCard,
  StatusBadge,
  type DataTableColumn,
} from "@/components/ui";
import {
  mockPricingRows,
  pricingStatusLabels,
  type MockPricingRow,
  type MockStationPrice,
} from "@/lib/mock";

const statusTone = {
  fresh: "healthy",
  stale: "warning",
  unavailable: "disabled",
} as const;

const pricingColumns: DataTableColumn<MockPricingRow>[] = [
  {
    key: "model",
    header: "模型",
    render: (row) => <span className="font-semibold text-slate-800">{row.model}</span>,
  },
  { key: "station", header: "推荐站点", render: (row) => row.recommendedStationName },
  {
    key: "input",
    header: "输入 / 1M",
    className: "text-right",
    render: (row) => <PriceValue value={row.inputCnyPer1M} />,
  },
  {
    key: "output",
    header: "输出 / 1M",
    className: "text-right",
    render: (row) => <PriceValue value={row.outputCnyPer1M} />,
  },
  { key: "count", header: "站点", className: "w-20 text-right", render: (row) => `${row.stationCount}` },
  {
    key: "delta",
    header: "变化",
    className: "w-20 text-right",
    render: (row) => (
      <span className={row.deltaPercent > 0 ? "text-amber-700" : "text-emerald-700"}>
        {row.deltaPercent > 0 ? "+" : ""}
        {row.deltaPercent.toFixed(1)}%
      </span>
    ),
  },
  {
    key: "status",
    header: "状态",
    className: "w-24",
    render: (row) => (
      <StatusBadge tone={statusTone[row.status]}>
        {pricingStatusLabels[row.status]}
      </StatusBadge>
    ),
  },
];

const stationColumns: DataTableColumn<MockStationPrice>[] = [
  { key: "station", header: "站点", render: (row) => row.stationName },
  { key: "input", header: "输入", className: "text-right", render: (row) => <PriceValue value={row.inputCnyPer1M} /> },
  { key: "output", header: "输出", className: "text-right", render: (row) => <PriceValue value={row.outputCnyPer1M} /> },
  { key: "ratio", header: "倍率", render: (row) => `${row.modelRatio} / ${row.groupRatio}` },
  {
    key: "health",
    header: "健康",
    render: (row) => (
      <StatusBadge tone={row.health === "正常" ? "healthy" : row.health === "警告" ? "warning" : "error"}>
        {row.health}
      </StatusBadge>
    ),
  },
];

export function PricingPage() {
  const selected = mockPricingRows[0];
  const cheapest = mockPricingRows.reduce((min, row) =>
    row.outputCnyPer1M < min.outputCnyPer1M ? row : min,
  );

  return (
    <PageScaffold title="价格表" description="模型价格归一化和推荐站点对比；当前为 mock 快照。">
      <div className="grid gap-[var(--shell-page-gap)] md:grid-cols-3">
        <MetricCard icon={BadgeDollarSign} label="最低输出价" value={`¥${cheapest.outputCnyPer1M.toFixed(2)}`} detail={cheapest.model} />
        <MetricCard icon={Layers3} label="覆盖模型" value={`${mockPricingRows.length}`} detail="mock rows" />
        <MetricCard icon={TrendingDown} label="价格变化" value="-6.4%" detail="gpt-4.1" />
      </div>

      <div className="grid gap-[var(--shell-page-gap)] xl:grid-cols-[minmax(0,1fr)_390px]">
        <SectionCard title="模型价格" description="主表保持紧凑，后续可接 pricing snapshots。" contentClassName="p-0">
          <DataTableLite
            columns={pricingColumns}
            rows={mockPricingRows}
            getRowKey={(row) => row.model}
            selectedKey={selected.model}
            className="rounded-none border-0 shadow-none"
          />
        </SectionCard>

        <InspectorPanel title={`${selected.model} inspector`} description="站点价格、推荐原因和原始倍率。">
          <div className="space-y-3 p-4">
            <div className="rounded-[var(--surface-radius)] border border-cyan-100 bg-cyan-50/60 p-3 text-sm text-slate-700">
              推荐：{selected.recommendReasons.join(" / ")}
            </div>
            <DataTableLite
              columns={stationColumns}
              rows={selected.stationPrices}
              getRowKey={(row) => row.stationName}
              className="shadow-none"
            />
            <div className="rounded-[var(--surface-radius)] border border-cyan-100 bg-white/80 p-3 text-xs text-muted-foreground">
              原始倍率默认弱化显示；后续接入真实采集后再提供展开明细。
            </div>
          </div>
        </InspectorPanel>
      </div>
    </PageScaffold>
  );
}

function PriceValue({ value }: { value: number }) {
  return <span className="font-semibold tabular-nums text-slate-800">¥{value.toFixed(2)}</span>;
}
