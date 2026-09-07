import { Fragment, useMemo } from "react";
import { ArrowDown, ArrowUp, Database } from "lucide-react";
import { DataTableLite, PageSizeSelect, Pagination, type DataTableColumn } from "@/components/ui";
import { ModelMappingDisplay } from "@/components/status/ModelMappingDisplay";
import type { RequestLog } from "@/lib/types/proxy";
import type { KeyPoolItem } from "@/lib/types/stationKeys";
import type { Station } from "@/lib/types/stations";
import {
  billingModeLabel,
  formatCompactTokenCount,
  formatEndpoint,
  formatGroupName,
  formatKeyName,
  formatKeyRate,
  formatLogTime,
  formatRequestCost,
  formatRequestTokenCount,
  isRequestInProgress,
  latencyBreakdown,
  latencyTone,
  reasoningEffortLabel,
  requestInputTokenCount,
  type RequestLatencyTone,
} from "./requestLogViewModels";

type RequestLogTableProps = {
  rows: RequestLog[];
  keyById: Map<string, KeyPoolItem>;
  stationById: Map<string, Pick<Station, "creditPerCny">>;
  selectedId: string | null;
  onSelect: (id: string) => void;
  compact?: boolean;
};

type RequestLogPaginationProps = {
  pageInfo: {
    page: number;
    totalPages: number;
    startIndex: number;
    endIndex: number;
    totalCount: number;
  };
  pageSize: number;
  onPageChange: (page: number) => void;
  onPageSizeChange: (pageSize: number) => void;
};

export function RequestLogTable({
  rows,
  keyById,
  stationById,
  onSelect,
  compact = true,
}: RequestLogTableProps) {
  const columns = useMemo<DataTableColumn<RequestLog>[]>(() => {
    const allColumns: DataTableColumn<RequestLog>[] = [
      { key: "key", header: "密钥", render: (row) => formatKeyName(row, keyById) },
      {
        key: "model",
        header: "模型",
        render: (row) => (
          <ModelMappingDisplay
            requestedModel={row.model}
            resolvedModel={row.resolvedUpstreamModel}
          />
        ),
      },
      { key: "reasoning", header: "推理强度", render: (row) => reasoningEffortLabel(row.reasoningEffort) },
      { key: "endpoint", header: "端点", render: (row) => formatEndpoint(row.path) },
      {
        key: "httpStatus",
        header: "状态码",
        render: (row) => <RequestStatusCode value={row.httpStatus} inProgress={isRequestInProgress(row)} />,
      },
      { key: "group", header: "分组", render: (row) => <LogMetaTag value={formatGroupName(row, keyById)} /> },
      { key: "rate", header: "倍率", render: (row) => <LogMetaTag value={formatKeyRate(row, keyById, stationById)} /> },
      { key: "type", header: "类型", render: (row) => <LogMetaTag value={row.stream ? "流式" : "同步"} /> },
      { key: "billing", header: "计费模式", render: (row) => <LogMetaTag value={billingModeLabel(row.billingMode)} /> },
      { key: "tokens", header: "Token", render: (row) => <TokenUsageCell log={row} /> },
      {
        key: "cost",
        header: "费用",
        className: "text-center",
        render: (row) => <span className="font-medium text-success-foreground">{formatRequestCost(row)}</span>,
      },
      {
        key: "latency",
        header: "延迟",
        className: "w-[128px] min-w-[128px] max-w-[128px] text-center",
        render: (row) => <LatencyCell log={row} />,
      },
      {
        key: "time",
        header: "时间",
        className: compact
          ? "w-[144px] min-w-[144px] tabular-nums"
          : "w-[176px] min-w-[176px] tabular-nums",
        render: (row) => formatLogTime(row.startedAt, true, !compact),
      },
    ];

    if (!compact) return allColumns;
    const compactColumnKeys = new Set([
      "key",
      "model",
      "httpStatus",
      "group",
      "rate",
      "tokens",
      "cost",
      "latency",
      "time",
    ]);
    return allColumns.filter((column) => compactColumnKeys.has(column.key));
  }, [compact, keyById, stationById]);

  return (
    <div className="overflow-x-auto">
      <div className={compact ? "min-w-[1040px]" : "min-w-[1480px]"}>
        <DataTableLite
          columns={columns}
          rows={rows}
          getRowKey={(row) => row.id}
          onRowClick={(row) => onSelect(row.id)}
          headerVariant="plain"
          className="rounded-none border-0 shadow-none [&_table]:table-fixed [&_td]:align-middle [&_td]:overflow-hidden [&_td]:text-ellipsis [&_td:last-child]:overflow-visible [&_td:last-child]:text-clip"
        />
      </div>
    </div>
  );
}

export function RequestStatusCode({
  value,
  inProgress = false,
}: {
  value: number | null;
  inProgress?: boolean;
}) {
  const label = inProgress ? "处理中" : (value ?? "—");
  return (
    <span
      className={`text-xs font-semibold tabular-nums ${inProgress ? "text-info-foreground" : `font-mono ${httpStatusToneClass(value)}`}`}
      title={inProgress ? "请求仍在处理中" : value === null ? "历史记录未保存 HTTP 状态码" : `HTTP ${value}`}
    >
      {label}
    </span>
  );
}

function httpStatusToneClass(value: number | null) {
  if (value === null) return "text-muted-foreground";
  if (value >= 500) return "text-danger-foreground";
  if (value >= 400) return "text-warning-foreground";
  if (value >= 300) return "text-info-foreground";
  return "text-success-foreground";
}

export function RequestLogPagination({
  pageInfo,
  pageSize,
  onPageChange,
  onPageSizeChange,
}: RequestLogPaginationProps) {
  return (
    <div
      data-testid="request-log-pagination-surface"
      className="mt-4 flex min-h-12 flex-wrap items-center justify-between gap-3 border border-border bg-surface px-3 py-2 text-xs text-muted-foreground"
    >
      <div className="flex flex-wrap items-center gap-3">
        <span>第 {pageInfo.startIndex}-{pageInfo.endIndex} 条 / 共 {pageInfo.totalCount} 条</span>
        <label className="flex items-center gap-2">
          <span>每页</span>
          <PageSizeSelect
            ariaLabel="每页记录数"
            value={pageSize}
            options={[20, 50, 100]}
            onChange={onPageSizeChange}
          />
        </label>
      </div>

      <Pagination
        ariaLabel="使用记录分页"
        page={pageInfo.page}
        totalPages={pageInfo.totalPages}
        onPageChange={onPageChange}
      />
    </div>
  );
}

function LogMetaTag({ value }: { value: string }) {
  return (
    <span
      className="inline-flex h-5 max-w-full items-center overflow-hidden rounded-[4px] bg-info-surface px-2 text-xs font-medium text-info-foreground"
      title={value}
    >
      <span className="truncate">{value}</span>
    </span>
  );
}

function TokenUsageCell({ log }: { log: RequestLog }) {
  const hasCache = (log.cacheReadTokens ?? 0) > 0 || (log.cacheCreationTokens ?? 0) > 0;
  const inputTokens = requestInputTokenCount(log);

  return (
    <div className="grid min-h-[36px] content-center gap-1 text-xs leading-4">
      <div className="flex items-center gap-2.5 whitespace-nowrap">
        <span className="flex items-center gap-0.5 font-medium text-foreground" title="输入 Token">
          <ArrowDown className="h-3.5 w-3.5 text-success-foreground" aria-hidden="true" />
          {formatRequestTokenCount(log, inputTokens)}
        </span>
        <span className="flex items-center gap-0.5 font-medium text-foreground" title="输出 Token">
          <ArrowUp className="h-3.5 w-3.5 text-platform-image-foreground" aria-hidden="true" />
          {formatRequestTokenCount(log, log.completionTokens)}
        </span>
      </div>
      {hasCache ? (
        <div className="flex items-center gap-2 whitespace-nowrap text-info-foreground">
          <span className="flex items-center gap-1" title="缓存读取 Token">
            <Database className="h-3.5 w-3.5" aria-hidden="true" />
            {formatCompactTokenCount(log.cacheReadTokens)}
          </span>
          {(log.cacheCreationTokens ?? 0) > 0 ? (
            <span title="缓存写入 Token">写 {formatCompactTokenCount(log.cacheCreationTokens)}</span>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}

const latencyTonePalette: Record<RequestLatencyTone, { bar: string; from: string; to: string; text: string }> = {
  normal: { bar: "bg-emerald-500", from: "from-emerald-500", to: "to-emerald-500", text: "text-emerald-600" },
  notice: { bar: "bg-amber-400", from: "from-amber-400", to: "to-amber-400", text: "text-amber-600" },
  warning: { bar: "bg-orange-500", from: "from-orange-500", to: "to-orange-500", text: "text-orange-600" },
  critical: { bar: "bg-red-500", from: "from-red-500", to: "to-red-500", text: "text-red-600" },
  muted: { bar: "bg-muted-foreground/40", from: "from-muted-foreground/40", to: "to-muted-foreground/40", text: "text-muted-foreground" },
};

function LatencyCell({ log }: { log: RequestLog }) {
  const firstTone = latencyTone(log.firstTokenMs, "first_token");
  const totalTone = latencyTone(log.durationMs, "total");
  const barClass = log.firstTokenMs == null
    ? latencyTonePalette[totalTone].bar
    : `bg-gradient-to-b from-40% to-60% ${latencyTonePalette[firstTone].from} ${latencyTonePalette[totalTone].to}`;

  return (
    <div className="relative min-h-[36px] w-full min-w-0 overflow-hidden text-xs leading-4">
      <span
        className={`absolute left-0 top-1/2 h-9 w-1 -translate-y-1/2 rounded-full ${barClass}`}
        aria-hidden="true"
      />
      <div className="absolute inset-y-0 left-3 right-0 grid content-center">
        <div className="grid w-full grid-cols-[auto_minmax(0,1fr)] items-center gap-x-2 gap-y-0.5">
          {latencyBreakdown(log).map((row) => (
            <Fragment key={row.label}>
              <span className="text-left text-muted-foreground" title={row.title}>{row.label}</span>
              <span
                className={`text-left font-medium tabular-nums ${latencyTonePalette[row.tone].text}`}
                title={row.title}
              >
                {row.value}
              </span>
            </Fragment>
          ))}
        </div>
      </div>
    </div>
  );
}
