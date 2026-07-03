import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

type PageScaffoldProps = {
  title: string;
  description: string;
  actions?: ReactNode;
  status?: ReactNode;
  backAction?: ReactNode;
  width?: "full" | "settings";
  children?: ReactNode;
};

export function PageScaffold({
  title,
  description,
  actions,
  status,
  backAction,
  width = "full",
  children,
}: PageScaffoldProps) {
  return (
    <section
      className={cn(
        width === "settings"
          ? "flex min-w-0 w-full flex-col gap-[var(--shell-page-gap)]"
          : "flex min-h-full min-w-0 w-full flex-col gap-[var(--shell-page-gap)]",
      )}
    >
      <div className="flex min-h-[44px] flex-wrap items-center justify-between gap-3">
        <div className="flex min-w-0 items-center gap-3">
          {backAction}
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <h1 className="truncate text-[18px] font-semibold leading-6 text-slate-900">
                {title}
              </h1>
              {status}
            </div>
            <p className="mt-0.5 max-w-3xl truncate text-xs text-muted-foreground">
              {description}
            </p>
          </div>
        </div>
        {actions && <div className="flex shrink-0 items-center gap-2">{actions}</div>}
      </div>
      {children}
    </section>
  );
}
