import type { ReactNode } from "react";

type PageScaffoldProps = {
  title: string;
  eyebrow: string;
  description: string;
  children?: ReactNode;
};

export function PageScaffold({
  title,
  eyebrow,
  description,
  children,
}: PageScaffoldProps) {
  return (
    <section className="mx-auto flex w-full max-w-6xl flex-col gap-4">
      <div className="flex items-end justify-between gap-4">
        <div>
          <div className="text-xs font-medium uppercase tracking-[0.18em] text-accent">
            {eyebrow}
          </div>
          <h1 className="mt-1 text-xl font-semibold">{title}</h1>
          <p className="mt-1 max-w-2xl text-sm text-muted-foreground">
            {description}
          </p>
        </div>
      </div>
      {children}
    </section>
  );
}
