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
import { listBalanceSnapshots } from "@/lib/api/economics";
import { stationStatusLabels } from "@/lib/types/stations";
import type { ProxyStatus, RequestLog } from "@/lib/types/proxy";
import type { BalanceSnapshot } from "@/lib/types/economics";
import type { KeyPoolItem } from "@/lib/types/stationKeys";

const healthTone = {
  healthy: "healthy",
  warning: "warning",
  error: "error",
  disabled: "disabled",
  unchecked: "info",
} as const;

const requestTone = {
  success: "healthy",
  fallback: "warning",
  failed: "error",
} as const;

export function DashboardPage() {
  const [proxyStatus, setProxyStatus] = useState<ProxyStatus | null>(null);
  const [requestLogs, setRequestLogs] = useState<RequestLog[]>([]);
  const [keyPoolItems, setKeyPoolItems] = useState<KeyPoolItem[]>([]);
  const [balanceSnapshots, setBalanceSnapshots] = useState<BalanceSnapshot[]>([]);

  useEffect(() => {
    void Promise.all([getProxyStatus(), listRequestLogs(), listKeyPoolItems(), listBalanceSnapshots()])
      .then(([status, logs, keys, balances]) => {
        setProxyStatus(status);
        setRequestLogs(logs);
        setKeyPoolItems(keys);
        setBalanceSnapshots(balances);
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
  const proxyRunning = proxyStatus?.running ?? false;
  const proxyBaseUrl = proxyStatus ? `http://${proxyStatus.bindAddr}:${proxyStatus.port}/v1` : "http://127.0.0.1:8787/v1";
  const enabledKeyCount = keyPoolItems.filter((key) => key.enabled).length;
  const proxyRequestCount = proxyStatus?.requestCount ?? requestLogs.length;
  const todayCost = requestLogs.reduce((sum, log) => sum + (log.estimatedTotalCost ?? 0), 0);
  const todayTokens = requestLogs.reduce((sum, log) => sum + (log.totalTokens ?? 0), 0);
  const lowBalanceStations = new Set(balanceSnapshots.filter((snapshot) => snapshot.status === "low" || snapshot.status === "depleted").map((snapshot) => snapshot.stationId)).size;
  const totalBalance = balanceSnapshots.reduce((sum, snapshot) => sum + (snapshot.value ?? 0), 0);

  return (
    <PageScaffold
      title="总览"
      description="本地代理状态、站点余额、近期请求和成本摘要。"
      actions={<Button variant="secondary">复制 CCSwitch 配置</Button>}
    >
      <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4 2xl:grid-cols-[repeat(4,minmax(0,1fr))]">
        <MetricCard icon={Server} label="可用 Key" value={`${enabledKeyCount}`} detail="Key 池启用中" />
        <MetricCard icon={AlertTriangle} label="余额告警" value={`${lowBalanceStations}`} detail="低余额站点数" tone="warning" />
        <MetricCard icon={Activity} label="今日请求" value={todayRequests.toLocaleString("zh-CN")} detail="真实代理日志" />
        <MetricCard icon={BadgeDollarSign} label="今日成本" value={formatCost(todayCost)} detail="基于 request_logs" />
        <MetricCard icon={KeyRound} label="今日 Token" value={formatTokens(todayTokens)} detail="input / output 合计" />
        <MetricCard icon={Clock3} label="累计请求" value={proxyRequestCount.toLocaleString("zh-CN")} detail="代理运行统计" />
        <MetricCard icon={Radio} label="本地代理" value={proxyRunning ? "运行" : "未启"} detail="127.0.0.1" tone={proxyRunning ? "good" : "warning"} />
        <MetricCard icon={Route} label="路由策略" value="priority_fallback" detail="P7 之后接 cheap_first" />
      </div>

      <div className="grid min-h-0 gap-3 xl:grid-cols-[minmax(0,1fr)_380px]">
        <div className="grid gap-3">
          <SectionCard
            title="本地代理入口"
            description="只监听 127.0.0.1。"
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
                  <MaskedSecret value={proxyRunning ? "sk-local-pool-****" : "未启动"} />
                  <Button variant="outline">复制</Button>
                </div>
              </div>
            </div>
          </SectionCard>

          <SectionCard title="最近活动" description="最近请求和价格变化摘要。">
            <div className="grid gap-4 xl:grid-cols-2">
              <ActivityList>
                {requestLogs.slice(0, 5).map((request) => (
                  <ActivityItem
                    key={request.id}
                    detail={`${request.path} · ${request.stationId ?? "未选择"} · ${request.durationMs ?? 0}ms · ${formatCost(request.estimatedTotalCost ?? 0)}`}
                    marker={<StatusBadge tone={requestTone[request.status as keyof typeof requestTone] ?? "healthy"}>{request.status}</StatusBadge>}
                    meta={formatTime(request.startedAt)}
                    title={request.model ?? request.path}
                  />
                ))}
              </ActivityList>
              <ActivityList>
                {balanceSnapshots.slice(0, 5).map((snapshot) => (
                  <ActivityItem
                    key={snapshot.id}
                    detail={`${snapshot.currency} ${snapshot.value ?? 0} · ${snapshot.status}`}
                    marker={<StatusBadge tone={healthTone[snapshot.status === "depleted" ? "error" : snapshot.status === "low" ? "warning" : "healthy"]}>{snapshot.status}</StatusBadge>}
                    meta={snapshot.stationId}
                    title={snapshot.scope}
                  />
                ))}
              </ActivityList>
            </div>
          </SectionCard>
        </div>

        <SectionCard title="站点健康" description="余额与请求状态聚合。">
          <div className="grid grid-cols-2 gap-3">
            {(Object.keys(stationStatusLabels) as Array<keyof typeof stationStatusLabels>).map((key) => (
              <div key={key} className="rounded-[var(--surface-radius)] border border-border bg-white p-3 shadow-[var(--surface-shadow)]">
                <div className="flex items-center justify-between gap-2">
                  <StatusBadge tone={healthTone[key]}>{stationStatusLabels[key]}</StatusBadge>
                  <span className="text-xl font-semibold text-slate-800">
                    {keyPoolItems.filter((item) => item.status === key).length}
                  </span>
                </div>
                <div className="mt-1 text-xs text-muted-foreground">Key</div>
              </div>
            ))}
          </div>
          <div className="mt-4 rounded-[var(--surface-radius)] border border-border bg-white p-3 text-xs leading-5 text-slate-700 shadow-[var(--surface-shadow)]">
            已知余额总计 {totalBalance.toFixed(2)} · 最近错误：{recentError}
          </div>
        </SectionCard>
      </div>
    </PageScaffold>
  );
}

function formatTime(value: string) {
  const numeric = Number(value);
  const date = Number.isFinite(numeric) && numeric > 1000000000000 ? new Date(numeric) : new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return date.toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit", second: "2-digit" });
}

function formatCost(value: number) {
  return `¥${value.toFixed(4)}`;
}

function formatTokens(value: number) {
  return `${value.toLocaleString("zh-CN")} t`;
}
