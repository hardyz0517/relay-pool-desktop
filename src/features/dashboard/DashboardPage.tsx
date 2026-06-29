import { useEffect, useMemo, useState } from "react";
import {
  Activity,
  AlertTriangle,
  BadgeDollarSign,
  Copy,
  KeyRound,
  Plus,
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
import { listBalanceSnapshots } from "@/lib/api/economics";
import { getProxyStatus, listRequestLogs } from "@/lib/api/proxy";
import { getSettings } from "@/lib/api/settings";
import { listKeyPoolItems } from "@/lib/api/stationKeys";
import type { BalanceSnapshot } from "@/lib/types/economics";
import type { ProxyStatus, RequestLog } from "@/lib/types/proxy";
import type { AppSettings } from "@/lib/types/settings";
import type { KeyPoolItem } from "@/lib/types/stationKeys";
import { stationStatusLabels } from "@/lib/types/stations";

type DashboardPageProps = {
  onNavigate: (pageId: "addProvider") => void;
};

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

function copyText(value: string) {
  void navigator.clipboard.writeText(value);
}

export function DashboardPage({ onNavigate }: DashboardPageProps) {
  const [proxyStatus, setProxyStatus] = useState<ProxyStatus | null>(null);
  const [requestLogs, setRequestLogs] = useState<RequestLog[]>([]);
  const [keyPoolItems, setKeyPoolItems] = useState<KeyPoolItem[]>([]);
  const [balanceSnapshots, setBalanceSnapshots] = useState<BalanceSnapshot[]>([]);
  const [settings, setSettings] = useState<AppSettings | null>(null);

  useEffect(() => {
    void Promise.all([
      getProxyStatus(),
      listRequestLogs(),
      listKeyPoolItems(),
      listBalanceSnapshots(),
      getSettings(),
    ])
      .then(([status, logs, keys, balances, nextSettings]) => {
        setProxyStatus(status);
        setRequestLogs(logs);
        setKeyPoolItems(keys);
        setBalanceSnapshots(balances);
        setSettings(nextSettings);
      })
      .catch(() => undefined);
  }, []);

  const todayLogs = useMemo(() => {
    const today = new Date().toDateString();
    return requestLogs.filter((log) => {
      const date = parseLogDate(log.startedAt);
      return !Number.isNaN(date.getTime()) && date.toDateString() === today;
    });
  }, [requestLogs]);

  const todayRequests = todayLogs.length;
  const todayFailedRequests = todayLogs.filter((log) => log.status === "failed").length;
  const failureRate = todayRequests > 0 ? todayFailedRequests / todayRequests : 0;
  const failureRateText = `${(failureRate * 100).toFixed(1)}%`;
  const failureRateTone = failureRate > 0.1 ? "danger" : failureRate > 0.03 ? "warning" : "good";
  const recentError = requestLogs.find((log) => log.status === "failed")?.errorMessage ?? "最近没有代理错误。";
  const proxyRunning = proxyStatus?.running ?? false;
  const proxyBaseUrl = proxyStatus
    ? `http://${proxyStatus.bindAddr}:${proxyStatus.port}/v1`
    : `http://127.0.0.1:${settings?.localProxyPort ?? 8787}/v1`;
  const localKeyMasked = settings?.localKeyMasked ?? "未读取";
  const enabledKeyCount = keyPoolItems.filter((key) => key.enabled).length;
  const proxyRequestCount = proxyStatus?.requestCount ?? requestLogs.length;
  const todayCost = todayLogs.reduce((sum, log) => sum + (log.estimatedTotalCost ?? 0), 0);
  const todayTokens = todayLogs.reduce((sum, log) => sum + (log.totalTokens ?? 0), 0);
  const lowBalanceStations = new Set(
    balanceSnapshots
      .filter((snapshot) => snapshot.status === "low" || snapshot.status === "depleted")
      .map((snapshot) => snapshot.stationId),
  ).size;
  const totalBalance = balanceSnapshots.reduce((sum, snapshot) => sum + (snapshot.value ?? 0), 0);

  return (
    <PageScaffold
      title="代理工作台"
      description="本地 OpenAI-compatible 入口、站点状态、近期请求和成本变化都聚在一个面板里。"
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
                <Button size="icon" variant="outline" aria-label="复制 Base URL" onClick={() => copyText(proxyBaseUrl)}>
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
                <MaskedSecret value={localKeyMasked} />
                <Button size="icon" variant="outline" aria-label="复制 Local Key" onClick={() => copyText(localKeyMasked)}>
                  <Copy className="h-4 w-4" />
                </Button>
              </div>
            </div>
          </div>
        </SectionCard>

        <MetricPanel
          title="今日指标"
          description="突出运行状态、Key 可用性、失败率和成本。"
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
            { label: "今日成本", value: formatCost(todayCost), detail: formatTokens(todayTokens), icon: BadgeDollarSign },
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
              subtitle={`${key.stationName} - ${key.stationBaseUrl}`}
              badges={
                <StatusBadge tone={key.enabled ? "healthy" : "disabled"}>
                  {key.enabled ? "可用" : "停用"}
                </StatusBadge>
              }
              metrics={[
                { label: "优先级", value: `${key.priority}` },
                {
                  label: "成功率",
                  value: key.successRate === null ? "-" : `${Math.round(key.successRate * 100)}%`,
                  tone: key.successRate !== null && key.successRate < 0.9 ? "warning" : "good",
                },
              ]}
            />
          ))}
        </div>
      </SectionCard>

      <div className="grid min-h-0 gap-3 xl:grid-cols-[minmax(0,1fr)_380px]">
        <SectionCard title="最近活动" description="请求、成本和余额变化合并为桌面工具活动流。">
          <div className="grid gap-4 xl:grid-cols-2">
            <div className="space-y-2">
              {requestLogs.slice(0, 5).map((request) => (
                <ObjectRow
                  key={request.id}
                  icon={<Server className="h-4 w-4" />}
                  title={request.model ?? request.path}
                  subtitle={`${request.path} - ${request.stationId ?? "未选择"} - ${request.durationMs ?? 0}ms - ${formatCost(request.estimatedTotalCost ?? 0)}`}
                  badges={
                    <StatusBadge tone={requestTone[request.status as keyof typeof requestTone] ?? "info"}>
                      {request.status}
                    </StatusBadge>
                  }
                  metrics={[{ label: "时间", value: formatTime(request.startedAt) }]}
                />
              ))}
            </div>
            <div className="space-y-2">
              {balanceSnapshots.slice(0, 5).map((snapshot) => (
                <ObjectRow
                  key={snapshot.id}
                  icon={<Route className="h-4 w-4" />}
                  title={snapshot.scope}
                  subtitle={`${snapshot.stationId} - ${snapshot.currency} ${snapshot.value ?? 0}`}
                  badges={<StatusBadge tone={balanceStatusTone(snapshot.status)}>{snapshot.status}</StatusBadge>}
                  metrics={[
                    { label: "来源", value: snapshot.source },
                    { label: "可信度", value: `${Math.round(snapshot.confidence * 100)}%` },
                  ]}
                />
              ))}
            </div>
          </div>
        </SectionCard>

        <SectionCard title="站点健康" description="余额与请求状态聚合。">
          <div className="grid grid-cols-2 gap-3">
            {(Object.keys(stationStatusLabels) as Array<keyof typeof stationStatusLabels>).map((key) => (
              <div
                key={key}
                className="rounded-[var(--surface-radius)] border border-border bg-white p-3 shadow-[var(--surface-shadow)]"
              >
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
            已知余额总计 {totalBalance.toFixed(2)} - 余额告警 {lowBalanceStations} - 最近错误：{recentError}
          </div>
        </SectionCard>
      </div>
    </PageScaffold>
  );
}

function parseLogDate(value: string) {
  const numeric = Number(value);
  return Number.isFinite(numeric) && numeric > 1000000000000 ? new Date(numeric) : new Date(value);
}

function formatTime(value: string) {
  const date = parseLogDate(value);
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

function balanceStatusTone(status: string) {
  if (status === "normal") {
    return "healthy";
  }
  if (status === "low") {
    return "warning";
  }
  if (status === "depleted") {
    return "error";
  }
  return "info";
}
