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
        <header className="flex min-h-[38px] flex-wrap items-start justify-between gap-3">
          <div className="min-w-0">
            {title && (
              <h2 className="text-sm font-semibold text-foreground">{title}</h2>
            )}
            {description && (
              <p className="mt-0.5 break-words text-sm text-muted-foreground">{description}</p>
            )}
          </div>
          {action}
        </header>
      )}
      <div
        className={cn(
          "overflow-hidden rounded-[var(--surface-radius)] border border-border bg-surface p-4 shadow-surface",
          contentClassName,
        )}
      >
        {children}
      </div>
    </section>
  );
}
