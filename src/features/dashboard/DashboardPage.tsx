import { useEffect, useMemo, useState } from "react";
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
import { getProxyStatus, listRequestLogs } from "@/lib/api/proxy";
import { listKeyPoolItems } from "@/lib/api/stationKeys";
import { mockDashboard, requestStatusLabels, stationStatusLabels } from "@/lib/mock";
import type { ProxyStatus, RequestLog } from "@/lib/types/proxy";
import type { KeyPoolItem } from "@/lib/types/stationKeys";

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
  const [proxyStatus, setProxyStatus] = useState<ProxyStatus | null>(null);
  const [requestLogs, setRequestLogs] = useState<RequestLog[]>([]);
  const [keyPoolItems, setKeyPoolItems] = useState<KeyPoolItem[]>([]);

  useEffect(() => {
    void Promise.all([getProxyStatus(), listRequestLogs(), listKeyPoolItems()])
      .then(([status, logs, keys]) => {
        setProxyStatus(status);
        setRequestLogs(logs);
        setKeyPoolItems(keys);
      })
      .catch(() => undefined);
  }, []);

  const todayRequests = useMemo(() => {
    const today = new Date().toDateString();
    return requestLogs.filter((log) => {
      const numeric = Number(log.startedAt);
      const date = Number.isFinite(numeric) && numeric > 1000000000000
        ? new Date(numeric)
        : new Date(log.startedAt);
      return !Number.isNaN(date.getTime()) && date.toDateString() === today;
    }).length;
  }, [requestLogs]);
  const recentError = requestLogs.find((log) => log.status === "failed")?.errorMessage ?? "最近没有代理错误";
  const proxyRunning = proxyStatus?.running ?? dashboard.proxyRunning;
  const proxyBaseUrl = proxyStatus ? `http://${proxyStatus.bindAddr}:${proxyStatus.port}/v1` : dashboard.baseUrl;
  const enabledKeyCount = keyPoolItems.filter((key) => key.enabled).length;
  const proxyRequestCount = proxyStatus?.requestCount ?? requestLogs.length;

  return (
    <PageScaffold
      title="总览"
      description="本地入口、站点状态、近期请求和价格变化。"
      actions={<Button variant="secondary">复制 CCSwitch 配置</Button>}
    >
      <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4 2xl:grid-cols-[repeat(4,minmax(0,1fr))]">
        <MetricCard icon={Server} label="可用 Key" value={`${enabledKeyCount}`} detail="Key 池启用中" />
        <MetricCard icon={AlertTriangle} label="余额告警" value={`${dashboard.balanceAlertCount}`} detail="低于阈值" tone="warning" />
        <MetricCard icon={Activity} label="今日请求" value={todayRequests.toLocaleString("zh-CN")} detail="真实代理日志" />
        <MetricCard icon={BadgeDollarSign} label="今日成本" value={`¥${dashboard.todayCostCny.toFixed(2)}`} detail="估算" />
        <MetricCard icon={KeyRound} label="今日 Token" value="42.8k" detail="输入/输出合计" />
        <MetricCard icon={Clock3} label="累计请求" value={proxyRequestCount.toLocaleString("zh-CN")} detail="代理运行统计" />
        <MetricCard icon={Radio} label="本地代理" value={proxyRunning ? "运行" : "未启"} detail="127.0.0.1" tone={proxyRunning ? "good" : "warning"} />
        <MetricCard icon={Route} label="路由策略" value="手动" detail="优先级" />
      </div>

      <div className="grid min-h-0 gap-3 xl:grid-cols-[minmax(0,1fr)_380px]">
        <div className="grid gap-3">
          <SectionCard
            title="本地代理入口"
            description="P5 本地 OpenAI-compatible 入口；仅监听 127.0.0.1。"
            action={
              <StatusBadge tone={proxyRunning ? "healthy" : "warning"}>
                {proxyRunning ? "运行中" : "未启动"}
              </StatusBadge>
            }
          >
            <div className="grid gap-3 lg:grid-cols-[minmax(0,1fr)_240px]">
              <div className="rounded-[var(--surface-radius)] border border-border bg-white p-4 shadow-[var(--surface-shadow)]">
                <div className="text-xs text-muted-foreground">Base URL</div>
                <div className="mt-1 flex min-w-0 items-center gap-2">
                  <code className="min-w-0 flex-1 truncate text-[15px] font-semibold text-slate-800">
                    {proxyBaseUrl}
                  </code>
                  <Button variant="outline">复制</Button>
                </div>
                <div className="mt-2 text-xs text-muted-foreground">
                  监听地址 {proxyStatus?.bindAddr ?? "127.0.0.1"} · 运行次数 {proxyRequestCount.toLocaleString("zh-CN")}
                </div>
              </div>
              <div className="rounded-[var(--surface-radius)] border border-border bg-white p-4 shadow-[var(--surface-shadow)]">
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
              <div key={key} className="rounded-[var(--surface-radius)] border border-border bg-white p-3 shadow-[var(--surface-shadow)]">
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
          <div className="mt-4 rounded-[var(--surface-radius)] border border-border bg-white p-3 text-xs leading-5 text-slate-700 shadow-[var(--surface-shadow)]">
            {recentError}
          </div>
        </SectionCard>
      </div>
    </PageScaffold>
  );
}
