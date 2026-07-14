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
  return <dl className={cn("overflow-hidden rounded-[var(--surface-radius)] border border-border divide-y divide-border", className)}>{children}</dl>;
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
        "grid min-h-9 grid-cols-[148px_minmax(0,1fr)] gap-3 bg-surface px-3 py-2 text-[13px]",
        className,
      )}
    >
      <dt className="min-w-0">
        <div className="truncate text-xs font-medium text-muted-foreground">{label}</div>
        {description && (
          <div className="mt-0.5 text-[11px] leading-4 text-muted-foreground">
            {description}
          </div>
        )}
      </dt>
      <dd className="min-w-0 text-foreground">{value}</dd>
    </div>
  );
}

