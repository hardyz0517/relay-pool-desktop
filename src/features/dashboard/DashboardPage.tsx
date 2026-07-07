import { type ReactNode, useEffect, useMemo, useState } from "react";
import {
  Activity,
  AlertTriangle,
  BadgeDollarSign,
  BarChart3,
  Clock3,
  Copy,
  Gauge,
  KeyRound,
  type LucideIcon,
  Power,
  Route,
  Server,
  Upload,
  Wallet,
} from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import {
  Button,
  MetricPanel,
  type MetricTone,
  ObjectRow,
  SectionCard,
  StatusBadge,
  useToast,
} from "@/components/ui";
import { readError } from "@/lib/errors";
import { parseTimestampLikeDate } from "@/lib/time";
import { listChangeEvents } from "@/lib/api/changeEvents";
import { listBalanceSnapshots } from "@/lib/api/economics";
import { getProxyStatus, listRequestLogs, startLocalProxy, stopLocalProxy } from "@/lib/api/proxy";
import { getLocalAccessKey, getSettings, importRelayPoolToCCSwitch } from "@/lib/api/settings";
import { listKeyPoolItems } from "@/lib/api/stationKeys";
import type { ChangeEvent } from "@/lib/types/changeEvents";
import type { BalanceSnapshot } from "@/lib/types/economics";
import type { ProxyStatus, RequestLog } from "@/lib/types/proxy";
import type { AppSettings } from "@/lib/types/settings";
import type { KeyPoolItem, StationKeyStatus } from "@/lib/types/stationKeys";
import { stationKeyStatusLabels } from "@/lib/types/stationKeys";
import { formatChangeTime, severityLabels, severityTone, unreadRiskCount } from "@/features/changes/changeEventViewModels";
import { summarizeDashboardBalances } from "@/features/dashboard/dashboardBalanceSummary";

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

const DASHBOARD_BALANCE_REFRESH_INTERVAL_MS = 30_000;

const dashboardMetricToneClassName: Record<MetricTone, string> = {
  neutral: "text-slate-700",
  good: "text-emerald-700",
  warning: "text-amber-700",
  danger: "text-rose-700",
};

const dashboardMetricIconClassName: Record<MetricTone, string> = {
  neutral: "bg-slate-100",
  good: "bg-emerald-50",
  warning: "bg-amber-50",
  danger: "bg-rose-50",
};

export function DashboardPage() {
  const toast = useToast();
  const [proxyStatus, setProxyStatus] = useState<ProxyStatus | null>(null);
  const [requestLogs, setRequestLogs] = useState<RequestLog[]>([]);
  const [keyPoolItems, setKeyPoolItems] = useState<KeyPoolItem[]>([]);
  const [balanceSnapshots, setBalanceSnapshots] = useState<BalanceSnapshot[]>([]);
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [changeEvents, setChangeEvents] = useState<ChangeEvent[]>([]);
  const [startingLocalProxy, setStartingLocalProxy] = useState(false);
  const [stoppingLocalProxy, setStoppingLocalProxy] = useState(false);
  const [importingCCSwitch, setImportingCCSwitch] = useState(false);

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

  useEffect(() => {
    const intervalId = window.setInterval(() => {
      void listBalanceSnapshots()
        .then(setBalanceSnapshots)
        .catch(() => {});
    }, DASHBOARD_BALANCE_REFRESH_INTERVAL_MS);
    return () => window.clearInterval(intervalId);
  }, []);

  async function copyText(value: string, label = "内容") {
    if (isMaskedDisplayValue(value)) {
      toast.error("复制失败", `${label}是脱敏展示值，不能复制。`);
      return;
    }
    try {
      await navigator.clipboard.writeText(value);
      toast.success(`${label}已复制`);
    } catch (copyError) {
      toast.error("复制失败", readError(copyError));
    }
  }

  async function copyLocalAccessKey() {
    try {
      const localAccessKey = await getLocalAccessKey();
      await navigator.clipboard.writeText(localAccessKey);
      toast.success("本地访问密钥已复制");
    } catch (copyError) {
      toast.error("复制失败", readError(copyError));
    }
  }

  async function handleStartLocalProxy() {
    setStartingLocalProxy(true);
    try {
      const nextStatus = await startLocalProxy();
      setProxyStatus(nextStatus);
      toast.success("本地路由已启动", `监听 ${nextStatus.bindAddr}:${nextStatus.port}`);
    } catch (startError) {
      toast.error("启动本地路由失败", readError(startError));
    } finally {
      setStartingLocalProxy(false);
    }
  }

  async function handleStopLocalProxy() {
    setStoppingLocalProxy(true);
    try {
      const nextStatus = await stopLocalProxy();
      setProxyStatus(nextStatus);
      toast.success("本地路由已关闭");
    } catch (stopError) {
      toast.error("关闭本地路由失败", readError(stopError));
    } finally {
      setStoppingLocalProxy(false);
    }
  }

  async function handleImportToCCSwitch() {
    setImportingCCSwitch(true);
    try {
      const result = await importRelayPoolToCCSwitch();
      toast.success("已唤起 CCSwitch", `${result.providerName} - ${result.endpoint}`);
    } catch (importError) {
      toast.error("导入 CCSwitch 失败", readError(importError));
    } finally {
      setImportingCCSwitch(false);
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
  const todaySuccessRate = todayRequests > 0 ? (todayRequests - todayFailedRequests) / todayRequests : null;
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
  const todayPromptTokens = todayLogs.reduce((sum, log) => sum + (log.promptTokens ?? 0), 0);
  const todayCompletionTokens = todayLogs.reduce((sum, log) => sum + (log.completionTokens ?? 0), 0);
  const totalTokens = requestLogs.reduce((sum, log) => sum + (log.totalTokens ?? 0), 0);
  const totalPromptTokens = requestLogs.reduce((sum, log) => sum + (log.promptTokens ?? 0), 0);
  const totalCompletionTokens = requestLogs.reduce((sum, log) => sum + (log.completionTokens ?? 0), 0);
  const totalCost = requestLogs.reduce((sum, log) => sum + (log.estimatedTotalCost ?? 0), 0);
  const averageResponseMs = averageDurationMs(todayLogs);
  const todayTpm = getTodayAverageTpm(todayTokens);
  const activeRequests = proxyStatus?.activeRequests ?? 0;
  const balanceSummary = useMemo(() => summarizeDashboardBalances(balanceSnapshots), [balanceSnapshots]);
  const { latestStationBalances, lowBalanceStations, primaryBalanceCurrency, totalBalance } = balanceSummary;
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
  const p9RiskBreakdown = useMemo(() => ({
    unresolvedCritical: activeRiskEvents.filter((event) => event.severity === "critical").length,
    groupBindingIssues: activeRiskEvents.filter((event) => event.eventType === "group_missing" || event.eventType === "key_group_unresolved").length,
    collectorFailures: activeRiskEvents.filter((event) => event.eventType === "collector_failed").length,
    priceRateIssues: activeRiskEvents.filter((event) => event.eventType === "price_expired" || event.eventType === "price_changed" || event.eventType === "rate_changed").length,
  }), [activeRiskEvents]);

  return (
    <PageScaffold title="总览">
      <div className="grid gap-4">
        <SectionCard
          title="当前路由"
          action={
            <StatusBadge tone={proxyRunning ? "healthy" : "warning"}>
              {proxyRunning ? "运行中" : "未启动"}
            </StatusBadge>
          }
          contentClassName="p-3"
        >
          <div className="grid gap-2 sm:grid-cols-[minmax(0,1fr)_136px] sm:items-center">
            <div className="grid min-w-0 gap-2 md:grid-cols-[minmax(220px,1.35fr)_minmax(170px,0.85fr)] md:items-center">
              <div className="grid min-h-9 min-w-0 grid-cols-[56px_minmax(0,1fr)_28px] items-center gap-2 rounded-[8px] bg-slate-50/70 px-2">
                <span className="text-xs font-medium text-slate-500">地址</span>
                <code className="min-w-0 truncate text-[13px] font-semibold text-slate-900">
                  {proxyBaseUrl}
                </code>
                <Button
                  size="icon"
                  variant="ghost"
                  className="h-7 w-7 shrink-0 text-slate-500 hover:text-slate-800"
                  aria-label="复制基础地址"
                  onClick={() => void copyText(proxyBaseUrl, "基础地址")}
                >
                  <Copy className="h-4 w-4" />
                </Button>
              </div>
              <div className="grid min-h-9 min-w-0 grid-cols-[56px_minmax(0,1fr)_28px] items-center gap-2 rounded-[8px] bg-slate-50/70 px-2">
                <span className="text-xs font-medium text-slate-500">密钥</span>
                <code className="min-w-0 truncate text-[13px] font-medium text-slate-700">
                  {localKeyMasked}
                </code>
                <Button
                  size="icon"
                  variant="ghost"
                  className="h-7 w-7 shrink-0 text-slate-500 hover:text-slate-800"
                  aria-label="复制本地访问密钥"
                  onClick={() => void copyLocalAccessKey()}
                >
                  <Copy className="h-3.5 w-3.5" />
                </Button>
              </div>
            </div>
            <div className="flex items-center gap-2 sm:justify-end">
              <button
                type="button"
                onClick={() => void (proxyRunning ? handleStopLocalProxy() : handleStartLocalProxy())}
                disabled={startingLocalProxy || stoppingLocalProxy}
                className={`flex h-16 w-16 shrink-0 cursor-pointer flex-col items-center justify-center gap-1.5 rounded-[8px] border px-2 py-2 text-[12px] font-medium leading-[14px] shadow-[0_1px_2px_rgba(15,23,42,0.08)] transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[hsl(var(--accent)/0.35)] disabled:pointer-events-none disabled:cursor-default disabled:opacity-60 ${
                  proxyRunning
                    ? "border-[#EFF0F3] bg-[#EFF0F3] text-slate-700 hover:bg-slate-200"
                    : "border-[#0060DF] bg-[#0060DF] text-white hover:bg-[#0052bf]"
                }`}
                aria-label={proxyRunning ? "关闭本地路由" : "启动本地路由"}
              >
                <Power className="h-4 w-4 shrink-0" />
                {startingLocalProxy ? (
                  <span>启动中</span>
                ) : stoppingLocalProxy ? (
                  <span>关闭中</span>
                ) : proxyRunning ? (
                  <span>关闭</span>
                ) : (
                  <span className="grid gap-0 text-center">
                    <span>启动</span>
                    <span>路由</span>
                  </span>
                )}
              </button>
              <button
                type="button"
                onClick={() => void handleImportToCCSwitch()}
                disabled={importingCCSwitch}
                className="flex h-16 w-16 shrink-0 cursor-pointer flex-col items-center justify-center gap-1.5 rounded-[8px] border border-slate-200 bg-white px-2 py-2 text-[12px] font-medium leading-[14px] text-slate-600 transition-colors hover:bg-slate-50 hover:text-slate-900 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[hsl(var(--accent)/0.35)] disabled:pointer-events-none disabled:cursor-default disabled:opacity-50"
                aria-label="导入到 CCSwitch"
              >
                <Upload className="h-4 w-4 shrink-0" />
                {importingCCSwitch ? (
                  <span>导入中</span>
                ) : (
                  <span className="grid gap-0 text-center">
                    <span>导入到</span>
                    <span>CCS</span>
                  </span>
                )}
              </button>
            </div>
          </div>
        </SectionCard>

        <MetricPanel
          title="今日指标"
          metrics={[
            {
              label: "总余额",
              value: formatBalance(totalBalance, primaryBalanceCurrency),
              detail: `${lowBalanceStations} 个余额告警`,
              icon: Wallet,
              tone: lowBalanceStations > 0 ? "warning" : "good",
              accent: "emerald",
            },
            {
              label: "可用密钥",
              value: `${enabledKeyCount}`,
              detail: "启用中",
              icon: KeyRound,
              tone: enabledKeyCount > 0 ? "good" : "warning",
              accent: "blue",
            },
            {
              label: "今日请求",
              value: formatCompactNumber(todayRequests),
              detail: `累计 ${formatCompactNumber(proxyRequestCount)}`,
              icon: Activity,
              tone: todayRequests > 0 ? "good" : "neutral",
              accent: "green",
            },
            {
              label: "今日消耗",
              value: formatCost(todayCost),
              detail: `累计 ${formatCost(totalCost)}`,
              icon: BadgeDollarSign,
              tone: todayCost > 0 ? "warning" : "neutral",
              accent: "purple",
            },
            {
              label: "今日 Token",
              value: formatCompactNumber(todayTokens),
              detail: `输入 ${formatCompactNumber(todayPromptTokens)} / 输出 ${formatCompactNumber(todayCompletionTokens)}`,
              icon: BarChart3,
              tone: todayTokens > 0 ? "good" : "neutral",
              accent: "amber",
            },
            {
              label: "累计 Token",
              value: formatCompactNumber(totalTokens),
              detail: `输入 ${formatCompactNumber(totalPromptTokens)} / 输出 ${formatCompactNumber(totalCompletionTokens)}`,
              icon: Server,
              tone: totalTokens > 0 ? "good" : "neutral",
              accent: "indigo",
            },
            {
              label: "平均响应",
              value: formatDuration(averageResponseMs),
              detail: averageResponseMs === null ? "暂无今日样本" : "今日平均",
              icon: Clock3,
              tone: averageResponseMs !== null && averageResponseMs > 15000 ? "warning" : "neutral",
              accent: "rose",
            },
            {
              label: "性能概览",
              value: formatPercent(todaySuccessRate),
              detail: `${formatCompactNumber(todayTpm)} TPM · ${activeRequests} 活跃`,
              icon: Gauge,
              tone: todaySuccessRate === null ? "neutral" : todaySuccessRate < 0.9 ? "warning" : "good",
              accent: "violet",
            },
          ]}
        />
      </div>

      <SectionCard
        title="当前风险"
        contentClassName="border-0"
        action={<StatusBadge tone={unreadRisks > 0 ? "warning" : "healthy"}>{unreadRisks > 0 ? `${unreadRisks} 未读` : "无未读风险"}</StatusBadge>}
      >
        <div className="mb-3 grid gap-2 md:grid-cols-4">
          <DashboardMetricTile
            label="严重未解决"
            value={p9RiskBreakdown.unresolvedCritical}
            detail="严重变更"
            icon={AlertTriangle}
            tone={p9RiskBreakdown.unresolvedCritical > 0 ? "warning" : "good"}
          />
          <DashboardMetricTile
            label="分组 / 密钥"
            value={p9RiskBreakdown.groupBindingIssues}
            detail="绑定问题"
            icon={KeyRound}
            tone={p9RiskBreakdown.groupBindingIssues > 0 ? "warning" : "good"}
          />
          <DashboardMetricTile
            label="采集失败"
            value={p9RiskBreakdown.collectorFailures}
            detail="同步异常"
            icon={Upload}
            tone={p9RiskBreakdown.collectorFailures > 0 ? "warning" : "good"}
          />
          <DashboardMetricTile
            label="价格 / 倍率"
            value={p9RiskBreakdown.priceRateIssues}
            detail="价格变更"
            icon={BadgeDollarSign}
            tone={p9RiskBreakdown.priceRateIssues > 0 ? "warning" : "good"}
          />
        </div>
        {activeRiskEvents.length === 0 ? (
          <div className="rounded-[8px] bg-slate-50/60 px-3 py-2.5 text-sm text-muted-foreground">
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
        <SectionCard title="最近活动">
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
                      {requestStatusLabel(request.status)}
                    </StatusBadge>
                  }
                  metrics={[{ label: "时间", value: formatTime(request.startedAt) }]}
                />
              ))}
            </div>
            <div className="space-y-2">
              <div className="text-sm font-semibold text-slate-900">余额变化</div>
              {latestStationBalances.slice(0, 5).map((snapshot) => (
                <ObjectRow
                  key={snapshot.id}
                  icon={<Route className="h-4 w-4" />}
                  title={snapshot.scope}
                  subtitle={`${snapshot.stationId} - ${snapshot.currency} ${snapshot.value ?? 0}`}
                  badges={<StatusBadge tone={balanceStatusTone(snapshot.status)}>{balanceStatusLabel(snapshot.status)}</StatusBadge>}
                  metrics={[
                    { label: "来源", value: snapshot.source },
                    { label: "可信度", value: `${Math.round(snapshot.confidence * 100)}%` },
                  ]}
                />
              ))}
            </div>
          </div>
        </SectionCard>

        <SectionCard title="Key 健康" contentClassName="border-0">
          <div className="grid grid-cols-[repeat(auto-fit,minmax(130px,1fr))] gap-2">
            {(Object.keys(stationKeyStatusLabels) as StationKeyStatus[]).map((key) => (
              <DashboardMetricTile
                key={key}
                label={stationKeyStatusLabels[key]}
                value={keyPoolItems.filter((item) => item.status === key).length}
                detail="密钥"
                icon={Server}
                tone={metricToneForHealth(key)}
              />
            ))}
          </div>
          <div className="mt-3 rounded-[8px] bg-slate-50/60 px-3 py-2.5 text-xs leading-5 text-slate-700">
            已知余额总计 {totalBalance.toFixed(2)} - 余额告警 {lowBalanceStations} - 最近错误：{recentError}
          </div>
        </SectionCard>
      </div>
    </PageScaffold>
  );
}

function metricToneForHealth(status: StationKeyStatus): MetricTone {
  const tone = healthTone[status];
  if (tone === "healthy") return "good";
  if (tone === "warning") return "warning";
  if (tone === "error") return "danger";
  return "neutral";
}

function DashboardMetricTile({
  label,
  value,
  detail,
  icon: Icon,
  tone = "neutral",
}: {
  label: string;
  value: ReactNode;
  detail?: ReactNode;
  icon: LucideIcon;
  tone?: MetricTone;
}) {
  return (
    <div className="flex min-h-[78px] items-center gap-3 rounded-[8px] border border-border bg-slate-50/60 px-3 py-2.5">
      <div
        className={`flex h-9 w-9 shrink-0 items-center justify-center rounded-[10px] ${dashboardMetricIconClassName[tone]} ${dashboardMetricToneClassName[tone]}`}
      >
        <Icon className="h-4 w-4" />
      </div>
      <div className="min-w-0 flex-1">
        <div className="truncate text-xs text-muted-foreground">{label}</div>
        <div className={`mt-0.5 truncate text-[21px] font-semibold leading-7 ${dashboardMetricToneClassName[tone]}`}>
          {value}
        </div>
        {detail && (
          <div className="mt-0.5 truncate text-xs text-muted-foreground">
            {detail}
          </div>
        )}
      </div>
    </div>
  );
}

function parseLogDate(value: string) {
  return parseTimestampLikeDate(value);
}

function formatTime(value: string) {
  const date = parseLogDate(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return date.toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit", second: "2-digit" });
}

function averageDurationMs(logs: RequestLog[]) {
  const durations = logs
    .map((log) => log.durationMs)
    .filter((duration): duration is number => typeof duration === "number" && Number.isFinite(duration));

  if (durations.length === 0) {
    return null;
  }

  return durations.reduce((sum, duration) => sum + duration, 0) / durations.length;
}

function getTodayAverageTpm(tokens: number) {
  if (tokens <= 0) {
    return 0;
  }

  const now = new Date();
  const startOfDay = new Date(now);
  startOfDay.setHours(0, 0, 0, 0);
  const elapsedMinutes = Math.max(1, (now.getTime() - startOfDay.getTime()) / 60000);
  return tokens / elapsedMinutes;
}

function formatBalance(value: number, currency?: string) {
  const symbol = currencySymbol(currency);
  return `${symbol}${value.toFixed(2)}`;
}

function formatCost(value: number) {
  return `¥${value.toFixed(4)}`;
}

function formatCompactNumber(value: number) {
  const absValue = Math.abs(value);
  if (absValue >= 1_000_000_000) {
    return `${trimFixed(value / 1_000_000_000)}B`;
  }
  if (absValue >= 1_000_000) {
    return `${trimFixed(value / 1_000_000)}M`;
  }
  if (absValue >= 1_000) {
    return `${trimFixed(value / 1_000)}K`;
  }
  if (!Number.isInteger(value)) {
    return trimFixed(value);
  }
  return value.toLocaleString("zh-CN");
}

function formatDuration(value: number | null) {
  if (value === null) {
    return "-";
  }
  if (value >= 1000) {
    return `${trimFixed(value / 1000)}s`;
  }
  return `${Math.round(value)}ms`;
}

function formatPercent(value: number | null) {
  if (value === null) {
    return "-";
  }
  return `${(value * 100).toFixed(1)}%`;
}

function trimFixed(value: number) {
  return value.toFixed(1).replace(/\.0$/, "");
}

function currencySymbol(currency?: string) {
  const normalized = currency?.toUpperCase();
  if (normalized === "USD") return "$";
  if (normalized === "CNY" || normalized === "RMB") return "¥";
  if (normalized === "EUR") return "€";
  if (normalized === "GBP") return "£";
  return "";
}

function requestStatusLabel(status: string) {
  if (status === "success") return "成功";
  if (status === "fallback") return "兜底";
  if (status === "failed") return "失败";
  return status;
}

function balanceStatusLabel(status: string) {
  if (status === "normal") return "正常";
  if (status === "low") return "偏低";
  if (status === "depleted") return "耗尽";
  return status;
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


function isMaskedDisplayValue(value: string) {
  return /\*{2,}|\[REDACTED\]/i.test(value);
}
