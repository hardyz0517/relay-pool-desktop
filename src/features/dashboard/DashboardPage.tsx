import { useEffect, useMemo, useState } from "react";
import {
  AlertTriangle,
  BadgeDollarSign,
  Copy,
  KeyRound,
  Plus,
  Route,
  Server,
  Upload,
} from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import {
  Button,
  MetricPanel,
  ObjectRow,
  SectionCard,
  StatusBadge,
  useToast,
} from "@/components/ui";
import { listChangeEvents } from "@/lib/api/changeEvents";
import { listBalanceSnapshots } from "@/lib/api/economics";
import { getProxyStatus, listRequestLogs } from "@/lib/api/proxy";
import { getLocalAccessKey, getSettings, importRelayPoolToCCSwitch } from "@/lib/api/settings";
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
  const p9RiskBreakdown = useMemo(() => ({
    unresolvedCritical: activeRiskEvents.filter((event) => event.severity === "critical").length,
    groupBindingIssues: activeRiskEvents.filter((event) => event.eventType === "group_missing" || event.eventType === "key_group_unresolved").length,
    collectorFailures: activeRiskEvents.filter((event) => event.eventType === "collector_failed").length,
    priceRateIssues: activeRiskEvents.filter((event) => event.eventType === "price_expired" || event.eventType === "price_changed" || event.eventType === "rate_changed").length,
  }), [activeRiskEvents]);

  return (
    <PageScaffold
      title="总览"
      actions={
        <>
          <Button onClick={() => onNavigate("addProvider")}>
            <Plus className="h-4 w-4" />
            添加供应商
          </Button>
        </>
      }
    >
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
          <div className="grid gap-2 sm:grid-cols-[minmax(0,1fr)_64px] sm:items-center">
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
            <div className="flex items-center sm:justify-end">
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
              label: "当前风险",
              value: `${activeRiskEvents.length}`,
              detail: `${criticalRisks} 严重`,
              icon: AlertTriangle,
              tone: activeRiskEvents.length > 0 ? "warning" : "good",
            },
            {
              label: "可用密钥",
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
        action={<StatusBadge tone={unreadRisks > 0 ? "warning" : "healthy"}>{unreadRisks > 0 ? `${unreadRisks} 未读` : "无未读风险"}</StatusBadge>}
      >
        <div className="mb-3 grid gap-2 md:grid-cols-4">
          <RiskMiniTile label="严重未解决" value={p9RiskBreakdown.unresolvedCritical} />
          <RiskMiniTile label="分组 / 密钥" value={p9RiskBreakdown.groupBindingIssues} />
          <RiskMiniTile label="采集失败" value={p9RiskBreakdown.collectorFailures} />
          <RiskMiniTile label="价格 / 倍率" value={p9RiskBreakdown.priceRateIssues} />
        </div>
        {activeRiskEvents.length === 0 ? (
          <div className="rounded-[10px] bg-slate-50/70 p-3 text-sm text-muted-foreground">
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
              {balanceSnapshots.slice(0, 5).map((snapshot) => (
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

        <SectionCard title="站点健康">
          <div className="grid grid-cols-[repeat(auto-fit,minmax(130px,1fr))] gap-2">
            {(Object.keys(stationStatusLabels) as Array<keyof typeof stationStatusLabels>).map((key) => (
              <div
                key={key}
                className="flex min-h-[52px] items-center justify-between gap-3 rounded-[10px] border border-slate-100 bg-slate-50/70 px-3 py-2"
              >
                <div className="min-w-0">
                  <StatusBadge tone={healthTone[key]}>{stationStatusLabels[key]}</StatusBadge>
                  <div className="mt-1 text-[11px] text-muted-foreground">密钥</div>
                </div>
                <span className="shrink-0 text-lg font-semibold leading-6 text-slate-800">
                  {keyPoolItems.filter((item) => item.status === key).length}
                </span>
              </div>
            ))}
          </div>
          <div className="mt-3 rounded-[10px] bg-slate-50/70 px-3 py-2 text-xs leading-5 text-slate-700">
            已知余额总计 {totalBalance.toFixed(2)} - 余额告警 {lowBalanceStations} - 最近错误：{recentError}
          </div>
        </SectionCard>
      </div>
    </PageScaffold>
  );
}

function RiskMiniTile({ label, value }: { label: string; value: number }) {
  return (
    <div className="rounded-[10px] border border-slate-100 bg-slate-50/70 p-3">
      <div className="text-xs text-muted-foreground">{label}</div>
      <div className={cnRiskTileValue(value)}>{value}</div>
    </div>
  );
}

function cnRiskTileValue(value: number) {
  return `mt-1 text-xl font-semibold ${value > 0 ? "text-amber-700" : "text-emerald-700"}`;
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

function readError(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function isMaskedDisplayValue(value: string) {
  return /\*{2,}|\[REDACTED\]/i.test(value);
}
