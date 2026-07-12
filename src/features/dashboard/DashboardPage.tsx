import { type ReactNode, useEffect, useMemo, useState } from "react";
import {
  Activity,
  AlertTriangle,
  ArrowUp,
  BadgeDollarSign,
  BarChart3,
  Clock3,
  Copy,
  FlaskConical,
  Gauge,
  Inbox,
  KeyRound,
  type LucideIcon,
  Power,
  Server,
  Upload,
  Wallet,
} from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import { usePageActivation } from "@/components/shell/PageActivity";
import {
  Button,
  IconButton,
  MetricPanel,
  type MetricTone,
  ObjectRow,
  SectionCard,
  StatusBadge,
  useToast,
} from "@/components/ui";
import { readError } from "@/lib/errors";
import { parseTimestampLikeDate } from "@/lib/time";
import { listBalanceSnapshots } from "@/lib/api/economics";
import { getProxyStatus, listRequestLogs, startLocalProxy, stopLocalProxy } from "@/lib/api/proxy";
import { getLocalAccessKey, importRelayPoolToCCSwitch } from "@/lib/api/settings";
import { loadDashboardWorkspace } from "@/lib/queries/dashboardQueries";
import type { ChangeEvent } from "@/lib/types/changeEvents";
import type { BalanceSnapshot } from "@/lib/types/economics";
import type { ProxyStatus, RequestLog } from "@/lib/types/proxy";
import type { AppSettings } from "@/lib/types/settings";
import type { KeyPoolItem, StationKeyStatus } from "@/lib/types/stationKeys";
import type { Station } from "@/lib/types/stations";
import { stationKeyStatusLabels } from "@/lib/types/stationKeys";
import { formatChangeTime, severityLabels, severityTone, unreadRiskCount } from "@/features/changes/changeEventViewModels";
import { summarizeDashboardBalances } from "@/features/dashboard/dashboardBalanceSummary";
import { formatRecentRequestCost, formatRequestCost, requestBaseCostValue } from "@/features/dashboard/requestCostFormat";
import { useUpdater } from "@/features/updater/UpdaterProvider";
import {
  summarizeDashboardRequestCosts,
  type DashboardCostTotal,
  type DashboardRequestCostSummary,
} from "@/features/dashboard/requestCostSummary";

const healthTone = {
  healthy: "healthy",
  warning: "warning",
  error: "error",
  disabled: "disabled",
  unchecked: "info",
} as const;

const DASHBOARD_BALANCE_REFRESH_INTERVAL_MS = 30_000;
const DASHBOARD_RUNTIME_REFRESH_INTERVAL_MS = 2_000;
const RECENT_PERFORMANCE_WINDOW_MINUTES = 5;

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
  const { state: updaterState, installNow } = useUpdater();
  const [proxyStatus, setProxyStatus] = useState<ProxyStatus | null>(null);
  const [requestLogs, setRequestLogs] = useState<RequestLog[]>([]);
  const [keyPoolItems, setKeyPoolItems] = useState<KeyPoolItem[]>([]);
  const [stations, setStations] = useState<Station[]>([]);
  const [balanceSnapshots, setBalanceSnapshots] = useState<BalanceSnapshot[]>([]);
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [changeEvents, setChangeEvents] = useState<ChangeEvent[]>([]);
  const [dashboardLoaded, setDashboardLoaded] = useState(false);
  const [startingLocalProxy, setStartingLocalProxy] = useState(false);
  const [stoppingLocalProxy, setStoppingLocalProxy] = useState(false);
  const [importingCCSwitch, setImportingCCSwitch] = useState(false);

  usePageActivation(() => {
    void loadDashboardWorkspace()
      .then((workspace) => {
        setProxyStatus(workspace.proxyStatus);
        setRequestLogs(workspace.requestLogs);
        setKeyPoolItems(workspace.keyPoolItems);
        setStations(workspace.stations);
        setBalanceSnapshots(workspace.balanceSnapshots);
        setSettings(workspace.settings);
        setChangeEvents(workspace.changeEvents);
        setDashboardLoaded(true);
      })
      .catch((requestError) => {
        toast.error("工作台刷新失败", readError(requestError));
      });
  });

  async function refreshDashboardRuntimeFacts() {
    const [nextProxyStatus, nextRequestLogs] = await Promise.all([
      getProxyStatus(),
      listRequestLogs(),
    ]);
    setProxyStatus(nextProxyStatus);
    setRequestLogs(nextRequestLogs);
  }

  useEffect(() => {
    const intervalId = window.setInterval(() => {
      void listBalanceSnapshots()
        .then(setBalanceSnapshots)
        .catch(() => {});
    }, DASHBOARD_BALANCE_REFRESH_INTERVAL_MS);
    return () => window.clearInterval(intervalId);
  }, []);

  useEffect(() => {
    const runtimeRefreshIntervalId = window.setInterval(() => {
      void refreshDashboardRuntimeFacts().catch(() => {});
    }, DASHBOARD_RUNTIME_REFRESH_INTERVAL_MS);
    return () => window.clearInterval(runtimeRefreshIntervalId);
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
  const recentError = requestLogs.find((log) => log.status === "failed")?.errorMessage ?? "最近没有代理错误。";
  const proxyRunning = proxyStatus?.running ?? false;
  const proxyBaseUrl = proxyStatus
    ? `http://${proxyStatus.bindAddr}:${proxyStatus.port}/v1`
    : `http://127.0.0.1:${settings?.localProxyPort ?? 8787}/v1`;
  const localKeyMasked = settings?.localKeyMasked ?? "未读取";
  const enabledKeyCount = keyPoolItems.filter((key) => key.enabled).length;
  const requestKeyNameById = useMemo(
    () => new Map(keyPoolItems.map((key) => [key.id, key.name])),
    [keyPoolItems],
  );
  const proxyRequestCount = Math.max(
    requestLogs.length,
    proxyStatus?.requestCount ?? 0,
  );
  const todayTokens = todayLogs.reduce((sum, log) => sum + (log.totalTokens ?? 0), 0);
  const todayPromptTokens = todayLogs.reduce((sum, log) => sum + (log.promptTokens ?? 0), 0);
  const todayCompletionTokens = todayLogs.reduce((sum, log) => sum + (log.completionTokens ?? 0), 0);
  const totalTokens = requestLogs.reduce((sum, log) => sum + (log.totalTokens ?? 0), 0);
  const totalPromptTokens = requestLogs.reduce((sum, log) => sum + (log.promptTokens ?? 0), 0);
  const totalCompletionTokens = requestLogs.reduce((sum, log) => sum + (log.completionTokens ?? 0), 0);
  const requestCostSummary = useMemo(() => summarizeDashboardRequestCosts(requestLogs), [requestLogs]);
  const averageResponseMs = averageDurationMs(todayLogs);
  const recentPerformance = getRecentPerformanceMetrics(requestLogs);
  const activeRequests = proxyStatus?.activeRequests ?? 0;
  const balanceSummary = useMemo(
    () => summarizeDashboardBalances(balanceSnapshots, stations),
    [balanceSnapshots, stations],
  );
  const { lowBalanceStations, primaryBalanceCurrency, stationUsage, totalBalance } = balanceSummary;
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
  const updateAction = updaterState.phase === "available" ? (
    <IconButton
      label="升级到新版本"
      title={`升级到 ${updaterState.version ?? "新版本"}`}
      variant="outline"
      className="h-8 w-8 border-cyan-200 bg-cyan-50 text-cyan-700 hover:bg-cyan-100 hover:text-cyan-800"
      onClick={() => void installNow()}
    >
      <ArrowUp className="h-4 w-4" />
    </IconButton>
  ) : null;

  return (
    <PageScaffold title="总览" actions={updateAction}>
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
          title="本地路由指标"
          metrics={[
            {
              label: "总余额",
              value: formatBalance(totalBalance, primaryBalanceCurrency),
              detail: `${lowBalanceStations} 个余额告警`,
              icon: Wallet,
              tone: lowBalanceStations > 0 ? "warning" : "good",
              valueClassName: "text-emerald-700",
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
              valueClassName: "text-slate-900",
              accent: "green",
            },
            {
              label: "今日消耗",
              value: <DashboardCostTotals totals={requestCostSummary.todayTotalsByCurrency} />,
              detail: <DashboardCostMetricDetail summary={requestCostSummary} />,
              icon: BadgeDollarSign,
              accent: "purple",
            },
            {
              label: "今日 Token",
              value: formatCompactNumber(todayTokens),
              detail: `输入 ${formatCompactNumber(todayPromptTokens)} / 输出 ${formatCompactNumber(todayCompletionTokens)}`,
              icon: BarChart3,
              tone: todayTokens > 0 ? "good" : "neutral",
              valueClassName: "text-slate-900",
              accent: "amber",
            },
            {
              label: "累计 Token",
              value: formatCompactNumber(totalTokens),
              detail: `输入 ${formatCompactNumber(totalPromptTokens)} / 输出 ${formatCompactNumber(totalCompletionTokens)}`,
              icon: Server,
              tone: totalTokens > 0 ? "good" : "neutral",
              valueClassName: "text-slate-900",
              accent: "indigo",
            },
            {
              label: "平均响应",
              value: formatDuration(averageResponseMs),
              detail: averageResponseMs === null ? "暂无今日样本" : "今日平均",
              icon: Clock3,
              tone: averageResponseMs !== null && averageResponseMs > 15000 ? "warning" : "neutral",
              valueClassName: "text-slate-900",
              accent: "rose",
            },
            {
              label: "性能概览",
              value: (
                <>
                  <span className="text-slate-900">{formatCompactNumber(recentPerformance.rpm)}</span>
                  <span className="ml-1 text-sm font-medium text-muted-foreground">RPM</span>
                </>
              ),
              detail: (
                <>
                  <span className="font-semibold text-slate-900">{formatCompactNumber(recentPerformance.tpm)}</span>
                  <span className="ml-1 text-muted-foreground">TPM</span>
                  <span className="text-muted-foreground">· {activeRequests} 活跃</span>
                </>
              ),
              icon: Gauge,
              tone: recentPerformance.rpm > 0 || activeRequests > 0 ? "good" : "neutral",
              valueClassName: "inline-flex items-baseline text-slate-900",
              accent: "violet",
            },
          ]}
        />
        <MetricPanel
          title="中转站指标统计"
          metrics={[
            {
              label: "站点今日请求",
              value: formatCompactNumber(stationUsage.todayRequestCount),
              detail: `累计 ${formatCompactNumber(stationUsage.totalRequestCount)}`,
              icon: Activity,
              tone: stationUsage.todayRequestCount > 0 ? "good" : "neutral",
              valueClassName: "text-slate-900",
              accent: "green",
            },
            {
              label: "站点今日消费",
              value: formatUsdAmount(stationUsage.todayConsumption),
              detail: `累计 ${formatUsdAmount(stationUsage.totalConsumption)}`,
              icon: BadgeDollarSign,
              tone: stationUsage.todayConsumption > 0 ? "good" : "neutral",
              valueClassName: "text-purple-700",
              accent: "purple",
            },
            {
              label: "站点今日 Token",
              value: formatCompactNumber(stationUsage.todayTokenCount),
              detail: `输入: ${formatCompactNumber(stationUsage.todayInputTokenCount)} / 输出: ${formatCompactNumber(stationUsage.todayOutputTokenCount)}`,
              icon: BarChart3,
              tone: stationUsage.todayTokenCount > 0 ? "good" : "neutral",
              valueClassName: "text-slate-900",
              accent: "amber",
            },
            {
              label: "站点累计 Token",
              value: formatCompactNumber(stationUsage.totalTokenCount),
              detail: `输入: ${formatCompactNumber(stationUsage.totalInputTokenCount)} / 输出: ${formatCompactNumber(stationUsage.totalOutputTokenCount)}`,
              icon: Server,
              tone: stationUsage.totalTokenCount > 0 ? "good" : "neutral",
              valueClassName: "text-slate-900",
              accent: "indigo",
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
          {dashboardLoaded && keyPoolItems.length === 0 ? (
            <div className="flex min-h-[164px] flex-col items-center justify-center px-4 py-8 text-center">
              <div className="flex h-16 w-16 items-center justify-center rounded-[16px] bg-slate-100 text-slate-300">
                <Inbox className="h-7 w-7" strokeWidth={1.75} />
              </div>
              <div className="mt-4 text-sm font-medium text-slate-800">暂无路由队列</div>
              <p className="mt-2 text-sm text-slate-500">
                添加或导入 Key 后，可用路由将显示在这里。
              </p>
            </div>
          ) : (
            keyPoolItems.slice(0, 6).map((key) => (
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
                  { label: "顺位", value: `${key.priority + 1}` },
                  {
                    label: "成功率",
                    value: key.successRate === null ? "-" : `${Math.round(key.successRate * 100)}%`,
                    tone: key.successRate !== null && key.successRate < 0.9 ? "warning" : "good",
                  },
                ]}
              />
            ))
          )}
        </div>
      </SectionCard>

      <div className="grid min-h-0 gap-3">
        <SectionCard title="最近使用" contentClassName="border-0">
          <div className="grid gap-3">
            {dashboardLoaded && requestLogs.length === 0 ? (
              <div className="flex min-h-[260px] flex-col items-center justify-center px-4 py-10 text-center">
                <div className="flex h-20 w-20 items-center justify-center rounded-[16px] bg-slate-100 text-slate-300">
                  <Inbox className="h-8 w-8" strokeWidth={1.75} />
                </div>
                <div className="mt-5 text-base font-medium text-slate-800">暂无使用记录</div>
                <p className="mt-2 text-sm text-slate-500">
                  开始使用 API 后，您的使用历史将显示在这里。
                </p>
              </div>
            ) : (
              requestLogs.slice(0, 5).map((request) => {
                const requestKeyName =
                  (request.stationKeyId && requestKeyNameById.get(request.stationKeyId)) ||
                  request.stationKeyId ||
                  "未知";
                return (
              <div
                key={request.id}
                className="grid min-h-[72px] grid-cols-[44px_minmax(0,1fr)_auto] items-center gap-3 rounded-[8px] bg-slate-50 px-4 py-3"
              >
                <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-[8px] bg-teal-100 text-teal-700">
                  <FlaskConical className="h-5 w-5" />
                </div>
                <div className="min-w-0">
                  <div className="flex min-w-0 items-baseline gap-2">
                    <span className="min-w-0 flex-1 truncate text-sm font-medium text-slate-900">
                      {request.model ?? request.path}
                    </span>
                    <span className="max-w-[45%] shrink truncate text-xs text-slate-500">
                      Key：{requestKeyName}
                    </span>
                  </div>
                  <div className="mt-0.5 truncate text-xs text-slate-500">
                    {formatDateTime(request.startedAt)}
                  </div>
                </div>
                <div className="min-w-[118px] text-right">
                  <div className="whitespace-nowrap text-sm font-semibold text-slate-400">
                    <span className="text-emerald-600">
                      {formatRecentRequestCost(request.estimatedTotalCost, request.costCurrency, request.costStatus)}
                    </span>
                    <span className="mx-1">/</span>
                    <span>
                      {formatRecentRequestCost(requestBaseCostValue(request), request.costCurrency, request.costStatus)}
                    </span>
                  </div>
                  <div className="mt-0.5 whitespace-nowrap text-xs text-slate-500">
                    {formatTokenCount(request.totalTokens)} tokens
                  </div>
                </div>
              </div>
                );
              })
            )}
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

function formatDateTime(value: string) {
  const date = parseLogDate(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  const time = date.toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit", second: "2-digit" });
  return `${year}/${month}/${day} ${time}`;
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

function getRecentPerformanceMetrics(logs: RequestLog[]) {
  const now = new Date();
  const windowStart = now.getTime() - RECENT_PERFORMANCE_WINDOW_MINUTES * 60_000;
  const recentLogs = logs.filter((log) => {
    const startedAt = parseLogDate(log.startedAt).getTime();
    return Number.isFinite(startedAt) && startedAt >= windowStart && startedAt <= now.getTime();
  });
  const recentTokens = recentLogs.reduce((sum, log) => sum + (log.totalTokens ?? 0), 0);
  return {
    rpm: recentLogs.length / RECENT_PERFORMANCE_WINDOW_MINUTES,
    tpm: recentTokens / RECENT_PERFORMANCE_WINDOW_MINUTES,
  };
}

function formatBalance(value: number, currency?: string) {
  const symbol = currencySymbol(currency);
  return `${symbol}${value.toFixed(2)}`;
}

function formatUsdAmount(value: number) {
  return `$${value.toFixed(value >= 100 ? 2 : 4)}`;
}

function DashboardCostTotals({ totals, compact = false }: { totals: DashboardCostTotal[]; compact?: boolean }) {
  const displayTotals = totals.length > 0
    ? totals
    : [{ currency: "USD", totalCost: 0, baseTotalCost: 0, requestCount: 0 }];

  return (
    <>
      {displayTotals.map((total, index) => {
      const symbol = currencySymbol(total.currency);
      const prefix = symbol || `${total.currency} `;
        return (
          <span key={total.currency}>
            {index > 0 ? <span className="text-slate-400"> · </span> : null}
            <span className={compact ? "text-purple-600" : undefined} title="实际花费">
              {prefix}{total.totalCost.toFixed(4)}
            </span>
            <span
              className={compact ? "font-normal text-slate-400" : "text-sm font-normal text-slate-400"}
              title="1倍率 Token 花费"
            >
              {` / ${prefix}${total.baseTotalCost.toFixed(4)}`}
            </span>
          </span>
        );
      })}
    </>
  );
}

function DashboardCostMetricDetail({ summary }: { summary: DashboardRequestCostSummary }) {
  const diagnostics: string[] = [];
  if (summary.unpricedCount > 0) {
    diagnostics.push(`${summary.unpricedCount} 未定价`);
  }
  if (summary.legacyEstimateCount > 0) {
    diagnostics.push(`${summary.legacyEstimateCount} 旧估算`);
  }
  return (
    <>
      <span>总计: </span>
      <DashboardCostTotals totals={summary.allTotalsByCurrency} compact />
      {diagnostics.map((diagnostic) => <span key={diagnostic}> · {diagnostic}</span>)}
    </>
  );
}

function formatTokenCount(value: number | null | undefined) {
  return (value ?? 0).toLocaleString("zh-CN");
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

function isMaskedDisplayValue(value: string) {
  return /\*{2,}|\[REDACTED\]/i.test(value);
}
