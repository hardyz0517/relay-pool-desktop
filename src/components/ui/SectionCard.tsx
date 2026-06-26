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
        "rounded-2xl border border-white/70 bg-white/92 shadow-[0_12px_30px_rgba(33,79,88,0.07)]",
        className,
      )}
    >
      {(title || description || action) && (
        <header className="flex min-h-11 items-center justify-between gap-3 border-b border-slate-200/70 px-4 py-2.5">
          <div className="min-w-0">
            {title && (
              <h2 className="truncate text-[13px] font-semibold text-slate-800">
                {title}
              </h2>
            )}
            {description && (
              <p className="mt-0.5 truncate text-xs text-muted-foreground">
                {description}
              </p>
            )}
          </div>
          {action}
        </header>
      )}
      <div className={cn("p-4", contentClassName)}>{children}</div>
    </section>
  );
}
