import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

type PageScaffoldProps = {
  title: string;
  description: string;
  actions?: ReactNode;
  width?: "full" | "settings";
  children?: ReactNode;
};

export function PageScaffold({
  title,
  description,
  actions,
  width = "full",
  children,
}: PageScaffoldProps) {
  return (
    <section
      className={cn(
        width === "settings"
          ? "flex w-full max-w-[1180px] flex-col gap-[var(--shell-page-gap)]"
          : "flex min-h-full w-full flex-col gap-[var(--shell-page-gap)]",
      )}
    >
      <div className="flex min-h-[44px] items-center justify-between gap-4">
        <div className="min-w-0">
          <h1 className="text-[17px] font-semibold leading-6 text-slate-800">
            {title}
          </h1>
          <p className="mt-0.5 max-w-3xl truncate text-xs text-muted-foreground">
            {description}
          </p>
        </div>
        {actions && <div className="flex shrink-0 items-center gap-2">{actions}</div>}
      </div>
      {children}
    </section>
  );
}
