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
    <div className={cn("overflow-hidden rounded-lg border border-border", className)}>
      <table className="w-full border-collapse bg-white text-left text-sm">
        <thead className="bg-slate-50 text-xs font-medium text-muted-foreground">
          <tr>
            {columns.map((column) => (
              <th key={column.key} className={cn("h-9 px-3", column.className)}>
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
                  "h-10 text-slate-700",
                  onRowClick && "cursor-pointer hover:bg-slate-50",
                  selectedKey === rowKey && "bg-blue-50/70",
                )}
              >
                {columns.map((column) => (
                  <td key={column.key} className={cn("px-3", column.className)}>
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
