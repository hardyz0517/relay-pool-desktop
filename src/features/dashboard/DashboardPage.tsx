import {
  Activity,
  AlertTriangle,
  BadgeDollarSign,
  Clock3,
  KeyRound,
  Radio,
  Route,
  Server,
} from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import {
  ActivityItem,
  ActivityList,
  Button,
  MaskedSecret,
  MetricCard,
  SectionCard,
  StatusBadge,
} from "@/components/ui";
import { mockDashboard, requestStatusLabels, stationStatusLabels } from "@/lib/mock";

const healthTone = {
  healthy: "healthy",
  warning: "warning",
  error: "error",
  disabled: "disabled",
} as const;

const requestTone = {
  success: "healthy",
  fallback: "warning",
  failed: "error",
} as const;

export function DashboardPage() {
  const dashboard = mockDashboard;

  return (
    <PageScaffold
      title="总览"
      description="本地入口、站点状态、近期请求和价格变化。"
      actions={<Button variant="secondary">复制 CCSwitch 配置</Button>}
    >
      <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4 2xl:grid-cols-[repeat(4,minmax(0,1fr))]">
        <MetricCard icon={Server} label="可用站点" value={`${dashboard.enabledStationCount}`} detail="启用中" />
        <MetricCard icon={AlertTriangle} label="余额告警" value={`${dashboard.balanceAlertCount}`} detail="低于阈值" tone="warning" />
        <MetricCard icon={Activity} label="今日请求" value={dashboard.todayRequests.toLocaleString("zh-CN")} detail="含 fallback" />
        <MetricCard icon={BadgeDollarSign} label="今日成本" value={`¥${dashboard.todayCostCny.toFixed(2)}`} detail="估算" />
        <MetricCard icon={KeyRound} label="今日 Token" value="42.8k" detail="输入/输出合计" />
        <MetricCard icon={Clock3} label="平均延迟" value="1.8s" detail="最近请求" />
        <MetricCard icon={Radio} label="本地代理" value={dashboard.proxyRunning ? "运行" : "未启"} detail="127.0.0.1" tone={dashboard.proxyRunning ? "good" : "warning"} />
        <MetricCard icon={Route} label="路由策略" value="手动" detail="优先级" />
      </div>

      <div className="grid min-h-0 gap-3 xl:grid-cols-[minmax(0,1fr)_380px]">
        <div className="grid gap-3">
          <SectionCard
            title="本地代理入口"
            description="P2.5 仅展示入口状态；真实代理在后续阶段接入。"
            action={
              <StatusBadge tone={dashboard.proxyRunning ? "healthy" : "warning"}>
                {dashboard.proxyRunning ? "运行中" : "未启动"}
              </StatusBadge>
            }
          >
            <div className="grid gap-3 lg:grid-cols-[minmax(0,1fr)_240px]">
              <div className="rounded-2xl border border-cyan-100 bg-cyan-50/60 p-4">
                <div className="text-xs text-muted-foreground">Base URL</div>
                <div className="mt-1 flex min-w-0 items-center gap-2">
                  <code className="min-w-0 flex-1 truncate text-[15px] font-semibold text-slate-800">
                    {dashboard.baseUrl}
                  </code>
                  <Button variant="outline">复制</Button>
                </div>
              </div>
              <div className="rounded-2xl border border-cyan-100 bg-white/80 p-4">
                <div className="text-xs text-muted-foreground">Local Key</div>
                <div className="mt-1 flex items-center justify-between gap-2">
                  <MaskedSecret value={dashboard.maskedLocalKey} />
                  <Button variant="outline">复制</Button>
                </div>
              </div>
            </div>
          </SectionCard>

          <SectionCard title="最近活动" description="请求和价格变化合并为桌面工具活动流。">
            <div className="grid gap-4 xl:grid-cols-2">
              <ActivityList>
                {dashboard.recentRequests.slice(0, 5).map((request) => (
                  <ActivityItem
                    key={request.id}
                    detail={`${request.stationName} · ${request.latencyMs}ms · ¥${request.estimatedCostCny.toFixed(3)}`}
                    marker={<StatusBadge tone={requestTone[request.status]}>{requestStatusLabels[request.status]}</StatusBadge>}
                    meta={request.createdAt}
                    title={request.model}
                  />
                ))}
              </ActivityList>
              <ActivityList>
                {dashboard.priceChanges.slice(0, 5).map((change) => (
                  <ActivityItem
                    key={`${change.model}-${change.stationName}`}
                    detail={`${change.stationName} · ${change.updatedAt}`}
                    meta={
                      <span className={change.deltaPercent > 0 ? "text-amber-700" : "text-emerald-700"}>
                        {change.deltaPercent > 0 ? "+" : ""}
                        {change.deltaPercent.toFixed(1)}%
                      </span>
                    }
                    title={change.model}
                  />
                ))}
              </ActivityList>
            </div>
          </SectionCard>
        </div>

        <SectionCard title="站点健康" description="状态聚合和待处理事项。">
          <div className="grid grid-cols-2 gap-3">
            {(Object.keys(dashboard.healthSummary) as Array<keyof typeof dashboard.healthSummary>).map((key) => (
              <div key={key} className="rounded-2xl border border-cyan-100 bg-cyan-50/55 p-3">
                <div className="flex items-center justify-between gap-2">
                  <StatusBadge tone={healthTone[key]}>{stationStatusLabels[key]}</StatusBadge>
                  <span className="text-xl font-semibold text-slate-800">
                    {dashboard.healthSummary[key]}
                  </span>
                </div>
                <div className="mt-1 text-xs text-muted-foreground">站点</div>
              </div>
            ))}
          </div>
          <div className="mt-4 rounded-2xl border border-amber-200 bg-amber-50/80 p-3 text-xs leading-5 text-amber-800">
            Lantern NewAPI 余额接近阈值；Harbor Compatible 最近一次健康检测被限流。
          </div>
        </SectionCard>
      </div>
    </PageScaffold>
  );
}
