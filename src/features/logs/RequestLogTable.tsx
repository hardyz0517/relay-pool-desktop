import { useMemo } from "react";
import { DataTableLite, StatusBadge, type DataTableColumn } from "@/components/ui";
import type { RequestLog } from "@/lib/types/proxy";
import type { KeyPoolItem } from "@/lib/types/stationKeys";
import {
  billingModeLabel,
  formatKeyName,
  formatLogTime,
  formatRequestCost,
  latencyBreakdown,
  pricingStatusLabel,
  pricingStatusTone,
  reasoningEffortLabel,
  tokenBreakdown,
} from "./requestLogViewModels";

const statusTone = {
  success: "healthy",
  failed: "error",
  fallback: "warning",
  interrupted: "error",
} as const;

const statusLabel = {
  success: "成功",
  failed: "失败",
  fallback: "兜底",
  interrupted: "已中断",
} as const;

type RequestLogTableProps = {
  rows: RequestLog[];
  keyById: Map<string, KeyPoolItem>;
  selectedId: string | null;
  onSelect: (id: string) => void;
};

export function RequestLogTable({ rows, keyById, selectedId, onSelect }: RequestLogTableProps) {
  const columns = useMemo<DataTableColumn<RequestLog>[]>(() => [
    { key: "key", header: "密钥", className: "w-44", render: (row) => formatKeyName(row, keyById) },
    { key: "model", header: "模型", className: "w-36", render: (row) => row.model ?? "未识别" },
    { key: "reasoning", header: "推理强度", className: "w-24", render: (row) => reasoningEffortLabel(row.reasoningEffort) },
    { key: "endpoint", header: "端点", className: "w-40", render: (row) => `${row.method} ${row.path}` },
    { key: "group", header: "分组", className: "w-32", render: (row) => row.groupBindingId ?? "未分组" },
    { key: "type", header: "类型", className: "w-20", render: (row) => row.stream ? "流式" : "非流式" },
    { key: "billing", header: "计费模式", className: "w-24", render: (row) => billingModeLabel(row.billingMode) },
    { key: "tokens", header: "Token", className: "w-36", render: (row) => <Breakdown rows={tokenBreakdown(row)} /> },
    {
      key: "cost",
      header: "费用",
      className: "w-32",
      render: (row) => (
        <div className="grid gap-1">
          <span className="font-medium text-emerald-700">{formatRequestCost(row)}</span>
          <StatusBadge className="h-5 w-fit px-1.5" tone={pricingStatusTone(row.costStatus)}>
            {pricingStatusLabel(row.costStatus)}
          </StatusBadge>
        </div>
      ),
    },
    { key: "latency", header: "延迟", className: "w-28", render: (row) => <Breakdown rows={latencyBreakdown(row)} /> },
    {
      key: "status",
      header: "状态",
      className: "w-20",
      render: (row) => (
        <StatusBadge tone={statusTone[row.status as keyof typeof statusTone] ?? "info"}>
          {statusLabel[row.status as keyof typeof statusLabel] ?? row.status}
        </StatusBadge>
      ),
    },
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
          className="rounded-none border-0 shadow-none"
        />
      </div>
    </div>
  );
}

function Breakdown({ rows }: { rows: Array<{ label: string; value: string }> }) {
  return (
    <div className="grid gap-0.5 text-xs leading-4">
      {rows.map((row) => (
        <div key={row.label} className="flex items-center justify-between gap-2 whitespace-nowrap">
          <span className="text-slate-500">{row.label}</span>
          <span className="font-medium text-slate-700">{row.value}</span>
        </div>
      ))}
    </div>
  );
}
