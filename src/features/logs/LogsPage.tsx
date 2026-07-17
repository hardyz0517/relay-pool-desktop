import { useMemo, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { RefreshCw, Trash2 } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import { usePageRefreshEnabled } from "@/components/shell/PageActivity";
import {
  Button,
  ConfirmDialog,
  EmptyState,
  InspectorPanel,
  PropertyList,
  PropertyRow,
  SegmentedControl,
  StatusBadge,
  Toolbar,
  useToast,
} from "@/components/ui";
import { readError } from "@/lib/errors";
import { formatRate } from "@/lib/formatters";
import { clearRequestLogs } from "@/lib/api/proxy";
import { useActivityQuery } from "@/lib/query/useActivityQuery";
import {
  keyPoolQueryOptions,
  proxyStatusQueryOptions,
  requestLogsQueryOptions,
  settingsQueryOptions,
} from "@/lib/query/resourceQueries";
import { queryKeys } from "@/lib/query/queryKeys";
import { RequestLogPagination, RequestLogTable } from "./RequestLogTable";
import {
  formatKeyName,
  formatLogTime,
  formatRequestCost,
  formatStationName,
  formatTokenTotal,
  normalizationLabel,
  paginateRequestLogs,
  pricingStatusLabel,
  requestTraceRows,
  statusFallback,
} from "./requestLogViewModels";

type LogFilter = "all" | "failed" | "fallback";

export function LogsPage() {
  const toast = useToast();
  const queryClient = useQueryClient();
  const refreshEnabled = usePageRefreshEnabled();
  const proxyStatusQuery = useActivityQuery(refreshEnabled, proxyStatusQueryOptions(false));
  const logsQuery = useActivityQuery(
    refreshEnabled,
    requestLogsQueryOptions(proxyStatusQuery.data?.running ? 2_000 : false),
  );
  const keysQuery = useActivityQuery(refreshEnabled, keyPoolQueryOptions());
  const settingsQuery = useActivityQuery(refreshEnabled, settingsQueryOptions());
  const logs = logsQuery.data ?? [];
  const keys = keysQuery.data ?? [];
  const developerModeEnabled = settingsQuery.data?.developerModeEnabled ?? false;
  const loading = logsQuery.isPending && logsQuery.data === undefined;
  const error = logsQuery.error ? readError(logsQuery.error) : null;
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [filter, setFilter] = useState<LogFilter>("all");
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(20);
  const [clearing, setClearing] = useState(false);
  const [clearConfirmOpen, setClearConfirmOpen] = useState(false);

  const filteredLogs = useMemo(() => {
    if (filter === "failed") {
      return logs.filter((log) => log.status === "failed");
    }
    if (filter === "fallback") {
      return logs.filter((log) => log.fallbackCount > 0 || log.status === "fallback");
    }
    return logs;
  }, [filter, logs]);

  const pageInfo = useMemo(
    () => paginateRequestLogs(filteredLogs, page, pageSize),
    [filteredLogs, page, pageSize],
  );
  const selected = pageInfo.logs.find((log) => log.id === selectedId) ?? pageInfo.logs[0] ?? null;
  const keyById = useMemo(() => new Map(keys.map((key) => [key.id, key] as const)), [keys]);

  async function refreshLogs(showSuccess = false) {
    setPage(1);
    try {
      await Promise.all([
        queryClient.refetchQueries({ queryKey: queryKeys.requestLogs, type: "active" }),
        queryClient.refetchQueries({ queryKey: queryKeys.keyPool, type: "active" }),
        queryClient.refetchQueries({ queryKey: queryKeys.settings, type: "active" }),
      ]);
      if (showSuccess) {
        toast.success("使用记录已刷新");
      }
    } catch (requestError) {
      const message = readError(requestError);
      toast.error("刷新使用记录失败", message);
    }
  }

  function handleClear() {
    setClearConfirmOpen(true);
  }

  async function handleConfirmClear() {
    setClearing(true);
    try {
      await queryClient.cancelQueries({ queryKey: queryKeys.requestLogs });
      await clearRequestLogs();
      queryClient.setQueryData(queryKeys.requestLogs, []);
      setPage(1);
      setSelectedId(null);
      setClearConfirmOpen(false);
      toast.success("使用记录已清空");
    } catch (requestError) {
      const message = readError(requestError);
      toast.error("清空使用记录失败", message);
    } finally {
      setClearing(false);
    }
  }

  function handleFilterChange(value: string) {
    setFilter(value as LogFilter);
    setPage(1);
    setSelectedId(null);
  }

  function handlePageSizeChange(value: number) {
    setPageSize(value);
    setPage(1);
    setSelectedId(null);
  }

  return (
    <PageScaffold title="使用记录">
      <div className="grid gap-[var(--shell-page-gap)]">
        <div className="min-w-0">
          <div
            data-testid="request-log-toolbar-surface"
            className="overflow-hidden rounded-[var(--surface-radius)] border border-border bg-surface shadow-[var(--surface-shadow)]"
          >
            <Toolbar>
              <SegmentedControl
                value={filter}
                onChange={handleFilterChange}
                options={[
                  { value: "all", label: "全部" },
                  { value: "failed", label: "失败" },
                  { value: "fallback", label: "兜底" },
                ]}
              />
              <div className="flex gap-2">
                <Button variant="outline" onClick={() => void refreshLogs(true)}>
                  <RefreshCw className="h-4 w-4" />
                  刷新
                </Button>
                <Button variant="danger" onClick={handleClear}>
                  <Trash2 className="h-4 w-4" />
                  清空
                </Button>
              </div>
            </Toolbar>
          </div>
          <div
            data-testid="request-log-table-surface"
            className="mt-3 overflow-hidden rounded-[var(--surface-radius)] border border-border bg-surface shadow-[var(--surface-shadow)]"
          >
            {error && <div className="border-b border-danger-border bg-danger-surface px-3 py-2 text-sm text-danger-foreground">{error}</div>}
            {filteredLogs.length === 0 ? (
              <EmptyState
                title={loading ? "正在读取使用记录" : "暂无使用记录"}
                description="启动本地代理并从外部工具发起请求后，这里会出现记录。"
              />
            ) : (
              <RequestLogTable
                rows={pageInfo.logs}
                keyById={keyById}
                selectedId={selected?.id ?? null}
                onSelect={setSelectedId}
              />
            )}
          </div>
          {filteredLogs.length > 0 && (
            <RequestLogPagination
              pageInfo={pageInfo}
              pageSize={pageSize}
              onPageChange={(nextPage) => {
                setPage(nextPage);
                setSelectedId(null);
              }}
              onPageSizeChange={handlePageSizeChange}
            />
          )}
        </div>

        {developerModeEnabled && (
          <InspectorPanel
            title="日志详情"
            description={selected ? `${selected.method} ${selected.path}` : "未选择请求"}
          >
            {selected ? (
              <div className="space-y-4 p-4">
                <PropertyList className="overflow-hidden rounded-[var(--surface-radius)] border border-border bg-surface">
                  <PropertyRow label="请求时间" value={formatLogTime(selected.startedAt)} />
                  <PropertyRow label="接口" value={`${selected.method} ${selected.path}`} />
                  <PropertyRow label="模型" value={selected.model ?? "未识别"} />
                  <PropertyRow label="推理强度" value={selected.reasoningEffort ?? "未记录"} />
                  <PropertyRow label="流式" value={selected.stream ? "是" : "否"} />
                  <PropertyRow label="密钥" value={formatKeyName(selected, keyById)} />
                  <PropertyRow label="所属中转站" value={formatStationName(selected, keyById)} />
                  <PropertyRow label="上游基础地址" value={selected.upstreamBaseUrl ?? "未转发"} />
                  <PropertyRow label="兜底次数" value={String(selected.fallbackCount)} />
                  <PropertyRow label="耗时" value={selected.durationMs == null ? "暂无" : `${selected.durationMs}ms`} />
                  <PropertyRow label="首字延迟" value={selected.firstTokenMs == null ? "暂无" : `${selected.firstTokenMs}ms`} />
                  {requestTraceRows(selected).map((row) => (
                    <PropertyRow key={row.label} label={row.label} value={row.value} />
                  ))}
                  <PropertyRow label="路由策略" value={selected.routePolicy ?? "未记录"} />
                  <PropertyRow label="选择原因" value={selected.routeReason ?? "未记录"} />
                  <PropertyRow label="用量" value={formatTokenTotal(selected)} />
                  <PropertyRow label="缓存读取" value={selected.cacheReadTokens == null ? "暂无" : `${selected.cacheReadTokens} t`} />
                  <PropertyRow label="缓存创建" value={selected.cacheCreationTokens == null ? "暂无" : `${selected.cacheCreationTokens} t`} />
                  <PropertyRow label="成本" value={formatRequestCost(selected)} />
                  <PropertyRow label="计费模式" value={selected.billingMode ?? "未记录"} />
                  <PropertyRow label="成本状态" value={pricingStatusLabel(selected.costStatus)} />
                  <PropertyRow label="价格规则" value={selected.pricingRuleId ?? "未命中"} />
                  <PropertyRow label="价格来源" value={selected.pricingSource ?? "未知"} />
                  <PropertyRow label="价格状态" value={normalizationLabel(selected.normalizationStatus ?? selected.costStatus)} />
                  <PropertyRow label="分组绑定" value={selected.groupBindingId ?? "未记录"} />
                  <PropertyRow label="余额作用域" value={statusFallback(selected.balanceScope)} />
                  <PropertyRow label="拒绝候选" value={`${parseRejectedCandidates(selected.rejectedCandidatesJson).length} 个`} />
                </PropertyList>
                <RejectedCandidateList json={selected.rejectedCandidatesJson} />
                <EconomicContextPreview json={selected.economicContextJson} />
              </div>
            ) : (
              <EmptyState title="暂无详情" description="选择一条使用记录查看路由解释和成本元数据。" />
            )}
          </InspectorPanel>
        )}
        <ConfirmDialog
          open={clearConfirmOpen}
          title="清空使用记录"
          description="确定要清空本地使用记录吗？此操作无法撤销。"
          confirmLabel="清空"
          confirming={clearing}
          onCancel={() => setClearConfirmOpen(false)}
          onConfirm={() => void handleConfirmClear()}
        />
      </div>
    </PageScaffold>
  );
}

type RejectedCandidateLog = {
  stationKeyId?: string;
  stationName?: string;
  keyName?: string;
  rejectionReasons?: string[];
  groupBindingId?: string | null;
  rateMultiplier?: number | null;
  normalizationStatus?: string | null;
  balanceStatus?: string | null;
  balanceScope?: string | null;
  economicFreshness?: string | null;
};

function RejectedCandidateList({ json }: { json: string | null }) {
  const candidates = parseRejectedCandidates(json);
  if (candidates.length === 0) {
    return null;
  }
  return (
    <div className="rounded-[var(--surface-radius)] border border-border bg-surface p-3">
      <div className="text-xs font-semibold text-foreground">拒绝候选原因</div>
      <div className="mt-2 grid gap-2">
        {candidates.map((candidate, index) => (
          <div key={`${candidate.stationKeyId ?? "candidate"}-${index}`} className="rounded-lg border border-border bg-surface-subtle p-2 text-xs leading-5 text-muted-foreground">
            <div className="font-medium text-foreground">
              {candidate.keyName ?? candidate.stationKeyId ?? "未知密钥"}
              {candidate.stationName ? ` · ${candidate.stationName}` : ""}
            </div>
            <div className="mt-1 grid gap-1 sm:grid-cols-2">
              <div>分组：{candidate.groupBindingId ?? "未绑定"}</div>
              <div>倍率：{candidate.rateMultiplier == null ? "未知" : formatRate(candidate.rateMultiplier)}</div>
              <div>价格：{normalizationLabel(candidate.normalizationStatus)}</div>
              <div>余额：{candidate.balanceStatus ?? "未知"} · {candidate.balanceScope ?? "未知"}</div>
              <div>新鲜度：{candidate.economicFreshness ?? "未知"}</div>
            </div>
            <div className="mt-2 flex flex-wrap gap-1">
              {(candidate.rejectionReasons?.length ? candidate.rejectionReasons : ["未记录原因"]).map((reason) => (
                <StatusBadge key={reason} className="h-5 px-1.5" tone="warning">{reason}</StatusBadge>
              ))}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

function EconomicContextPreview({ json }: { json: string | null }) {
  if (!json) {
    return null;
  }
  return (
    <div className="rounded-[var(--surface-radius)] border border-border bg-surface p-3">
      <div className="text-xs font-semibold text-foreground">经济上下文</div>
      <pre className="mt-2 max-h-40 overflow-auto rounded-lg bg-surface-subtle p-2 text-xs leading-5 text-muted-foreground">
        {formatJson(json)}
      </pre>
    </div>
  );
}

function parseRejectedCandidates(json: string | null): RejectedCandidateLog[] {
  if (!json) {
    return [];
  }
  try {
    const value = JSON.parse(json);
    return Array.isArray(value) ? (value as RejectedCandidateLog[]) : [];
  } catch {
    return [];
  }
}

function formatJson(value: string) {
  try {
    return JSON.stringify(JSON.parse(value), null, 2);
  } catch {
    return value;
  }
}
