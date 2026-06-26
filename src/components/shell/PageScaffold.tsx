import type { ReactNode } from "react";

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
      className={
        width === "settings"
          ? "flex w-full max-w-[1180px] flex-col gap-3"
          : "flex min-h-full w-full flex-col gap-3"
      }
    >
      <div className="flex min-h-10 items-center justify-between gap-4">
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
