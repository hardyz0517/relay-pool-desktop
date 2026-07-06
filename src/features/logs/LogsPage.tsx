import { useEffect, useMemo, useState } from "react";
import { RefreshCw, Trash2 } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import {
  Button,
  ConfirmDialog,
  DataTableLite,
  EmptyState,
  InspectorPanel,
  PropertyList,
  PropertyRow,
  SegmentedControl,
  StatusBadge,
  Toolbar,
  useToast,
  type DataTableColumn,
} from "@/components/ui";
import { clearRequestLogs, listRequestLogs } from "@/lib/api/proxy";
import { listKeyPoolItems } from "@/lib/api/stationKeys";
import type { RequestLog } from "@/lib/types/proxy";
import type { KeyPoolItem } from "@/lib/types/stationKeys";

const statusTone = {
  success: "healthy",
  failed: "error",
  fallback: "warning",
} as const;

const statusLabel = {
  success: "成功",
  failed: "失败",
  fallback: "兜底",
} as const;

type LogFilter = "all" | "failed" | "fallback";

export function LogsPage() {
  const toast = useToast();
  const [logs, setLogs] = useState<RequestLog[]>([]);
  const [keys, setKeys] = useState<KeyPoolItem[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [filter, setFilter] = useState<LogFilter>("all");
  const [loading, setLoading] = useState(true);
  const [clearing, setClearing] = useState(false);
  const [clearConfirmOpen, setClearConfirmOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void refreshLogs();
  }, []);

  const filteredLogs = useMemo(() => {
    if (filter === "failed") {
      return logs.filter((log) => log.status === "failed");
    }
    if (filter === "fallback") {
      return logs.filter((log) => log.fallbackCount > 0 || log.status === "fallback");
    }
    return logs;
  }, [filter, logs]);

  const selected = filteredLogs.find((log) => log.id === selectedId) ?? filteredLogs[0] ?? null;
  const keyById = useMemo(() => new Map(keys.map((key) => [key.id, key] as const)), [keys]);
  const logColumns = useMemo<DataTableColumn<RequestLog>[]>(() => [
    { key: "time", header: "时间", className: "w-28", render: (row) => formatTime(row.startedAt) },
    { key: "path", header: "接口", render: (row) => <span className="font-semibold text-slate-800">{row.path}</span> },
    { key: "model", header: "模型", render: (row) => row.model ?? "未识别" },
    { key: "key", header: "密钥", render: (row) => formatKeyName(row, keyById) },
    { key: "station", header: "中转站", render: (row) => formatStationName(row, keyById) },
    {
      key: "status",
      header: "状态",
      className: "w-24",
      render: (row) => (
        <StatusBadge tone={statusTone[row.status as keyof typeof statusTone] ?? "info"}>
          {statusLabel[row.status as keyof typeof statusLabel] ?? row.status}
        </StatusBadge>
      ),
    },
    { key: "tokens", header: "用量", className: "w-28 text-right", render: (row) => formatTokens(row) },
    { key: "cost", header: "成本", className: "w-28 text-right", render: (row) => formatCost(row) },
    { key: "fallback", header: "兜底", className: "w-24 text-right", render: (row) => row.fallbackCount },
    { key: "latency", header: "耗时", className: "w-24 text-right", render: (row) => row.durationMs == null ? "暂无" : `${row.durationMs}ms` },
  ], [keyById]);

  async function refreshLogs(showSuccess = false) {
    setLoading(true);
    setError(null);
    try {
      const [nextLogs, nextKeys] = await Promise.all([listRequestLogs(), listKeyPoolItems()]);
      setLogs(nextLogs);
      setKeys(nextKeys);
      setSelectedId((current) => current ?? nextLogs[0]?.id ?? null);
      if (showSuccess) {
        toast.success("请求日志已刷新");
      }
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      toast.error("刷新请求日志失败", message);
    } finally {
      setLoading(false);
    }
  }

  function handleClear() {
    setClearConfirmOpen(true);
  }

  async function handleConfirmClear() {
    setClearing(true);
    setError(null);
    try {
      await clearRequestLogs();
      setLogs([]);
      setSelectedId(null);
      setClearConfirmOpen(false);
      toast.success("请求日志已清空");
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      toast.error("清空请求日志失败", message);
    } finally {
      setClearing(false);
    }
  }

  return (
    <PageScaffold title="请求日志">
      <div className="grid gap-[var(--shell-page-gap)]">
        <div className="min-w-0 overflow-hidden rounded-[var(--surface-radius)] border border-border bg-white shadow-[var(--surface-shadow)]">
          <Toolbar>
            <SegmentedControl
              value={filter}
              onChange={(value) => setFilter(value as LogFilter)}
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
          {error && <div className="border-b border-rose-100 bg-rose-50 px-3 py-2 text-sm text-rose-700">{error}</div>}
          {filteredLogs.length === 0 ? (
            <EmptyState
              title={loading ? "正在读取请求日志" : "暂无请求日志"}
              description="启动本地代理并从外部工具发起请求后，这里会出现记录。"
            />
          ) : (
            <DataTableLite
              columns={logColumns}
              rows={filteredLogs}
              getRowKey={(row) => row.id}
              selectedKey={selected?.id}
              onRowClick={(row) => setSelectedId(row.id)}
              className="rounded-none border-0 shadow-none"
            />
          )}
        </div>

        <InspectorPanel
          title="日志详情"
          description={selected ? `${selected.method} ${selected.path}` : "未选择请求"}
        >
          {selected ? (
            <div className="space-y-4 p-4">
              <PropertyList className="overflow-hidden rounded-[var(--surface-radius)] border border-border bg-white">
                <PropertyRow label="请求时间" value={formatTime(selected.startedAt)} />
                <PropertyRow label="接口" value={`${selected.method} ${selected.path}`} />
                <PropertyRow label="模型" value={selected.model ?? "未识别"} />
                <PropertyRow label="流式" value={selected.stream ? "是" : "否"} />
                <PropertyRow label="密钥" value={formatKeyName(selected, keyById)} />
                <PropertyRow label="所属中转站" value={formatStationName(selected, keyById)} />
                <PropertyRow label="上游基础地址" value={selected.upstreamBaseUrl ?? "未转发"} />
                <PropertyRow label="兜底次数" value={String(selected.fallbackCount)} />
                <PropertyRow label="耗时" value={selected.durationMs == null ? "暂无" : `${selected.durationMs}ms`} />
                <PropertyRow label="路由策略" value={selected.routePolicy ?? "未记录"} />
                <PropertyRow label="选择原因" value={selected.routeReason ?? "未记录"} />
                <PropertyRow label="用量" value={formatTokens(selected)} />
                <PropertyRow label="成本" value={formatCost(selected)} />
                <PropertyRow label="成本状态" value={statusFallback(selected.costStatus)} />
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
            <EmptyState title="暂无详情" description="选择一条请求日志查看路由解释和成本元数据。" />
          )}
        </InspectorPanel>
        <ConfirmDialog
          open={clearConfirmOpen}
          title="清空请求日志"
          description="确定要清空本地请求日志吗？此操作无法撤销。"
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
    <div className="rounded-[var(--surface-radius)] border border-border bg-white p-3">
      <div className="text-xs font-semibold text-slate-700">拒绝候选原因</div>
      <div className="mt-2 grid gap-2">
        {candidates.map((candidate, index) => (
          <div key={`${candidate.stationKeyId ?? "candidate"}-${index}`} className="rounded-lg border border-slate-100 bg-slate-50/70 p-2 text-xs leading-5 text-muted-foreground">
            <div className="font-medium text-slate-700">
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
    <div className="rounded-[var(--surface-radius)] border border-border bg-white p-3">
      <div className="text-xs font-semibold text-slate-700">经济上下文</div>
      <pre className="mt-2 max-h-40 overflow-auto rounded-lg bg-slate-50 p-2 text-xs leading-5 text-slate-600">
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

function formatTime(value: string) {
  const numeric = Number(value);
  const date = Number.isFinite(numeric) && numeric > 1000000000000 ? new Date(numeric) : new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return date.toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit", second: "2-digit" });
}

function formatKeyName(log: RequestLog, keyById: Map<string, KeyPoolItem>) {
  if (!log.stationKeyId) {
    return "未选择";
  }
  const key = keyById.get(log.stationKeyId);
  return key ? `${key.name} · ${key.apiKeyMasked}` : log.stationKeyId;
}

function formatStationName(log: RequestLog, keyById: Map<string, KeyPoolItem>) {
  if (log.stationKeyId) {
    const key = keyById.get(log.stationKeyId);
    if (key) {
      return `${key.stationName} · ${key.stationType}`;
    }
  }
  return log.stationId ?? "未选择";
}

function formatTokens(log: RequestLog) {
  if (log.totalTokens == null) {
    return log.costStatus === "unknown_usage" ? "用量未知" : "暂无";
  }
  return `${log.totalTokens.toLocaleString("zh-CN")} t`;
}

function formatCost(log: RequestLog) {
  if (log.estimatedTotalCost == null) {
    return log.costStatus === "unknown_usage" ? "未知" : "暂无";
  }
  const currency = log.costCurrency ?? "USD";
  return `${currency} ${log.estimatedTotalCost.toFixed(6)}`;
}

function formatRate(value: number) {
  return `${value.toFixed(3).replace(/0+$/, "").replace(/\.$/, "")}x`;
}

function statusFallback(value: string | null | undefined) {
  return value ?? "未知";
}

function normalizationLabel(value: string | null | undefined) {
  if (!value) return "未知";
  if (value === "complete") return "完整";
  if (value === "group_rate_only") return "仅倍率";
  if (value === "expired") return "已过期";
  if (value === "unknown_usage") return "用量未知";
  return value;
}

function formatJson(value: string) {
  try {
    return JSON.stringify(JSON.parse(value), null, 2);
  } catch {
    return value;
  }
}

function readError(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}
