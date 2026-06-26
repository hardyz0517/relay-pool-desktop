import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

type PropertyListProps = {
  children: ReactNode;
  className?: string;
};

type PropertyRowProps = {
  label: string;
  description?: string;
  value: ReactNode;
  className?: string;
};

export function PropertyList({ children, className }: PropertyListProps) {
  return <dl className={cn("divide-y divide-border", className)}>{children}</dl>;
}

export function PropertyRow({
  label,
  description,
  value,
  className,
}: PropertyRowProps) {
  return (
    <div
      className={cn(
        "grid min-h-9 grid-cols-[148px_minmax(0,1fr)] gap-3 px-3 py-2 text-[13px]",
        className,
      )}
    >
      <dt className="min-w-0">
        <div className="truncate text-xs font-medium text-slate-600">{label}</div>
        {description && (
          <div className="mt-0.5 text-[11px] leading-4 text-muted-foreground">
            {description}
          </div>
        )}
      </dt>
      <dd className="min-w-0 text-slate-700">{value}</dd>
    </div>
  );
}

