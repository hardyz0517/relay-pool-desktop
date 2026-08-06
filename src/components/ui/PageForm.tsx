import type { FormHTMLAttributes, ReactNode } from "react";
import { cn } from "@/lib/utils";

type PageFormProps = FormHTMLAttributes<HTMLFormElement> & {
  children: ReactNode;
  footer: ReactNode;
};

export function PageForm({ children, footer, className, ...props }: PageFormProps) {
  return (
    <form className={cn("flex h-full min-h-0 flex-1 flex-col", className)} {...props}>
      <div className="min-h-0 flex-1 overflow-y-auto pb-[var(--shell-page-gap)] [scrollbar-width:none] [&::-webkit-scrollbar]:hidden">
        <div className="grid content-start gap-[var(--shell-page-gap)]">{children}</div>
      </div>
      <div className="sticky bottom-0 z-10 -mx-[var(--shell-page-gap)] -mb-[var(--shell-page-gap)] flex shrink-0 flex-wrap items-center justify-end gap-2 border-t border-border bg-surface/95 px-[calc(var(--shell-page-gap)+1rem)] py-2 backdrop-blur">
        {footer}
      </div>
    </form>
  );
}
