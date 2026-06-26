import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

type WorkspaceLayoutProps = {
  children: ReactNode;
  columns?: string;
  className?: string;
};

export function WorkspaceLayout({
  children,
  columns = "xl:grid-cols-[320px_minmax(0,1fr)]",
  className,
}: WorkspaceLayoutProps) {
  return (
    <div className={cn("grid min-h-0 flex-1 gap-3", columns, className)}>
      {children}
    </div>
  );
}
