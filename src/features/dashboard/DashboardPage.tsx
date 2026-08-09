import { type ReactNode, useEffect, useMemo, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
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
import { startLocalProxy, stopLocalProxy } from "@/lib/api/proxy";
import { getLocalAccessKey, importRelayPoolToCCSwitch } from "@/lib/api/settings";
import type { KeyPoolItem, StationKeyStatus } from "@/lib/types/stationKeys";
import { stationKeyStatusLabels } from "@/lib/types/stationKeys";
import { useActivityQuery } from "@/lib/query/useActivityQuery";
import {
  currentStationBalanceSnapshotsQueryOptions,
  dashboardCumulativeRequestMetricsQueryOptions,
  dashboardLiveRequestMetricsQueryOptions,
  keyPoolQueryOptions,
  channelMonitoringQueryOptions,
  proxyStatusQueryOptions,
  requestLogsQueryOptions,
  settingsQueryOptions,
  stationsQueryOptions,
} from "@/lib/query/resourceQueries";
import { alertingCurrentQueryOptions } from "@/lib/queries/alertingQueries";
import { queryKeys } from "@/lib/query/queryKeys";
import type { AlertingIncident } from "@/lib/types/alerting";
import { summarizeDashboardBalances } from "@/features/dashboard/dashboardBalanceSummary";
import { formatRecentRequestCost } from "@/features/dashboard/requestCostFormat";
import { useUpdater } from "@/lib/updater/UpdaterProvider";
import {
  amountMicroToMajorUnits,
  getLocalDayMetricsInput,
  hasCostQualityIssue,
  msUntilNextLocalDay,
} from "@/features/dashboard/dashboardRequestMetricsViewModel";
import type { DashboardCostMetrics, DashboardCostTotal, DashboardPeriodMetrics } from "@/lib/types/dashboardMetrics";
import { summarizeDashboardKeyHealth } from "@/features/dashboard/dashboardKeyHealth";

const healthTone = {
  healthy: "healthy",
  warning: "warning",
  error: "error",
  disabled: "disabled",
  unchecked: "info",
} as const;

const dashboardKeyHealthLabels: Record<StationKeyStatus, string> = {
  unchecked: "未检测",
  healthy: "正常",
  warning: "降级",
  error: "错误",
  disabled: "禁用",
};

const dashboardMetricToneClassName: Record<MetricTone, string> = {
  neutral: "text-foreground",
  good: "text-success-foreground",
  warning: "text-warning-foreground",
  danger: "text-danger-foreground",
};

const dashboardMetricIconClassName: Record<MetricTone, string> = {
  neutral: "bg-muted",
  good: "bg-success-surface",
  warning: "bg-warning-surface",
  danger: "bg-danger-surface",
};

type DashboardMetricsQueryState = {
  value: string;
  detail: string;
  tone: MetricTone;
};

type DashboardMetricsQueryLike = {
  data: unknown;
  error: unknown;
  isError: boolean;
  isFetching: boolean;
  isLoading: boolean;
};

export function DashboardPage() {
  const toast = useToast();
  const { state: updaterState, showUpdateDialog } = useUpdater();
  const queryClient = useQueryClient();
  const proxyStatusQuery = useActivityQuery(proxyStatusQueryOptions(false));
  const [localDayMetricsInput, setLocalDayMetricsInput] = useState(() => getLocalDayMetricsInput());
  const proxyStatus = proxyStatusQuery.data ?? null;
  const proxyRunning = proxyStatus?.running ?? false;
  const liveRequestMetricsQuery = useActivityQuery(
    dashboardLiveRequestMetricsQueryOptions(
      localDayMetricsInput,
      proxyRunning ? 2_000 : false,
    ),
  );
  const cumulativeRequestMetricsQuery = useActivityQuery(
    dashboardCumulativeRequestMetricsQueryOptions(proxyRunning ? 30_000 : false),
  );
  const requestLogsQuery = useActivityQuery(
    requestLogsQueryOptions(proxyRunning ? 2_000 : false),
  );
  const keyPoolQuery = useActivityQuery(keyPoolQueryOptions());
  const channelMonitoringQuery = useActivityQuery(channelMonitoringQueryOptions(5_000));
  const stationsQuery = useActivityQuery(stationsQueryOptions());
  const balancesQuery = useActivityQuery(
    currentStationBalanceSnapshotsQueryOptions(),
  );
  const settingsQuery = useActivityQuery(settingsQueryOptions());
  const alertingQuery = useActivityQuery(alertingCurrentQueryOptions({ limit: 50 }));
  const [startingLocalProxy, setStartingLocalProxy] = useState(false);
  const [stoppingLocalProxy, setStoppingLocalProxy] = useState(false);
  const [importingCCSwitch, setImportingCCSwitch] = useState(false);

  const requestLogs = requestLogsQuery.data ?? [];
  const liveRequestMetrics = liveRequestMetricsQuery.data ?? null;
  const cumulativeRequestMetrics = cumulativeRequestMetricsQuery.data ?? null;
  const recentPerformance = liveRequestMetrics?.recent ?? null;
  const todayMetrics = liveRequestMetrics?.today ?? null;
  const lifetimeMetrics = cumulativeRequestMetrics?.lifetime ?? null;
  const todayCosts = liveRequestMetrics?.todayCosts ?? null;
  const lifetimeCosts = cumulativeRequestMetrics?.lifetimeCosts ?? null;
  const keyPoolItems = keyPoolQuery.data ?? [];
  const keyHealthSummary = useMemo(() => summarizeDashboardKeyHealth(
    keyPoolItems,
    channelMonitoringQuery.data?.monitors ?? [],
    channelMonitoringQuery.data?.statusWorkspace.rows ?? [],
  ), [channelMonitoringQuery.data, keyPoolItems]);
  const stations = stationsQuery.data ?? [];
  const balanceSnapshots = balancesQuery.data ?? [];
  const settings = settingsQuery.data ?? null;
  const alertingIncidents = alertingQuery.data?.items ?? [];
  const dashboardLoaded = [
    proxyStatusQuery.data,
    requestLogsQuery.data,
    keyPoolQuery.data,
    stationsQuery.data,
    balancesQuery.data,
    settingsQuery.data,
    alertingQuery.data,
  ].every((value) => value !== undefined);

  useEffect(() => {
    const timeout = window.setTimeout(() => {
      setLocalDayMetricsInput(getLocalDayMetricsInput());
    }, msUntilNextLocalDay());
    return () => window.clearTimeout(timeout);
  }, [localDayMetricsInput.localDayStartMs]);
  async function copyText(value: string, label = "内容") { if (isMaskedDisplayValue(value)) return; try { await navigator.clipboard.writeText(value); toast.success(`${label}已复制`); } catch (error) { toast.error("复制失败", readError(error)); } }
  async function copyLocalAccessKey() { try { await navigator.clipboard.writeText(await getLocalAccessKey()); toast.success("本地访问密钥已复制"); } catch (error) { toast.error("复制失败", readError(error)); } }
  async function handleStartLocalProxy() { setStartingLocalProxy(true); try { const nextStatus = await startLocalProxy(); queryClient.setQueryData(queryKeys.proxyStatus, nextStatus); } catch (error) { toast.error("启动本地路由失败", readError(error)); } finally { setStartingLocalProxy(false); } }
  async function handleStopLocalProxy() { setStoppingLocalProxy(true); try { const nextStatus = await stopLocalProxy(); queryClient.setQueryData(queryKeys.proxyStatus, nextStatus); } catch (error) { toast.error("关闭本地路由失败", readError(error)); } finally { setStoppingLocalProxy(false); } }
  async function handleImportToCCSwitch() { setImportingCCSwitch(true); try { const result = await importRelayPoolToCCSwitch(); toast.success("已唤起 CCSwitch", `${result.providerName} - ${result.endpoint}`); } catch (error) { toast.error("导入 CCSwitch 失败", readError(error)); } finally { setImportingCCSwitch(false); } }
  const liveMetricsState = dashboardMetricsQueryState(liveRequestMetricsQuery);
  const cumulativeMetricsState = dashboardMetricsQueryState(cumulativeRequestMetricsQuery);
  const todayRequests = todayMetrics?.requestCount ?? null;
  const proxyBaseUrl = proxyStatus ? `http://${proxyStatus.bindAddr}:${proxyStatus.port}/v1` : `http://127.0.0.1:${settings?.localProxyPort ?? 8787}/v1`;
  const localKeyMasked = settings?.localKeyMasked ?? "未读取";
  const enabledKeyCount = keyPoolItems.filter((key) => key.enabled).length;
  const requestKeyNameById = useMemo(
    () => new Map(keyPoolItems.map((key) => [key.id, key.name])),
    [keyPoolItems],
  );
  const stationNamesById = useMemo(
    () => new Map(stations.map((station) => [station.id, station.name] as const)),
    [stations],
  );
  const proxyRequestCount = Math.max(lifetimeMetrics?.requestCount ?? 0, proxyStatus?.requestCount ?? 0);
  const todayTokens = todayMetrics?.totalTokens ?? null;
  const todayPromptTokens = todayMetrics?.promptTokens ?? null;
  const todayCompletionTokens = todayMetrics?.completionTokens ?? null;
  const totalTokens = lifetimeMetrics?.totalTokens ?? null;
  const totalPromptTokens = lifetimeMetrics?.promptTokens ?? null;
  const totalCompletionTokens = lifetimeMetrics?.completionTokens ?? null;
  const averageTotalDurationMs = todayMetrics?.avgTotalDurationMs ?? null;
  const activeRequests = proxyStatus?.activeRequests ?? 0;
  const balanceSummary = useMemo(
    () => summarizeDashboardBalances(balanceSnapshots, stations),
    [balanceSnapshots, stations],
  );
  const { lowBalanceStations, primaryBalanceCurrency, stationUsage, totalBalance } = balanceSummary;
  const activeRiskEvents = useMemo(
    () =>
      alertingIncidents.filter(
        (event) =>
          (event.severity === "critical" || event.severity === "warning") &&
          event.lifecycleState !== "resolved",
      ),
    [alertingIncidents],
  );
  const unreadRisks = alertingQuery.data?.unseenCount ?? 0;
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
      className="h-8 w-8 border-info-border bg-info-surface text-info-foreground hover:bg-info-surface hover:text-info-foreground"
      onClick={showUpdateDialog}
    >
      <ArrowUp className="h-4 w-4" />
    </IconButton>
  ) : null;

  return (
    <PageScaffold title="总览" actions={updateAction}>
      <div className="grid gap-4">
        {/* <SectionCard
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
              <div className="grid min-h-9 min-w-0 grid-cols-[56px_minmax(0,1fr)_28px] items-center gap-2 rounded-[8px] bg-surface-subtle px-2">
                <span className="text-xs font-medium text-muted-foreground">地址</span>
                <code className="min-w-0 truncate text-[13px] font-semibold text-foreground">
                  {proxyBaseUrl}
                </code>
                <Button
                  size="icon"
                  variant="ghost"
                  className="h-7 w-7 shrink-0 text-muted-foreground hover:text-foreground"
                  aria-label="复制基础地址"
                  onClick={() => void copyText(proxyBaseUrl, "基础地址")}
                >
                  <Copy className="h-4 w-4" />
                </Button>
              </div>
              <div className="grid min-h-9 min-w-0 grid-cols-[56px_minmax(0,1fr)_28px] items-center gap-2 rounded-[8px] bg-surface-subtle px-2">
                <span className="text-xs font-medium text-muted-foreground">密钥</span>
                <code className="min-w-0 truncate text-[13px] font-medium text-foreground">
                  {localKeyMasked}
                </code>
                <Button
                  size="icon"
                  variant="ghost"
                  className="h-7 w-7 shrink-0 text-muted-foreground hover:text-foreground"
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
                className={`flex h-16 w-16 shrink-0 cursor-pointer flex-col items-center justify-center gap-1.5 rounded-[8px] border px-2 py-2 text-[12px] font-medium leading-[14px] shadow-surface transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/30 disabled:pointer-events-none disabled:cursor-default disabled:opacity-60 ${
                  proxyRunning
                    ? "border-border bg-surface text-foreground hover:bg-hover"
                    : "border-primary bg-primary-solid text-primary-foreground hover:bg-primary-solid"
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
                className="flex h-16 w-16 shrink-0 cursor-pointer flex-col items-center justify-center gap-1.5 rounded-[8px] border border-border bg-surface px-2 py-2 text-[12px] font-medium leading-[14px] text-muted-foreground transition-colors hover:bg-surface-subtle hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/30 disabled:pointer-events-none disabled:cursor-default disabled:opacity-50"
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
        </SectionCard> */}

        <MetricPanel
          title="本地路由指标"
          metrics={[
            {
              label: "总余额",
              value: formatBalance(totalBalance, primaryBalanceCurrency),
              detail: `${lowBalanceStations} 个余额告警`,
              icon: Wallet,
              tone: lowBalanceStations > 0 ? "warning" : "good",
              valueClassName: "text-success-foreground",
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
              value: todayRequests === null ? liveMetricsState.value : formatCompactNumber(todayRequests),
              detail: `总计：${cumulativeRequestMetrics ? formatCompactNumber(proxyRequestCount) : cumulativeMetricsState.value}`,
              icon: Activity,
              tone: todayRequests !== null && todayRequests > 0 ? "good" : liveMetricsState.tone,
              valueClassName: "text-foreground",
              accent: "green",
            },
            {
              label: "今日消耗",
              value: todayCosts ? <DashboardCostTotals totals={todayCosts.totals} /> : liveMetricsState.value,
              detail: (
                <DashboardCostMetricDetail
                  current={todayCosts}
                  currentState={liveMetricsState}
                  cumulative={lifetimeCosts}
                  cumulativeState={cumulativeMetricsState}
                />
              ),
              icon: BadgeDollarSign,
              tone: todayCosts ? "neutral" : liveMetricsState.tone,
              accent: "purple",
            },
            {
              label: "今日 Token",
              value: todayTokens === null ? liveMetricsState.value : formatCompactNumber(todayTokens),
              detail: todayMetrics
                ? `输入 ${formatCompactNumber(todayPromptTokens ?? 0)} / 输出 ${formatCompactNumber(todayCompletionTokens ?? 0)}`
                : liveMetricsState.detail,
              icon: BarChart3,
              tone: todayTokens !== null && todayTokens > 0 ? "good" : liveMetricsState.tone,
              valueClassName: "text-foreground",
              accent: "amber",
            },
            {
              label: "累计 Token",
              value: totalTokens === null ? cumulativeMetricsState.value : formatCompactNumber(totalTokens),
              detail: lifetimeMetrics
                ? `输入 ${formatCompactNumber(totalPromptTokens ?? 0)} / 输出 ${formatCompactNumber(totalCompletionTokens ?? 0)}`
                : cumulativeMetricsState.detail,
              icon: Server,
              tone: totalTokens !== null && totalTokens > 0 ? "good" : cumulativeMetricsState.tone,
              valueClassName: "text-foreground",
              accent: "indigo",
            },
            {
              label: "平均总耗时",
              value: todayMetrics ? formatDuration(averageTotalDurationMs) : liveMetricsState.value,
              detail: todayMetrics ? formatAverageDurationDetail(todayMetrics) : liveMetricsState.detail,
              icon: Clock3,
              tone: averageTotalDurationMs !== null && averageTotalDurationMs > 15000 ? "warning" : liveMetricsState.tone,
              valueClassName: "text-foreground",
              accent: "rose",
            },
            {
              label: "性能概览",
              value: recentPerformance ? (
                <>
                  <span className="text-foreground">{formatCompactNumber(recentPerformance.rpm)}</span>
                  <span className="ml-1 text-sm font-medium text-muted-foreground">RPM</span>
                </>
              ) : liveMetricsState.value,
              detail: recentPerformance ? (
                <>
                  <span className="font-semibold text-foreground">{formatCompactNumber(recentPerformance.tpm)}</span>
                  <span className="ml-1 text-muted-foreground">TPM</span>
                  <span className="text-muted-foreground">· {activeRequests} 活跃</span>
                </>
              ) : liveMetricsState.detail,
              icon: Gauge,
              tone: recentPerformance && (recentPerformance.rpm > 0 || activeRequests > 0) ? "good" : liveMetricsState.tone,
              valueClassName: "inline-flex items-baseline text-foreground",
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
              detail: `总计：${formatCompactNumber(stationUsage.totalRequestCount)}`,
              icon: Activity,
              tone: stationUsage.todayRequestCount > 0 ? "good" : "neutral",
              valueClassName: "text-foreground",
              accent: "green",
            },
            {
              label: "站点今日消费",
              value: (
                <>
                  <span title="实际花费">{formatUsdAmount(stationUsage.todayConsumption)}</span>
                  {stationUsage.todayBaseConsumption !== null && (
                    <span className="ml-1 text-sm font-normal text-muted-foreground/70" title="1倍率 Token 花费">
                      {`/ ${formatUsdAmount(stationUsage.todayBaseConsumption)}`}
                    </span>
                  )}
                </>
              ),
              detail: (
                <>
                  <span>总计：</span>
                  <span className="font-semibold text-platform-image-foreground" title="实际花费">
                    {formatUsdAmount(stationUsage.totalConsumption)}
                  </span>
                  {stationUsage.totalBaseConsumption !== null && (
                    <span className="text-muted-foreground/70" title="1倍率 Token 花费">
                      {` / ${formatUsdAmount(stationUsage.totalBaseConsumption)}`}
                    </span>
                  )}
                </>
              ),
              icon: BadgeDollarSign,
              tone: stationUsage.todayConsumption > 0 ? "good" : "neutral",
              valueClassName: "inline-flex items-baseline text-platform-image-foreground",
              accent: "purple",
            },
            {
              label: "站点今日 Token",
              value: formatCompactNumber(stationUsage.todayTokenCount),
              detail: `输入: ${formatCompactNumber(stationUsage.todayInputTokenCount)} / 输出: ${formatCompactNumber(stationUsage.todayOutputTokenCount)}`,
              icon: BarChart3,
              tone: stationUsage.todayTokenCount > 0 ? "good" : "neutral",
              valueClassName: "text-foreground",
              accent: "amber",
            },
            {
              label: "站点累计 Token",
              value: formatCompactNumber(stationUsage.totalTokenCount),
              detail: `输入: ${formatCompactNumber(stationUsage.totalInputTokenCount)} / 输出: ${formatCompactNumber(stationUsage.totalOutputTokenCount)}`,
              icon: Server,
              tone: stationUsage.totalTokenCount > 0 ? "good" : "neutral",
              valueClassName: "text-foreground",
              accent: "indigo",
            },
          ]}
        />
      </div>

      <section className="grid min-w-0 gap-3">
        <header className="flex flex-wrap items-center justify-between gap-3">
          <h2 className="truncate text-[13px] font-semibold text-foreground">
            当前风险
          </h2>
          <StatusBadge tone={unreadRisks > 0 ? "warning" : "healthy"}>
            {unreadRisks > 0 ? `${unreadRisks} 未读` : "无未读风险"}
          </StatusBadge>
        </header>
        <div className="grid min-w-0 grid-cols-4 gap-3">
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
          <div className="rounded-[8px] bg-surface-subtle px-3 py-2.5 text-sm text-muted-foreground">
            当前没有未解决的严重或警告变更。
          </div>
        ) : (
          <div className="grid min-w-0 gap-2">
            {activeRiskEvents.slice(0, 5).map((event) => {
              const item = buildAlertingRiskItem(event, stationNamesById, keyPoolItems);
              return (
                <ObjectRow
                  key={event.id}
                  className="min-w-0"
                  icon={<AlertTriangle className="h-4 w-4" />}
                  title={item.title}
                  subtitle={`${item.description} · ${formatAlertingTime(event.lastSeenAtMs)}`}
                  badges={<StatusBadge tone={alertingSeverityTone(event.severity)}>{alertingSeverityLabel(event.severity)}</StatusBadge>}
                />
              );
            })}
          </div>
        )}
      </section>

      <section className="grid gap-3">
        <h2 className="truncate text-[13px] font-semibold text-foreground">
          路由队列
        </h2>
        <div className="grid gap-3">
          {dashboardLoaded && keyPoolItems.length === 0 ? (
            <div className="flex min-h-[164px] flex-col items-center justify-center rounded-[8px] border border-border bg-surface px-4 py-8 text-center shadow-[var(--surface-shadow)]">
              <div className="flex h-16 w-16 items-center justify-center rounded-[16px] bg-muted text-muted-foreground/45">
                <Inbox className="h-7 w-7" strokeWidth={1.75} />
              </div>
              <div className="mt-4 text-sm font-medium text-foreground">暂无路由队列</div>
              <p className="mt-2 text-sm text-muted-foreground">
                添加或导入密钥后，可用路由将显示在这里。
              </p>
            </div>
          ) : (
            keyPoolItems.slice(0, 6).map((key, index) => (
              <ObjectRow
                key={key.id}
                icon={<KeyRound className="h-4 w-4" />}
                title={key.name}
                subtitle={`${key.stationName} - ${key.stationApiBaseUrl}`}
                badges={
                  <StatusBadge tone={key.enabled ? "healthy" : "disabled"}>
                    {key.enabled ? "可用" : "停用"}
                  </StatusBadge>
                }
                metrics={[
                  { label: "顺位", value: `${index + 1}` },
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
      </section>

      <div className="grid min-h-0 gap-3">
        <section className="grid gap-3">
          <h2 className="truncate text-[13px] font-semibold text-foreground">
            最近使用
          </h2>
          <div className="grid gap-3">
            {dashboardLoaded && requestLogs.length === 0 ? (
              <div className="flex min-h-[260px] flex-col items-center justify-center rounded-[8px] border border-border bg-surface px-4 py-10 text-center shadow-[var(--surface-shadow)]">
                <div className="flex h-20 w-20 items-center justify-center rounded-[16px] bg-muted text-muted-foreground/45">
                  <Inbox className="h-8 w-8" strokeWidth={1.75} />
                </div>
                <div className="mt-5 text-base font-medium text-foreground">暂无使用记录</div>
                <p className="mt-2 text-sm text-muted-foreground">
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
                className="grid min-h-[72px] grid-cols-[44px_minmax(0,1fr)_auto] items-center gap-3 rounded-[8px] border border-border bg-surface px-4 py-3 shadow-surface transition-colors hover:bg-surface-subtle"
              >
                <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-[8px] bg-info-surface text-info-foreground">
                  <FlaskConical className="h-5 w-5" />
                </div>
                <div className="min-w-0">
                  <div className="flex min-w-0 items-baseline gap-2">
                    <span className="min-w-0 flex-1 truncate text-sm font-medium text-foreground">
                      {request.model ?? request.path}
                    </span>
                    <span className="max-w-[45%] shrink truncate text-xs text-muted-foreground">
                      密钥：{requestKeyName}
                    </span>
                  </div>
                  <div className="mt-0.5 truncate text-xs text-muted-foreground">
                    {formatDateTime(request.startedAt)}
                  </div>
                </div>
                <div className="min-w-[118px] text-right">
                  <div className="whitespace-nowrap text-sm font-semibold text-muted-foreground/70">
                    <span className="text-success-foreground">
                      {formatRecentRequestCost(request.estimatedTotalCost, request.costCurrency, request.costStatus)}
                    </span>
                  </div>
                  <div className="mt-0.5 whitespace-nowrap text-xs text-muted-foreground">
                    {formatTokenCount(request.totalTokens)} tokens
                  </div>
                </div>
              </div>
                );
              })
            )}
          </div>
        </section>

        <section className="grid gap-3">
          <h2 className="truncate text-[13px] font-semibold text-foreground">
            密钥健康
          </h2>
          <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-5">
            {(Object.keys(stationKeyStatusLabels) as StationKeyStatus[]).map((key) => (
              <DashboardMetricTile
                key={key}
                label={dashboardKeyHealthLabels[key]}
                value={keyHealthSummary[key]}
                detail="密钥"
                icon={Server}
                tone={metricToneForHealth(key)}
              />
            ))}
          </div>
        </section>
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
    <div className="flex min-h-[96px] min-w-0 items-center gap-3 rounded-[12px] border border-border bg-surface px-4 py-3 shadow-surface">
      <div
        className={`flex h-9 w-9 shrink-0 items-center justify-center rounded-[8px] ${dashboardMetricIconClassName[tone]} ${dashboardMetricToneClassName[tone]}`}
      >
        <Icon className="h-4 w-4" />
      </div>
      <div className="min-w-0 flex-1">
        <div className="truncate text-xs text-muted-foreground">{label}</div>
        <div className={`mt-0.5 truncate text-[22px] font-semibold leading-7 ${dashboardMetricToneClassName[tone]}`}>
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
    : [{ currency: "USD", amountMicro: 0, requestCount: 0 }];

  return (
    <>
      {displayTotals.map((total, index) => {
      const symbol = currencySymbol(total.currency);
      const prefix = symbol || `${total.currency} `;
        return (
          <span key={total.currency}>
            {index > 0 ? <span className="text-muted-foreground/70"> · </span> : null}
            <span className={compact ? "text-platform-image-foreground" : undefined} title="实际花费">
              {prefix}{amountMicroToMajorUnits(total).toFixed(4)}
            </span>
          </span>
        );
      })}
    </>
  );
}

function DashboardCostMetricDetail({
  current,
  currentState,
  cumulative,
  cumulativeState,
}: {
  current: DashboardCostMetrics | null;
  currentState: DashboardMetricsQueryState;
  cumulative: DashboardCostMetrics | null;
  cumulativeState: DashboardMetricsQueryState;
}) {
  const diagnostics: string[] = [];
  if (current && hasCostQualityIssue(current)) {
    diagnostics.push("今日成本不完整");
  }
  if (cumulative && hasCostQualityIssue(cumulative)) {
    diagnostics.push("累计成本不完整");
  }
  return (
    <>
      <span>总计: </span>
      {cumulative ? (
        <DashboardCostTotals totals={cumulative.totals} compact />
      ) : (
        <span>{cumulativeState.value}</span>
      )}
      {!current ? <span> · 今日 {currentState.detail}</span> : null}
      {diagnostics.map((diagnostic) => <span key={diagnostic}> · {diagnostic}</span>)}
    </>
  );
}

function dashboardMetricsQueryState(query: DashboardMetricsQueryLike): DashboardMetricsQueryState {
  if (query.data) {
    return {
      value: query.isFetching ? "刷新中" : "已读取",
      detail: query.isFetching ? "后台刷新中" : "snapshot ready",
      tone: "neutral",
    };
  }
  if (query.isError) {
    return {
      value: "读取失败",
      detail: readError(query.error),
      tone: "warning",
    };
  }
  if (query.isLoading || query.isFetching) {
    return {
      value: "读取中",
      detail: "等待后端指标快照",
      tone: "neutral",
    };
  }
  return {
    value: "未读取",
    detail: "页面激活后读取",
    tone: "neutral",
  };
}

function formatAverageDurationDetail(metrics: DashboardPeriodMetrics) {
  if (metrics.durationSampleCount === 0) {
    return "暂无今日样本";
  }
  const firstToken = metrics.avgFirstTokenMs === null
    ? "TTFT 无样本"
    : `TTFT ${formatDuration(metrics.avgFirstTokenMs)}`;
  return `${firstToken} · ${formatCompactNumber(metrics.durationSampleCount)} 样本`;
}

function formatTokenCount(value: number | null | undefined) {
  return (value ?? 0).toLocaleString("zh-CN");
}

function buildAlertingRiskItem(
  incident: AlertingIncident,
  stationNamesById: Map<string, string>,
  keyPoolItems: KeyPoolItem[],
) {
  const stationName = incident.stationId ? stationNamesById.get(incident.stationId) : null;
  const keyId = incident.conditionKey.startsWith("key:") ? incident.conditionKey.slice(4) : null;
  const key = keyId ? keyPoolItems.find((item) => item.id === keyId) : null;
  const keyOwner = key?.stationName ?? stationName ?? "未知站点";
  const keyLabel = key?.name ?? keyId ?? "未知密钥";
  const title = incident.eventType === "key_invalid"
    ? `${keyOwner} 的密钥「${keyLabel}」无效`
    : eventLabel(incident.eventType);
  const scope = stationName ?? incident.stationId ?? incident.conditionKey;
  const state = incidentStateLabel(incident.lifecycleState);
  const description = incident.eventType === "key_invalid"
    ? `${keyOwner} · 密钥「${keyLabel}」· ${state} · 第 ${incident.episodeNumber} 次`
    : `${scope} · ${state} · 第 ${incident.episodeNumber} 次`;
  return { title, description };
}

function eventLabel(eventType: string) {
  return ({
    collector_failed: "采集失败",
    station_down: "站点不可用",
    balance_low: "余额偏低",
    balance_depleted: "余额耗尽",
    price_expired: "价格已过期",
    key_invalid: "密钥无效",
    route_impacted: "路由受影响",
    group_missing: "分组缺失",
    key_group_unresolved: "密钥分组无法解析",
    group_added: "新增分组",
    rate_changed: "倍率变化",
    group_rate_changed: "分组倍率变化",
    price_changed: "价格变化",
    model_added: "新增模型",
    model_removed: "模型移除",
    audit_change: "配置变更",
  } as Record<string, string>)[eventType] ?? eventType;
}

function formatAlertingTime(value: number) {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? "时间未知" : date.toLocaleString("zh-CN", { month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit" });
}

function incidentStateLabel(state: AlertingIncident["lifecycleState"]) {
  return ({ pending: "检测中", open: "未处理", recovering: "恢复中", resolved: "已恢复" } as Record<string, string>)[state] ?? state;
}

function alertingSeverityTone(severity: AlertingIncident["severity"]): "error" | "warning" | "info" {
  return severity === "critical" ? "error" : severity === "warning" ? "warning" : "info";
}

function alertingSeverityLabel(severity: AlertingIncident["severity"]) {
  return severity === "critical" ? "严重" : severity === "warning" ? "警告" : "信息";
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
