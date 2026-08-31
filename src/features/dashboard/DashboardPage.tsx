import { type ReactNode, useEffect, useMemo, useState } from "react";
import {
  Activity,
  AlertTriangle,
  ArrowUp,
  BadgeDollarSign,
  BarChart3,
  ChevronRight,
  Clock3,
  FlaskConical,
  Gauge,
  Inbox,
  KeyRound,
  type LucideIcon,
  Wallet,
} from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import {
  Button,
  IconButton,
  MetricPanel,
  type MetricTone,
  ObjectRow,
  StatusBadge,
} from "@/components/ui";
import { readError } from "@/lib/errors";
import { parseTimestampLikeDate } from "@/lib/time";
import type { KeyPoolItem } from "@/lib/types/stationKeys";
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
import {
  loadRoutingRuntimeOverlayQuery,
  loadRoutingWorkspaceSnapshotQuery,
  routingQueryKeys,
} from "@/lib/queries/routingQueries";
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
import {
  summarizeDashboardKeyHealth,
  type DashboardKeyHealthStatus,
} from "@/features/dashboard/dashboardKeyHealth";

const dashboardKeyHealthLabels: Record<DashboardKeyHealthStatus, string> = {
  unchecked: "未检测",
  healthy: "正常",
  warning: "降级",
  error: "错误",
};

const dashboardKeyHealthStatuses: DashboardKeyHealthStatus[] = [
  "unchecked",
  "healthy",
  "warning",
  "error",
];

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

export function DashboardPage({
  onOpenKeyPool,
  onOpenLocalRouting,
  onOpenRequestLogs,
}: {
  onOpenKeyPool?: () => void;
  onOpenLocalRouting?: () => void;
  onOpenRequestLogs?: () => void;
}) {
  const { state: updaterState, showUpdateDialog } = useUpdater();
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
  const routingRuntimeQuery = useActivityQuery({
    queryKey: routingQueryKeys.runtimeOverlay(),
    queryFn: loadRoutingRuntimeOverlayQuery,
    staleTime: 1_000,
    refetchInterval: proxyRunning ? 1_000 : false,
  });
  const routingSnapshotQuery = useActivityQuery({
    queryKey: routingQueryKeys.workspaceSnapshot({ limit: 50 }),
    queryFn: () => loadRoutingWorkspaceSnapshotQuery({ limit: 50 }),
    staleTime: 5_000,
  });
  const stationsQuery = useActivityQuery(stationsQueryOptions());
  const balancesQuery = useActivityQuery(
    currentStationBalanceSnapshotsQueryOptions(),
  );
  const settingsQuery = useActivityQuery(settingsQueryOptions());
  const alertingQuery = useActivityQuery(alertingCurrentQueryOptions({ limit: 50 }));
  const criticalAlertingQuery = useActivityQuery(
    alertingCurrentQueryOptions({ severity: "critical", limit: 1 }),
  );

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
  const currentConcurrencyByKeyId = useMemo(
    () => new Map(
      (routingRuntimeQuery.data?.candidates ?? []).map((candidate) => [
        candidate.stationKeyId,
        candidate.stationKeyInFlight == null
          ? null
          : Math.max(0, Math.trunc(candidate.stationKeyInFlight)),
      ]),
    ),
    [routingRuntimeQuery.data],
  );
  const routingScoreByKeyId = useMemo(
    () => new Map(
      (routingSnapshotQuery.data?.candidates ?? []).map((candidate) => [
        candidate.stationKeyId,
        candidate.score,
      ]),
    ),
    [routingSnapshotQuery.data],
  );
  const stations = stationsQuery.data ?? [];
  const balanceSnapshots = balancesQuery.data ?? [];
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
  const liveMetricsState = dashboardMetricsQueryState(liveRequestMetricsQuery);
  const cumulativeMetricsState = dashboardMetricsQueryState(cumulativeRequestMetricsQuery);
  const todayRequests = todayMetrics?.requestCount ?? null;
  const enabledKeyCount = keyPoolItems.filter((key) => key.enabled).length;
  const disabledKeyCount = keyPoolItems.length - enabledKeyCount;
  const requestKeyById = useMemo(
    () => new Map(keyPoolItems.map((key) => [key.id, key])),
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
  const activeProblems = alertingQuery.data?.activeCount ?? 0;
  const criticalProblems = criticalAlertingQuery.data?.activeCount ?? 0;
  const unreadReminders = alertingQuery.data?.unseenCount ?? 0;
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
        <div data-tour="dashboard-metrics">
          <MetricPanel
          title="本地路由指标"
          columns={3}
          metrics={[
            {
              label: "今日请求",
              value: todayRequests === null ? liveMetricsState.value : formatCompactNumber(todayRequests),
              detail: `累计 ${cumulativeRequestMetrics ? formatCompactNumber(proxyRequestCount) : cumulativeMetricsState.value}`,
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
                ? `输入 ${formatCompactNumber(todayPromptTokens ?? 0)} / 输出 ${formatCompactNumber(todayCompletionTokens ?? 0)} · 累计 ${totalTokens === null ? cumulativeMetricsState.value : formatCompactNumber(totalTokens)}`
                : liveMetricsState.detail,
              icon: BarChart3,
              tone: todayTokens !== null && todayTokens > 0 ? "good" : liveMetricsState.tone,
              valueClassName: "text-foreground",
              accent: "amber",
            },
            {
              label: "可用密钥",
              value: `${enabledKeyCount}`,
              detail: disabledKeyCount > 0
                ? `${enabledKeyCount} 启用 · ${disabledKeyCount} 禁用`
                : `${enabledKeyCount} / ${keyPoolItems.length} 启用`,
              icon: KeyRound,
              tone: enabledKeyCount > 0 ? "good" : "warning",
              accent: "blue",
            },
            {
              label: "平均耗时",
              value: todayMetrics ? formatDuration(averageTotalDurationMs) : liveMetricsState.value,
              detail: todayMetrics ? formatAverageDurationDetail(todayMetrics) : liveMetricsState.detail,
              icon: Clock3,
              tone: averageTotalDurationMs !== null && averageTotalDurationMs > 15000 ? "warning" : liveMetricsState.tone,
              valueClassName: "text-foreground",
              accent: "rose",
            },
            {
              label: "实时流量",
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
                  <span className="text-muted-foreground">· {activeRequests} 活跃请求</span>
                </>
              ) : liveMetricsState.detail,
              icon: Gauge,
              tone: recentPerformance && (recentPerformance.rpm > 0 || activeRequests > 0) ? "good" : liveMetricsState.tone,
              valueClassName: "inline-flex items-baseline text-foreground",
              accent: "violet",
            },
          ]}
          />
        </div>
        <div data-tour="dashboard-station-metrics">
          <MetricPanel
          title="中转站指标统计"
          columns={4}
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
              detail: `累计 ${formatCompactNumber(stationUsage.totalTokenCount)}`,
              icon: BarChart3,
              tone: stationUsage.todayTokenCount > 0 ? "good" : "neutral",
              valueClassName: "text-foreground",
              accent: "amber",
            },
          ]}
          />
        </div>
      </div>

      <section className="grid min-w-0 gap-3" data-tour="dashboard-risk">
        <header className="flex flex-wrap items-center justify-between gap-3">
          <h2 className="truncate text-[13px] font-semibold text-foreground">
            当前风险
          </h2>
        </header>
        <div className="grid min-w-0 grid-cols-3 gap-3">
          <DashboardMetricTile
            label="活动问题"
            value={activeProblems}
            detail="未恢复告警"
            icon={AlertTriangle}
            tone={activeProblems > 0 ? "warning" : "good"}
          />
          <DashboardMetricTile
            label="严重问题"
            value={criticalProblems}
            detail="优先处理"
            icon={AlertTriangle}
            tone={criticalProblems > 0 ? "danger" : "good"}
          />
          <DashboardMetricTile
            label="未读提醒"
            value={unreadReminders}
            detail="待查看"
            icon={Inbox}
            tone={unreadReminders > 0 ? "warning" : "good"}
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

      <section className="grid min-w-0 gap-3" data-tour="dashboard-key-health">
        <h2 className="truncate text-[13px] font-semibold text-foreground">
          密钥健康
        </h2>
        <div className="flex min-w-0 flex-wrap items-center gap-x-4 gap-y-2 rounded-[12px] border border-border bg-surface px-4 py-3 shadow-surface">
          <div className="flex min-w-0 flex-1 flex-wrap items-center gap-x-2 gap-y-1 text-xs text-muted-foreground">
            {dashboardKeyHealthStatuses.map((status, index) => (
              <span key={status} className="whitespace-nowrap">
                {index > 0 ? <span className="mr-2 text-border-strong">·</span> : null}
                {dashboardKeyHealthLabels[status]}{" "}
                <span className="font-semibold text-foreground">{keyHealthSummary[status]}</span>
              </span>
            ))}
          </div>
          {onOpenKeyPool ? (
            <Button
              size="sm"
              variant="ghost"
              className="ml-auto shrink-0 text-muted-foreground"
              onClick={onOpenKeyPool}
            >
              查看详情
              <ChevronRight className="h-3.5 w-3.5" />
            </Button>
          ) : null}
        </div>
      </section>

      <div className="grid min-w-0 items-start gap-4 md:grid-cols-[minmax(0,3fr)_minmax(0,2fr)]">
      <section className="grid min-w-0 gap-3" data-tour="dashboard-routing-queue">
          <header className="flex items-center justify-between gap-3">
            <h2 className="truncate text-[13px] font-semibold text-foreground">
              路由队列
            </h2>
            {onOpenLocalRouting ? (
              <Button
                size="sm"
                variant="ghost"
                className="shrink-0 text-muted-foreground"
                onClick={onOpenLocalRouting}
              >
                查看全部
                <ChevronRight className="h-3.5 w-3.5" />
              </Button>
            ) : null}
          </header>
        <div className="grid gap-3">
          {dashboardLoaded && keyPoolItems.length === 0 ? (
            <div className="flex min-h-[164px] flex-col items-center justify-center rounded-[8px] border border-border bg-surface px-4 py-8 text-center shadow-surface">
              <div className="flex h-16 w-16 items-center justify-center rounded-[16px] bg-muted text-muted-foreground/45">
                <Inbox className="h-7 w-7" strokeWidth={1.75} />
              </div>
              <div className="mt-4 text-sm font-medium text-foreground">暂无路由队列</div>
              <p className="mt-2 text-sm text-muted-foreground">
                添加或导入密钥后，可用路由将显示在这里。
              </p>
            </div>
          ) : (
            keyPoolItems.slice(0, 6).map((key) => (
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
                  {
                    label: "当前并发",
                    value: (
                      <span className="inline-flex h-7 min-w-7 items-center justify-center rounded-[6px] bg-muted px-2 text-center text-foreground">
                        {currentConcurrencyByKeyId.get(key.id) ?? "—"}
                      </span>
                    ),
                    align: "center",
                  },
                  {
                    label: "评分",
                    value: formatDashboardRoutingScore(routingScoreByKeyId.get(key.id)),
                    tone: routingScoreByKeyId.get(key.id) == null ? "neutral" : "good",
                  },
                ]}
              />
            ))
          )}
        </div>
        </section>

        <section className="grid min-w-0 gap-3" data-tour="dashboard-recent-usage">
          <header className="flex items-center justify-between gap-3">
            <h2 className="truncate text-[13px] font-semibold text-foreground">
              最近使用
            </h2>
            {onOpenRequestLogs ? (
              <Button
                size="sm"
                variant="ghost"
                className="shrink-0 text-muted-foreground"
                onClick={onOpenRequestLogs}
              >
                查看全部
                <ChevronRight className="h-3.5 w-3.5" />
              </Button>
            ) : null}
          </header>
          <div className="grid gap-3">
            {dashboardLoaded && requestLogs.length === 0 ? (
              <div className="flex min-h-[164px] flex-col items-center justify-center rounded-[8px] border border-border bg-surface px-4 py-8 text-center shadow-surface">
                <div className="flex h-16 w-16 items-center justify-center rounded-[16px] bg-muted text-muted-foreground/45">
                  <Inbox className="h-7 w-7" strokeWidth={1.75} />
                </div>
                <div className="mt-4 text-sm font-medium text-foreground">暂无使用记录</div>
                <p className="mt-2 text-sm text-muted-foreground">
                  开始使用 API 后，您的使用历史将显示在这里。
                </p>
              </div>
            ) : (
              requestLogs.slice(0, 5).map((request) => {
                const requestKey = request.stationKeyId
                  ? requestKeyById.get(request.stationKeyId)
                  : null;
                const requestStationName =
                  (request.stationId && stationNamesById.get(request.stationId)) ||
                  requestKey?.stationName ||
                  "未知站点";
                const requestKeyName = requestKey?.name || "未知密钥";
                return (
              <div
                key={request.id}
                className="grid min-h-[84px] min-w-0 grid-cols-[36px_minmax(0,1fr)_auto] items-center gap-3 rounded-[8px] border border-border bg-surface px-3 py-3 shadow-surface transition-colors hover:bg-surface-subtle"
              >
                <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-[8px] bg-info-surface text-info-foreground">
                  <FlaskConical className="h-4 w-4" />
                </div>
                <div className="min-w-0">
                  <div className="truncate text-sm font-medium text-foreground">
                    {request.model ?? request.path}
                  </div>
                  <div className="mt-0.5 truncate text-xs text-muted-foreground">
                    {formatDateTime(request.startedAt)}
                  </div>
                  <div className="mt-0.5 truncate text-xs text-muted-foreground">
                    {requestStationName} · {requestKeyName}
                  </div>
                </div>
                <div className="min-w-[88px] text-right text-xs">
                  <div className="whitespace-nowrap font-semibold text-success-foreground">
                    {formatRecentRequestCost(request.estimatedTotalCost, request.costCurrency, request.costStatus)}
                  </div>
                  <div className="mt-1 whitespace-nowrap text-muted-foreground">
                    {formatTokenCount(request.totalTokens)} tokens
                  </div>
                </div>
              </div>
                );
              })
            )}
          </div>
        </section>
      </div>
    </PageScaffold>
  );
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
      <span>累计 </span>
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

function formatDashboardRoutingScore(score: number | null | undefined) {
  return score == null ? "-" : `${Math.round(score / 100)} 分`;
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
    : incident.eventType === "group_missing" && incident.groupName
      ? `分组缺失 · ${incident.groupName}`
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
