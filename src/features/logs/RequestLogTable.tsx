import { useMemo } from "react";
import { ArrowDown, ArrowUp, Database } from "lucide-react";
import { DataTableLite, Pagination, type DataTableColumn } from "@/components/ui";
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
  reasoningEffortLabel,
  requestLatencyTone,
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
      { key: "model", header: "模型", render: (row) => row.model ?? "未识别" },
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
        className: "text-center",
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
          <select
            aria-label="每页记录数"
            value={pageSize}
            onChange={(event) => onPageSizeChange(Number(event.target.value))}
            className="h-8 rounded-[4px] border border-border bg-surface px-2 text-sm text-foreground outline-none transition-colors focus:border-ring focus:ring-2 focus:ring-ring/20"
          >
            {[20, 50, 100].map((size) => (
              <option key={size} value={size}>{size}</option>
            ))}
          </select>
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

  return (
    <div className="grid min-h-[36px] content-center gap-1 text-xs leading-4">
      <div className="flex items-center gap-2.5 whitespace-nowrap">
        <span className="flex items-center gap-0.5 font-medium text-foreground" title="输入 Token">
          <ArrowDown className="h-3.5 w-3.5 text-success-foreground" aria-hidden="true" />
          {formatRequestTokenCount(log, log.promptTokens)}
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

function LatencyCell({ log }: { log: RequestLog }) {
  const tone = requestLatencyTone(log);
  return (
    <div className="relative min-h-[36px] w-full text-xs leading-4">
      <div className="absolute left-1/2 top-1/2 grid w-max -translate-x-1/2 -translate-y-1/2 gap-0.5">
        <span
          className={`absolute right-full top-0 mr-2.5 h-9 w-1 rounded-full ${latencyToneBarClass(tone)}`}
          aria-hidden="true"
        />
        {latencyBreakdown(log).map((row) => (
          <div
            key={row.label}
            className="flex items-center gap-2 whitespace-nowrap"
            title={row.title}
          >
            <span className="text-muted-foreground">{row.label}</span>
            <span className={`font-medium ${latencyToneTextClass(row.tone)}`}>{row.value}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

function latencyToneBarClass(tone: RequestLatencyTone) {
  if (tone === "critical") return "bg-danger-foreground";
  if (tone === "warning") return "bg-warning-foreground";
  if (tone === "notice") return "bg-info-foreground";
  if (tone === "muted") return "bg-muted-foreground/40";
  return "bg-success-foreground";
}

function latencyToneTextClass(tone: RequestLatencyTone) {
  if (tone === "critical") return "text-danger-foreground";
  if (tone === "warning") return "text-warning-foreground";
  if (tone === "notice") return "text-info-foreground";
  if (tone === "muted") return "text-muted-foreground";
  return "text-success-foreground";
}
