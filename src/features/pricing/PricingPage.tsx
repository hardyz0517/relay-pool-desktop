import { PageScaffold } from "@/components/shell/PageScaffold";
import {
  DataTableLite,
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
    render: (row) => <span className="font-medium text-slate-800">{row.model}</span>,
  },
  {
    key: "station",
    header: "推荐站点",
    render: (row) => row.recommendedStationName,
  },
  {
    key: "input",
    header: "输入价格",
    className: "text-right",
    render: (row) => <PriceValue value={row.inputCnyPer1M} />,
  },
  {
    key: "output",
    header: "输出价格",
    className: "text-right",
    render: (row) => <PriceValue value={row.outputCnyPer1M} />,
  },
  {
    key: "count",
    header: "可用站点",
    className: "w-24 text-right",
    render: (row) => `${row.stationCount} 个`,
  },
  {
    key: "delta",
    header: "变化",
    className: "w-20 text-right",
    render: (row) => (
      <span
        className={
          row.deltaPercent > 0
            ? "text-amber-700"
            : row.deltaPercent < 0
              ? "text-emerald-700"
              : "text-slate-500"
        }
      >
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
  {
    key: "input",
    header: "输入",
    className: "text-right",
    render: (row) => <PriceValue value={row.inputCnyPer1M} />,
  },
  {
    key: "output",
    header: "输出",
    className: "text-right",
    render: (row) => <PriceValue value={row.outputCnyPer1M} />,
  },
  { key: "modelRatio", header: "模型倍率", render: (row) => row.modelRatio },
  { key: "groupRatio", header: "分组倍率", render: (row) => row.groupRatio },
  {
    key: "health",
    header: "健康",
    render: (row) => (
      <StatusBadge
        tone={
          row.health === "正常"
            ? "healthy"
            : row.health === "警告"
              ? "warning"
              : "error"
        }
      >
        {row.health}
      </StatusBadge>
    ),
  },
];

export function PricingPage() {
  const selected = mockPricingRows[0];

  return (
    <PageScaffold
      eyebrow="Pricing"
      title="价格表"
      description="用假数据展示模型价格归一化、推荐站点和各站价格对比。"
    >
      <div className="grid gap-4 xl:grid-cols-[minmax(0,1.35fr)_420px]">
        <SectionCard
          title="模型价格"
          description="普通轻量表格；复杂排序和虚拟滚动后续再考虑 TanStack Table。"
          contentClassName="p-0"
        >
          <DataTableLite
            columns={pricingColumns}
            rows={mockPricingRows}
            getRowKey={(row) => row.model}
            selectedKey={selected.model}
            className="rounded-none border-0"
          />
        </SectionCard>

        <SectionCard
          title={`${selected.model} 详情`}
          description="同一模型在不同站点的价格对比。"
        >
          <div className="mb-3 flex flex-wrap gap-2">
            {selected.recommendReasons.map((reason) => (
              <StatusBadge key={reason} tone="info">
                {reason}
              </StatusBadge>
            ))}
          </div>
          <DataTableLite
            columns={stationColumns}
            rows={selected.stationPrices}
            getRowKey={(row) => row.stationName}
          />
          <div className="mt-3 rounded-md border border-border bg-slate-50 px-3 py-2">
            <div className="mb-2 text-xs font-medium text-muted-foreground">
              原始倍率折叠展示
            </div>
            <div className="grid gap-2 text-xs text-slate-700">
              {selected.stationPrices.map((price) => (
                <div
                  key={price.stationName}
                  className="flex items-center justify-between rounded bg-white px-2 py-1 ring-1 ring-border"
                >
                  <span>{price.stationName}</span>
                  <span>
                    model {price.modelRatio} / group {price.groupRatio}
                  </span>
                </div>
              ))}
            </div>
          </div>
        </SectionCard>
      </div>
    </PageScaffold>
  );
}

function PriceValue({ value }: { value: number }) {
  return (
    <span className="font-medium tabular-nums text-slate-800">
      ¥{value.toFixed(2)}
      <span className="ml-1 text-xs font-normal text-muted-foreground">/1M</span>
    </span>
  );
}
