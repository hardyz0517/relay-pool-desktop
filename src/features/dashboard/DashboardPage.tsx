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
  useToast,
} from "@/components/ui";
import { listChangeEvents } from "@/lib/api/changeEvents";
import { listBalanceSnapshots } from "@/lib/api/economics";
import { getProxyStatus, listRequestLogs } from "@/lib/api/proxy";
import { getSettings } from "@/lib/api/settings";
import { listKeyPoolItems } from "@/lib/api/stationKeys";
import type { ChangeEvent } from "@/lib/types/changeEvents";
import type { BalanceSnapshot } from "@/lib/types/economics";
import type { ProxyStatus, RequestLog } from "@/lib/types/proxy";
import type { AppSettings } from "@/lib/types/settings";
import type { KeyPoolItem } from "@/lib/types/stationKeys";
import { stationStatusLabels } from "@/lib/types/stations";
import { formatChangeTime, severityLabels, severityTone, unreadRiskCount } from "@/features/changes/changeEventViewModels";

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

export function DashboardPage({ onNavigate }: DashboardPageProps) {
  const toast = useToast();
  const [proxyStatus, setProxyStatus] = useState<ProxyStatus | null>(null);
  const [requestLogs, setRequestLogs] = useState<RequestLog[]>([]);
  const [keyPoolItems, setKeyPoolItems] = useState<KeyPoolItem[]>([]);
  const [balanceSnapshots, setBalanceSnapshots] = useState<BalanceSnapshot[]>([]);
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [changeEvents, setChangeEvents] = useState<ChangeEvent[]>([]);

  useEffect(() => {
    void Promise.all([
      getProxyStatus(),
      listRequestLogs(),
      listKeyPoolItems(),
      listBalanceSnapshots(),
      getSettings(),
      listChangeEvents(),
    ])
      .then(([status, logs, keys, balances, nextSettings, changes]) => {
        setProxyStatus(status);
        setRequestLogs(logs);
        setKeyPoolItems(keys);
        setBalanceSnapshots(balances);
        setSettings(nextSettings);
        setChangeEvents(changes);
      })
      .catch((requestError) => {
        toast.error("工作台刷新失败", readError(requestError));
      });
  }, []);

  async function copyText(value: string, label = "内容") {
    try {
      await navigator.clipboard.writeText(value);
      toast.success(`${label}已复制`);
    } catch (copyError) {
      toast.error("复制失败", readError(copyError));
    }
  }

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
  const activeRiskEvents = useMemo(
    () =>
      changeEvents.filter(
        (event) =>
          (event.severity === "critical" || event.severity === "warning") &&
          event.status !== "dismissed" &&
          event.status !== "resolved",
      ),
    [changeEvents],
  );
  const unreadRisks = unreadRiskCount(changeEvents);
  const criticalRisks = activeRiskEvents.filter((event) => event.severity === "critical").length;

  return (
    <PageScaffold
      title="总览"
      description="优先展示当前风险、本地代理状态、今日请求、失败率和成本摘要。"
      actions={
        <>
          <Button variant="secondary" onClick={() => void copyText(proxyBaseUrl, "本地入口")}>
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
      <div className="grid gap-4">
        <SectionCard
          title="当前路由"
          description="外部工具会优先使用这条本地 OpenAI-compatible 入口。"
          action={
            <StatusBadge tone={proxyRunning ? "healthy" : "warning"}>
              {proxyRunning ? "运行中" : "未启动"}
            </StatusBadge>
          }
        >
          <div className="grid gap-3">
            <div className="rounded-[var(--surface-radius)] border border-border bg-white p-4 shadow-[var(--surface-shadow)]">
              <div className="text-xs text-muted-foreground">Base URL</div>
              <div className="mt-1 flex min-w-0 items-center gap-2">
                <code className="min-w-0 flex-1 truncate text-[15px] font-semibold text-slate-800">
                  {proxyBaseUrl}
                </code>
                <Button size="icon" variant="outline" aria-label="复制 Base URL" onClick={() => void copyText(proxyBaseUrl, "Base URL")}>
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
                <Button size="icon" variant="outline" aria-label="复制 Local Key" onClick={() => void copyText(localKeyMasked, "Local Key")}>
                  <Copy className="h-4 w-4" />
                </Button>
              </div>
            </div>
          </div>
        </SectionCard>

        <MetricPanel
          title="今日指标"
          description="突出风险、Key 可用性、失败率和成本。"
          metrics={[
            {
              label: "当前风险",
              value: `${activeRiskEvents.length}`,
              detail: `${criticalRisks} 严重`,
              icon: AlertTriangle,
              tone: activeRiskEvents.length > 0 ? "warning" : "good",
            },
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
        title="当前风险"
        description="来自变更中心的未解决严重 / 警告事件。"
        action={<StatusBadge tone={unreadRisks > 0 ? "warning" : "healthy"}>{unreadRisks > 0 ? `${unreadRisks} 未读` : "无未读风险"}</StatusBadge>}
      >
        {activeRiskEvents.length === 0 ? (
          <div className="rounded-[var(--surface-radius)] border border-border bg-white p-3 text-sm text-muted-foreground shadow-[var(--surface-shadow)]">
            当前没有未解决的严重或警告变更。
          </div>
        ) : (
          <div className="grid gap-2">
            {activeRiskEvents.slice(0, 5).map((event) => (
              <ObjectRow
                key={event.id}
                icon={<AlertTriangle className="h-4 w-4" />}
                title={event.title}
                subtitle={`${event.message} · ${formatChangeTime(event.detectedAt)}`}
                badges={<StatusBadge tone={severityTone[event.severity]}>{severityLabels[event.severity]}</StatusBadge>}
                metrics={[{ label: "来源", value: event.source }]}
              />
            ))}
          </div>
        )}
      </SectionCard>

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

      <div className="grid min-h-0 gap-3">
        <SectionCard title="最近活动" description="请求、成本和余额变化合并为桌面工具活动流。">
          <div className="grid gap-4">
            <div className="space-y-2">
              <div className="text-sm font-semibold text-slate-900">请求日志</div>
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
              <div className="text-sm font-semibold text-slate-900">余额变化</div>
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

function readError(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}
