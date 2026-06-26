import { PageScaffold } from "@/components/shell/PageScaffold";
import {
  Button,
  DataTableLite,
  KeyValueRow,
  MaskedSecret,
  MetricCard,
  SectionCard,
  StatusBadge,
  type DataTableColumn,
} from "@/components/ui";
import {
  mockDashboard,
  requestStatusLabels,
  stationStatusLabels,
  type MockRequestLog,
} from "@/lib/mock";

type PriceChangeRow = (typeof mockDashboard.priceChanges)[number];
type HealthSummaryKey = keyof typeof mockDashboard.healthSummary;

const requestStatusTone: Record<MockRequestLog["status"], "healthy" | "warning" | "error"> = {
  success: "healthy",
  fallback: "warning",
  failed: "error",
};

const healthTone: Record<HealthSummaryKey, "healthy" | "warning" | "error" | "disabled"> = {
  healthy: "healthy",
  warning: "warning",
  error: "error",
  disabled: "disabled",
};

const recentRequestColumns: DataTableColumn<MockRequestLog>[] = [
  {
    key: "createdAt",
    header: "时间",
    className: "w-[86px]",
    render: (row) => row.createdAt,
  },
  {
    key: "model",
    header: "模型",
    render: (row) => <span className="font-medium text-slate-800">{row.model}</span>,
  },
  {
    key: "stationName",
    header: "站点",
    render: (row) => row.stationName,
  },
  {
    key: "status",
    header: "状态",
    className: "w-[88px]",
    render: (row) => (
      <StatusBadge tone={requestStatusTone[row.status]}>
        {requestStatusLabels[row.status]}
      </StatusBadge>
    ),
  },
  {
    key: "latency",
    header: "耗时",
    className: "w-[72px] text-right",
    render: (row) => `${row.latencyMs}ms`,
  },
  {
    key: "cost",
    header: "估算",
    className: "w-[78px] text-right",
    render: (row) => `¥${row.estimatedCostCny.toFixed(2)}`,
  },
];

const priceChangeColumns: DataTableColumn<PriceChangeRow>[] = [
  {
    key: "model",
    header: "模型",
    render: (row) => <span className="font-medium text-slate-800">{row.model}</span>,
  },
  {
    key: "station",
    header: "推荐站点",
    render: (row) => row.stationName,
  },
  {
    key: "delta",
    header: "变化",
    className: "w-[80px] text-right",
    render: (row) => (
      <span
        className={
          row.deltaPercent > 0
            ? "text-rose-700"
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
    key: "updatedAt",
    header: "更新时间",
    className: "w-[92px]",
    render: (row) => row.updatedAt,
  },
];

export function DashboardPage() {
  const dashboard = mockDashboard;
  const proxyStatusTone = dashboard.proxyRunning ? "healthy" : "warning";

  return (
    <PageScaffold
      eyebrow="Overview"
      title="总览"
      description="本地代理入口、站点健康、余额告警、今日请求和价格变化的假数据总览。"
    >
      <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-4">
        <MetricCard
          label="可用站点"
          value={`${dashboard.enabledStationCount} 个`}
          detail="启用且参与路由"
          tone="good"
        />
        <MetricCard
          label="余额告警"
          value={`${dashboard.balanceAlertCount} 个`}
          detail="低于当前阈值"
          tone={dashboard.balanceAlertCount > 0 ? "warning" : "good"}
        />
        <MetricCard
          label="今日请求"
          value={dashboard.todayRequests.toLocaleString("zh-CN")}
          detail="含 fallback 请求"
        />
        <MetricCard
          label="今日估算花费"
          value={`¥${dashboard.todayCostCny.toFixed(2)}`}
          detail="按最新倍率换算"
        />
      </div>

      <div className="grid gap-4 lg:grid-cols-[0.95fr_1.05fr]">
        <SectionCard
          title="本地代理"
          description="Phase 1 仅展示本地入口配置，不启动真实代理。"
          action={
            <StatusBadge tone={proxyStatusTone}>
              {dashboard.proxyRunning ? "运行中" : "未启动"}
            </StatusBadge>
          }
        >
          <dl className="divide-y divide-border">
            <KeyValueRow
              label="Base URL"
              value={
                <div className="flex items-center justify-between gap-2">
                  <code className="min-w-0 truncate rounded border border-border bg-slate-50 px-1.5 py-0.5 text-xs text-slate-700">
                    {dashboard.baseUrl}
                  </code>
                  <Button variant="outline" className="h-7 px-2 text-xs">
                    复制
                  </Button>
                </div>
              }
            />
            <KeyValueRow
              label="Local Key"
              value={
                <div className="flex items-center justify-between gap-2">
                  <MaskedSecret value={dashboard.maskedLocalKey} />
                  <Button variant="outline" className="h-7 px-2 text-xs">
                    复制
                  </Button>
                </div>
              }
            />
            <KeyValueRow
              label="路由策略"
              value={
                <div className="flex items-center gap-2">
                  <StatusBadge tone="info">{dashboard.strategy}</StatusBadge>
                  <span className="text-xs text-muted-foreground">
                    按站点优先级选择，失败后切换
                  </span>
                </div>
              }
            />
          </dl>
        </SectionCard>

        <SectionCard title="站点健康概览" description="按最近一次健康检测结果聚合。">
          <div className="grid gap-2 sm:grid-cols-4">
            {(Object.keys(dashboard.healthSummary) as HealthSummaryKey[]).map((key) => (
              <div
                key={key}
                className="rounded-md border border-border bg-slate-50 px-3 py-2"
              >
                <div className="flex items-center justify-between gap-2">
                  <StatusBadge tone={healthTone[key]}>
                    {stationStatusLabels[key]}
                  </StatusBadge>
                  <span className="text-lg font-semibold text-slate-800">
                    {dashboard.healthSummary[key]}
                  </span>
                </div>
                <div className="mt-1 text-xs text-muted-foreground">站点数量</div>
              </div>
            ))}
          </div>
          <div className="mt-3 rounded-md border border-amber-200 bg-amber-50 px-3 py-2 text-xs text-amber-800">
            Lantern NewAPI 余额接近阈值；Harbor Compatible 最近一次健康检测被限流。
          </div>
        </SectionCard>
      </div>

      <div className="grid gap-4 xl:grid-cols-[1.3fr_0.9fr]">
        <SectionCard
          title="最近请求"
          description="展示最近请求的模型、站点、状态、耗时和估算成本。"
          action={
            <Button variant="ghost" className="h-7 px-2 text-xs">
              查看日志
            </Button>
          }
          contentClassName="p-0"
        >
          <DataTableLite
            columns={recentRequestColumns}
            rows={dashboard.recentRequests.slice(0, 5)}
            getRowKey={(row) => row.id}
            className="rounded-none border-0"
          />
        </SectionCard>

        <SectionCard
          title="最近价格变化"
          description="按最新价格快照对推荐站点做变化提示。"
          contentClassName="p-0"
        >
          <DataTableLite
            columns={priceChangeColumns}
            rows={dashboard.priceChanges.slice(0, 5)}
            getRowKey={(row) => `${row.model}-${row.stationName}`}
            className="rounded-none border-0"
          />
        </SectionCard>
      </div>
    </PageScaffold>
  );
}
