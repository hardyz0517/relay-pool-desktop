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
  headerVariant?: "default" | "plain";
};

export function DataTableLite<T>({
  columns,
  rows,
  getRowKey,
  selectedKey,
  onRowClick,
  className,
  headerVariant = "default",
}: DataTableLiteProps<T>) {
  return (
    <div className={cn("overflow-auto rounded-[var(--surface-radius)] border border-border bg-surface shadow-surface", className)}>
      <table className="w-full border-collapse bg-surface text-left text-[13px]">
        <thead
          className={cn(
            headerVariant === "plain"
              ? "border-b border-border bg-surface text-xs font-medium text-muted-foreground"
              : "bg-surface-subtle text-[11px] font-medium uppercase tracking-wide text-muted-foreground",
          )}
        >
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
                  "h-9 text-foreground",
                  onRowClick && "cursor-pointer hover:bg-hover",
                  selectedKey === rowKey && "bg-selected text-selected-foreground",
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
