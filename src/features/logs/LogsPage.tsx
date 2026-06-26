import { PageScaffold } from "@/components/shell/PageScaffold";
import {
  DataTableLite,
  KeyValueRow,
  SectionCard,
  StatusBadge,
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
  {
    key: "model",
    header: "模型",
    render: (row) => <span className="font-medium text-slate-800">{row.model}</span>,
  },
  { key: "station", header: "实际站点", render: (row) => row.stationName },
  {
    key: "status",
    header: "状态",
    className: "w-24",
    render: (row) => (
      <StatusBadge tone={statusTone[row.status]}>
        {requestStatusLabels[row.status]}
      </StatusBadge>
    ),
  },
  {
    key: "fallback",
    header: "Fallback",
    className: "w-24",
    render: (row) => (row.fallback ? "是" : "否"),
  },
  {
    key: "latency",
    header: "耗时",
    className: "w-20 text-right",
    render: (row) => `${row.latencyMs}ms`,
  },
  {
    key: "tokens",
    header: "Tokens",
    className: "w-28 text-right",
    render: (row) => `${row.inputTokens}/${row.outputTokens}`,
  },
  {
    key: "cost",
    header: "估算成本",
    className: "w-24 text-right",
    render: (row) => `¥${row.estimatedCostCny.toFixed(3)}`,
  },
];

export function LogsPage() {
  const selected = mockRequestLogs[1];

  return (
    <PageScaffold
      eyebrow="Logs"
      title="请求日志"
      description="展示请求列表、fallback 轨迹和脱敏摘要；当前为静态假数据。"
    >
      <div className="grid gap-4 xl:grid-cols-[minmax(0,1.35fr)_420px]">
        <SectionCard
          title="请求列表"
          description="记录模型、站点、状态、耗时、token 和估算成本。"
          contentClassName="p-0"
        >
          <DataTableLite
            columns={logColumns}
            rows={mockRequestLogs}
            getRowKey={(row) => row.id}
            selectedKey={selected.id}
            className="rounded-none border-0"
          />
        </SectionCard>

        <SectionCard title="请求详情" description="静态选中日志详情面板。">
          <dl>
            <KeyValueRow label="请求模型" value={selected.model} />
            <KeyValueRow label="标准模型" value={selected.canonicalModel} />
            <KeyValueRow label="上游模型" value={selected.upstreamModel} />
            <KeyValueRow label="最终站点" value={selected.stationName} />
            <KeyValueRow
              label="错误原因"
              value={selected.errorReason ?? "无"}
            />
            <KeyValueRow
              label="脱敏摘要"
              value={
                <code className="text-xs text-slate-700">
                  {selected.redactedRequestSummary}
                </code>
              }
            />
          </dl>

          <div className="mt-4">
            <div className="mb-2 text-xs font-medium text-muted-foreground">
              候选站点排序
            </div>
            <div className="flex flex-wrap gap-2">
              {selected.candidateStations.map((station, index) => (
                <span
                  key={station}
                  className="rounded-md border border-border bg-slate-50 px-2 py-1 text-xs text-slate-700"
                >
                  {index + 1}. {station}
                </span>
              ))}
            </div>
          </div>

          <div className="mt-4">
            <div className="mb-2 text-xs font-medium text-muted-foreground">
              Fallback trace
            </div>
            <div className="space-y-2">
              {selected.fallbackTrace.map((step) => (
                <FallbackStepRow key={`${step.stationName}-${step.reason}`} step={step} />
              ))}
            </div>
          </div>
        </SectionCard>
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
    <div className="rounded-md border border-border bg-slate-50 px-3 py-2 text-sm">
      <div className="flex items-center justify-between gap-3">
        <span className="font-medium text-slate-800">{step.stationName}</span>
        <StatusBadge tone={tone}>{step.result}</StatusBadge>
      </div>
      <div className="mt-1 text-xs text-muted-foreground">{step.reason}</div>
    </div>
  );
}
