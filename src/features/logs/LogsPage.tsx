import { PageScaffold } from "@/components/shell/PageScaffold";
import {
  DataTableLite,
  InspectorPanel,
  PropertyList,
  PropertyRow,
  SegmentedControl,
  StatusBadge,
  Toolbar,
  type DataTableColumn,
} from "@/components/ui";
import {
  mockRequestLogs,
  requestStatusLabels,
  type MockFallbackStep,
  type MockRequestLog,
} from "@/lib/mock";

const statusTone = {
  success: "healthy",
  failed: "error",
  fallback: "warning",
} as const;

const logColumns: DataTableColumn<MockRequestLog>[] = [
  { key: "time", header: "时间", className: "w-20", render: (row) => row.createdAt },
  { key: "model", header: "模型", render: (row) => <span className="font-semibold text-slate-800">{row.model}</span> },
  { key: "station", header: "实际站点", render: (row) => row.stationName },
  {
    key: "status",
    header: "状态",
    className: "w-24",
    render: (row) => <StatusBadge tone={statusTone[row.status]}>{requestStatusLabels[row.status]}</StatusBadge>,
  },
  { key: "fallback", header: "Fallback", className: "w-24", render: (row) => (row.fallback ? "是" : "否") },
  { key: "latency", header: "耗时", className: "w-20 text-right", render: (row) => `${row.latencyMs}ms` },
  { key: "tokens", header: "Tokens", className: "w-28 text-right", render: (row) => `${row.inputTokens}/${row.outputTokens}` },
  { key: "cost", header: "估算", className: "w-24 text-right", render: (row) => `¥${row.estimatedCostCny.toFixed(3)}` },
];

export function LogsPage() {
  const selected = mockRequestLogs[1];

  return (
    <PageScaffold title="请求日志" description="请求列表和 fallback 轨迹；当前为静态 mock。">
      <div className="grid gap-3 xl:grid-cols-[minmax(0,1fr)_390px]">
        <div className="min-w-0 overflow-hidden rounded-2xl border border-white/70 bg-white/90 shadow-[0_12px_30px_rgba(33,79,88,0.07)]">
          <Toolbar>
            <SegmentedControl
              value="all"
              options={[
                { value: "all", label: "全部" },
                { value: "failed", label: "失败" },
                { value: "fallback", label: "Fallback" },
              ]}
            />
            <div className="hidden gap-2 text-xs text-muted-foreground md:flex">
              <span>模型：全部</span>
              <span>站点：全部</span>
            </div>
          </Toolbar>
          <DataTableLite
            columns={logColumns}
            rows={mockRequestLogs}
            getRowKey={(row) => row.id}
            selectedKey={selected.id}
            className="rounded-none border-0 shadow-none"
          />
        </div>

        <InspectorPanel title="Log inspector" description={`${selected.model} · ${selected.createdAt}`}>
          <div className="space-y-4 p-4">
            <PropertyList className="overflow-hidden rounded-2xl border border-cyan-100 bg-white/75">
              <PropertyRow label="请求模型" value={selected.model} />
              <PropertyRow label="标准模型" value={selected.canonicalModel} />
              <PropertyRow label="上游模型" value={selected.upstreamModel} />
              <PropertyRow label="最终站点" value={selected.stationName} />
              <PropertyRow label="错误原因" value={selected.errorReason ?? "无"} />
            </PropertyList>

            <div>
              <div className="mb-2 text-xs font-semibold text-slate-700">Fallback trace</div>
              <div className="space-y-2">
                {selected.fallbackTrace.map((step) => (
                  <FallbackStepRow key={`${step.stationName}-${step.reason}`} step={step} />
                ))}
              </div>
            </div>

            <div className="rounded-2xl border border-cyan-100 bg-cyan-50/60 p-3">
              <div className="text-xs font-semibold text-slate-700">脱敏请求摘要</div>
              <code className="mt-2 block truncate text-xs text-slate-600">
                {selected.redactedRequestSummary}
              </code>
            </div>
          </div>
        </InspectorPanel>
      </div>
    </PageScaffold>
  );
}

function FallbackStepRow({ step }: { step: MockFallbackStep }) {
  const tone =
    step.result === "selected"
      ? "healthy"
      : step.result === "failed"
        ? "error"
        : "disabled";

  return (
    <div className="rounded-2xl border border-cyan-100 bg-cyan-50/50 px-3 py-2 text-xs">
      <div className="flex items-center justify-between gap-3">
        <span className="font-semibold text-slate-800">{step.stationName}</span>
        <StatusBadge tone={tone}>{step.result}</StatusBadge>
      </div>
      <div className="mt-1 text-muted-foreground">{step.reason}</div>
    </div>
  );
}
