import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

type PageScaffoldProps = {
  title: string;
  description?: string;
  actions?: ReactNode;
  status?: ReactNode;
  backAction?: ReactNode;
  width?: "full" | "settings";
  stickyHeader?: boolean;
  children?: ReactNode;
};

export function PageScaffold({
  title,
  description,
  actions,
  status,
  backAction,
  width = "full",
  stickyHeader = false,
  children,
}: PageScaffoldProps) {
  return (
    <section
      className={cn(
        width === "settings"
          ? "relative flex min-w-0 w-full max-w-none flex-col gap-[var(--shell-page-gap)]"
          : "relative flex min-h-full min-w-0 w-full flex-col gap-[var(--shell-page-gap)]",
      )}
    >
      <div
        className={cn(
          "flex min-h-[44px] flex-wrap items-center justify-between gap-3",
          stickyHeader &&
            "sticky top-0 z-20 -mx-[var(--shell-page-gap)] -mt-[var(--shell-page-gap)] border-b border-border bg-background/95 px-[var(--shell-page-gap)] py-3 backdrop-blur",
        )}
      >
        <div className="flex min-w-0 items-center gap-3">
          {backAction}
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <h1 className="truncate text-[18px] font-semibold leading-6 text-slate-900">
                {title}
              </h1>
              {status}
            </div>
            {description && (
              <p className="mt-0.5 max-w-3xl truncate text-xs text-muted-foreground">
                {description}
              </p>
            )}
          </div>
        </div>
        {actions && <div className="flex shrink-0 items-center gap-2">{actions}</div>}
      </div>
      {children}
    </section>
  );
}
