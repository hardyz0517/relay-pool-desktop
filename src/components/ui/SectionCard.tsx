import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

type SectionCardProps = {
  title?: string;
  description?: string;
  action?: ReactNode;
  children: ReactNode;
  className?: string;
  contentClassName?: string;
};

export function SectionCard({
  title,
  description,
  action,
  children,
  className,
  contentClassName,
}: SectionCardProps) {
  return (
    <section
      className={cn(
        "grid gap-2",
        className,
      )}
    >
      {(title || description || action) && (
        <header className="flex min-h-[38px] items-start justify-between gap-3">
          <div className="min-w-0">
            {title && (
              <h2 className="truncate text-sm font-semibold text-slate-950">{title}</h2>
            )}
            {description && (
              <p className="mt-0.5 truncate text-sm text-slate-600">{description}</p>
            )}
          </div>
          {action}
        </header>
      )}
      <div
        className={cn(
          "overflow-hidden rounded-[var(--surface-radius)] border border-border bg-white p-4 shadow-[var(--surface-shadow)]",
          contentClassName,
        )}
      >
        {children}
      </div>
    </section>
  );
}
