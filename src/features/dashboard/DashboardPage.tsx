import { useEffect, useMemo, useState } from "react";
import {
  Activity,
  AlertTriangle,
  BadgeDollarSign,
  Copy,
  KeyRound,
  Plus,
  Radio,
  Route,
  Server,
} from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import {
  Button,
  MaskedSecret,
  MetricPanel,
  ObjectRow,
  SectionCard,
  StatusBadge,
} from "@/components/ui";
import { getProxyStatus, listRequestLogs } from "@/lib/api/proxy";
import { listKeyPoolItems } from "@/lib/api/stationKeys";
import { mockDashboard } from "@/lib/mock";
import type { AppPageId } from "@/lib/types/navigation";
import type { ProxyStatus, RequestLog } from "@/lib/types/proxy";
import type { KeyPoolItem } from "@/lib/types/stationKeys";

type DashboardPageProps = {
  onNavigate: (pageId: AppPageId) => void;
};

const requestTone = {
  success: "healthy",
  fallback: "warning",
  failed: "error",
} as const;

function copyText(value: string) {
  void navigator.clipboard.writeText(value);
}

export function DashboardPage({ onNavigate }: DashboardPageProps) {
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

  const todayFailedRequests = requestLogs.filter((log) => log.status === "failed").length;
  const failureRate = todayRequests > 0 ? todayFailedRequests / todayRequests : 0;
  const failureRateText = `${(failureRate * 100).toFixed(1)}%`;
  const failureRateTone = failureRate > 0.1 ? "danger" : failureRate > 0.03 ? "warning" : "good";

  const recentError = requestLogs.find((log) => log.status === "failed")?.errorMessage ?? "最近没有代理错误。";
  const proxyRunning = proxyStatus?.running ?? dashboard.proxyRunning;
  const proxyBaseUrl = proxyStatus ? `http://${proxyStatus.bindAddr}:${proxyStatus.port}/v1` : dashboard.baseUrl;
  const enabledKeyCount = keyPoolItems.filter((key) => key.enabled).length;
  const proxyRequestCount = proxyStatus?.requestCount ?? requestLogs.length;

  return (
    <PageScaffold
      title="代理工作台"
      description="本地 OpenAI-compatible 入口、站点状态、近期请求和价格变化都聚在一个面板里。"
      actions={
        <>
          <Button variant="secondary" onClick={() => copyText(proxyBaseUrl)}>
            <Copy className="h-4 w-4" />
            复制本地入口
          </Button>
          <Button onClick={() => onNavigate("addProvider")}>
            <Plus className="h-4 w-4" />
            添加 Provider
          </Button>
        </>
      }
    >
      <div className="grid gap-4 xl:grid-cols-[minmax(0,1.35fr)_minmax(360px,0.65fr)]">
        <SectionCard
          title="当前路由"
          description="外部工具会优先使用这条本地 OpenAI-compatible 入口。"
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
                <Button variant="outline" onClick={() => copyText(proxyBaseUrl)}>
                  <Copy className="h-4 w-4" />
                </Button>
              </div>
              <div className="mt-2 text-xs text-muted-foreground">
                监听地址 {proxyStatus?.bindAddr ?? "127.0.0.1"} - 运行次数 {proxyRequestCount.toLocaleString("zh-CN")}
              </div>
            </div>
            <div className="rounded-[var(--surface-radius)] border border-border bg-white p-4 shadow-[var(--surface-shadow)]">
              <div className="text-xs text-muted-foreground">Local Key</div>
              <div className="mt-1 flex items-center justify-between gap-2">
                <MaskedSecret value={dashboard.maskedLocalKey} />
                <Button variant="outline" onClick={() => copyText(dashboard.maskedLocalKey)}>
                  <Copy className="h-4 w-4" />
                </Button>
              </div>
            </div>
          </div>
        </SectionCard>

        <MetricPanel
          title="今日指标"
          description="只保留判断运行状态最重要的 4 个数字。"
          metrics={[
            { label: "今日请求", value: todayRequests.toLocaleString("zh-CN"), detail: "代理日志", icon: Activity },
            {
              label: "可用 Key",
              value: `${enabledKeyCount}`,
              detail: "启用中",
              icon: KeyRound,
              tone: enabledKeyCount > 0 ? "good" : "warning",
            },
            {
              label: "失败率",
              value: failureRateText,
              detail: "今日请求",
              icon: AlertTriangle,
              tone: failureRateTone,
            },
            { label: "今日成本", value: `¥${dashboard.todayCostCny.toFixed(2)}`, detail: "估算", icon: BadgeDollarSign },
          ]}
        />
      </div>

      <SectionCard
        title="路由队列"
        description="轻量显示当前可用对象，详细管理仍在中转站和 Key 池页面。"
      >
        <div className="grid gap-2">
          {keyPoolItems.slice(0, 6).map((key) => (
            <ObjectRow
              key={key.id}
              icon={<KeyRound className="h-4 w-4" />}
              title={key.name}
              subtitle={key.stationName}
              badges={
                <StatusBadge tone={key.enabled ? "healthy" : "disabled"}>
                  {key.enabled ? "可用" : "停用"}
                </StatusBadge>
              }
              metrics={[
                { label: "优先级", value: `${key.priority}` },
                { label: "状态", value: key.status },
              ]}
            />
          ))}
        </div>
      </SectionCard>

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
                  <Button variant="outline" onClick={() => copyText(proxyBaseUrl)}>
                    <Copy className="h-4 w-4" />
                  </Button>
                </div>
                <div className="mt-2 text-xs text-muted-foreground">
                  监听地址 {proxyStatus?.bindAddr ?? "127.0.0.1"} - 运行次数 {proxyRequestCount.toLocaleString("zh-CN")}
                </div>
              </div>
              <div className="rounded-[var(--surface-radius)] border border-border bg-white p-4 shadow-[var(--surface-shadow)]">
                <div className="text-xs text-muted-foreground">Local Key</div>
                <div className="mt-1 flex items-center justify-between gap-2">
                  <MaskedSecret value={dashboard.maskedLocalKey} />
                  <Button variant="outline" onClick={() => copyText(dashboard.maskedLocalKey)}>
                    <Copy className="h-4 w-4" />
                  </Button>
                </div>
              </div>
            </div>
          </SectionCard>

          <SectionCard title="最近活动" description="请求和价格变化合并为桌面工具活动流。">
            <div className="grid gap-4 xl:grid-cols-2">
              <div className="space-y-2">
                {dashboard.recentRequests.slice(0, 5).map((request) => (
                  <ObjectRow
                    key={request.id}
                    icon={<Server className="h-4 w-4" />}
                    title={request.model}
                    subtitle={`${request.stationName} · ${request.latencyMs}ms · ¥${request.estimatedCostCny.toFixed(3)}`}
                    badges={<StatusBadge tone={requestTone[request.status]}>{request.status}</StatusBadge>}
                    metrics={[{ label: "时间", value: request.createdAt }]}
                  />
                ))}
              </div>
              <div className="space-y-2">
                {dashboard.priceChanges.slice(0, 5).map((change) => (
                  <ObjectRow
                    key={`${change.model}-${change.stationName}`}
                    icon={<Route className="h-4 w-4" />}
                    title={change.model}
                    subtitle={change.stationName}
                    metrics={[
                      {
                        label: "变动",
                        value: `${change.deltaPercent > 0 ? "+" : ""}${change.deltaPercent.toFixed(1)}%`,
                        tone: change.deltaPercent > 0 ? "warning" : "good",
                      },
                      { label: "更新", value: change.updatedAt },
                    ]}
                  />
                ))}
              </div>
            </div>
          </SectionCard>
        </div>

        <SectionCard title="站点健康" description="状态聚合和待处理事项。">
          <div className="grid grid-cols-2 gap-3">
            {(Object.keys(dashboard.healthSummary) as Array<keyof typeof dashboard.healthSummary>).map((key) => (
              <div
                key={key}
                className="rounded-[var(--surface-radius)] border border-border bg-white p-3 shadow-[var(--surface-shadow)]"
              >
                <div className="flex items-center justify-between gap-2">
                  <StatusBadge tone={key === "healthy" ? "healthy" : key === "warning" ? "warning" : key === "error" ? "error" : "disabled"}>
                    {key}
                  </StatusBadge>
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
