import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

type WorkspaceLayoutProps = {
  children: ReactNode;
  columns?: string;
  className?: string;
};

export function WorkspaceLayout({
  children,
  columns = "",
  className,
}: WorkspaceLayoutProps) {
  return (
    <div className={cn("grid min-h-0 flex-1 gap-[var(--shell-page-gap)]", columns, className)}>
      {children}
    </div>
  );
}
