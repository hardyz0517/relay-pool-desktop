import { useMemo } from "react";
import { ArrowDown, ArrowUp, ChevronLeft, ChevronRight, Database } from "lucide-react";
import { DataTableLite, type DataTableColumn } from "@/components/ui";
import type { RequestLog } from "@/lib/types/proxy";
import type { KeyPoolItem } from "@/lib/types/stationKeys";
import {
  billingModeLabel,
  formatCompactTokenCount,
  formatGroupName,
  formatKeyName,
  formatLogTime,
  formatRequestCost,
  formatRequestTokenCount,
  latencyBreakdown,
  reasoningEffortLabel,
} from "./requestLogViewModels";

type RequestLogTableProps = {
  rows: RequestLog[];
  keyById: Map<string, KeyPoolItem>;
  selectedId: string | null;
  onSelect: (id: string) => void;
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

export function RequestLogTable({ rows, keyById, selectedId, onSelect }: RequestLogTableProps) {
  const columns = useMemo<DataTableColumn<RequestLog>[]>(() => [
    { key: "key", header: "密钥", className: "w-44", render: (row) => formatKeyName(row, keyById) },
    { key: "model", header: "模型", className: "w-36", render: (row) => row.model ?? "未识别" },
    { key: "reasoning", header: "推理强度", className: "w-24", render: (row) => reasoningEffortLabel(row.reasoningEffort) },
    { key: "endpoint", header: "端点", className: "w-40", render: (row) => row.path },
    { key: "group", header: "分组", className: "w-32", render: (row) => <LogMetaTag value={formatGroupName(row, keyById)} /> },
    { key: "type", header: "类型", className: "w-20", render: (row) => <LogMetaTag value={row.stream ? "流式" : "同步"} /> },
    { key: "billing", header: "计费模式", className: "w-24", render: (row) => <LogMetaTag value={billingModeLabel(row.billingMode)} /> },
    { key: "tokens", header: "Token", className: "w-36", render: (row) => <TokenUsageCell log={row} /> },
    {
      key: "cost",
      header: "费用",
      className: "w-32",
      render: (row) => <span className="font-medium text-success-foreground">{formatRequestCost(row)}</span>,
    },
    { key: "latency", header: "延迟", className: "w-28", render: (row) => <LatencyCell log={row} /> },
    { key: "time", header: "时间", className: "w-44", render: (row) => formatLogTime(row.startedAt, true) },
  ], [keyById]);

  return (
    <div className="overflow-x-auto">
      <div className="min-w-[1320px]">
        <DataTableLite
          columns={columns}
          rows={rows}
          getRowKey={(row) => row.id}
          selectedKey={selectedId ?? undefined}
          onRowClick={(row) => onSelect(row.id)}
          headerVariant="plain"
          className="rounded-none border-0 shadow-none"
        />
      </div>
    </div>
  );
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

      <div className="flex items-center" aria-label="使用记录分页">
        <button
          type="button"
          aria-label="上一页"
          title="上一页"
          disabled={pageInfo.page <= 1}
          onClick={() => onPageChange(pageInfo.page - 1)}
          className="inline-flex h-8 w-8 items-center justify-center rounded-l-[4px] border border-border bg-surface text-muted-foreground transition-colors hover:bg-surface-subtle hover:text-foreground focus-visible:z-10 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/30 disabled:cursor-not-allowed disabled:text-muted-foreground/45"
        >
          <ChevronLeft className="h-4 w-4" aria-hidden="true" />
        </button>
        <span className="inline-flex h-8 min-w-9 items-center justify-center border-y border-primary bg-info-surface px-2 font-medium text-info-foreground">
          {pageInfo.page}
        </span>
        <button
          type="button"
          aria-label="下一页"
          title="下一页"
          disabled={pageInfo.page >= pageInfo.totalPages}
          onClick={() => onPageChange(pageInfo.page + 1)}
          className="inline-flex h-8 w-8 items-center justify-center rounded-r-[4px] border border-border bg-surface text-muted-foreground transition-colors hover:bg-surface-subtle hover:text-foreground focus-visible:z-10 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/30 disabled:cursor-not-allowed disabled:text-muted-foreground/45"
        >
          <ChevronRight className="h-4 w-4" aria-hidden="true" />
        </button>
      </div>
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
  return (
    <div className="flex min-h-[36px] items-center gap-2.5 text-xs leading-4">
      <span className="h-9 w-1 shrink-0 rounded-full bg-success-foreground" aria-hidden="true" />
      <div className="grid min-w-[74px] gap-0.5">
        {latencyBreakdown(log).map((row) => (
          <div key={row.label} className="flex items-center justify-between gap-2 whitespace-nowrap">
            <span className="text-muted-foreground">{row.label}</span>
            <span className="font-medium text-success-foreground">{row.value}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
