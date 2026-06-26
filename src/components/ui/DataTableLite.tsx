import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

export type DataTableColumn<T> = {
  key: string;
  header: string;
  className?: string;
  render: (row: T) => ReactNode;
};

type DataTableLiteProps<T> = {
  columns: DataTableColumn<T>[];
  rows: T[];
  getRowKey: (row: T) => string;
  selectedKey?: string;
  onRowClick?: (row: T) => void;
  className?: string;
};

export function DataTableLite<T>({
  columns,
  rows,
  getRowKey,
  selectedKey,
  onRowClick,
  className,
}: DataTableLiteProps<T>) {
  return (
    <div className={cn("overflow-auto rounded-2xl border border-white/70 bg-white/95 shadow-[0_12px_30px_rgba(33,79,88,0.06)]", className)}>
      <table className="w-full border-collapse bg-white text-left text-[13px]">
        <thead className="bg-teal-50/70 text-[11px] font-medium uppercase tracking-wide text-slate-500">
          <tr>
            {columns.map((column) => (
              <th key={column.key} className={cn("h-8 whitespace-nowrap px-2.5", column.className)}>
                {column.header}
              </th>
            ))}
          </tr>
        </thead>
        <tbody className="divide-y divide-border">
          {rows.map((row) => {
            const rowKey = getRowKey(row);
            return (
              <tr
                key={rowKey}
                onClick={() => onRowClick?.(row)}
                className={cn(
                  "h-9 text-slate-700",
                  onRowClick && "cursor-pointer hover:bg-teal-50/55",
                  selectedKey === rowKey && "bg-teal-50 text-slate-800",
                )}
              >
                {columns.map((column) => (
                  <td key={column.key} className={cn("whitespace-nowrap px-2.5", column.className)}>
                    {column.render(row)}
                  </td>
                ))}
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
