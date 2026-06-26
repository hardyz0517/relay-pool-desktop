import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

type InspectorPanelProps = {
  title?: string;
  description?: string;
  actions?: ReactNode;
  children: ReactNode;
  className?: string;
};

export function InspectorPanel({
  title,
  description,
  actions,
  children,
  className,
}: InspectorPanelProps) {
  return (
    <aside
      className={cn(
        "min-w-0 overflow-hidden rounded-md border border-border bg-white",
        className,
      )}
    >
      {(title || description || actions) && (
        <div className="flex min-h-10 items-center justify-between gap-3 border-b border-border px-3 py-2">
          <div className="min-w-0">
            {title && (
              <div className="truncate text-[13px] font-semibold text-slate-800">
                {title}
              </div>
            )}
            {description && (
              <div className="mt-0.5 truncate text-xs text-muted-foreground">
                {description}
              </div>
            )}
          </div>
          {actions}
        </div>
      )}
      {children}
    </aside>
  );
}
