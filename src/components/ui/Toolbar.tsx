import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

type ToolbarProps = {
  children: ReactNode;
  className?: string;
};

export function Toolbar({ children, className }: ToolbarProps) {
  return (
    <div
      className={cn(
        "flex min-h-[44px] items-center justify-between gap-2 border-b border-border bg-white px-3 py-2",
        className,
      )}
    >
      {children}
    </div>
  );
}
