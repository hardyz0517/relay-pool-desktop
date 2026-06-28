import type { HTMLAttributes } from "react";
import { cn } from "@/lib/utils";

type CardProps = HTMLAttributes<HTMLDivElement> & {
  interactive?: boolean;
};

export function Card({ className, interactive = false, ...props }: CardProps) {
  return (
    <div
      className={cn(
        "rounded-[var(--surface-radius)] border border-border bg-white shadow-[var(--surface-shadow)]",
        interactive && "transition-shadow hover:shadow-[var(--surface-shadow-hover)]",
        className,
      )}
      {...props}
    />
  );
}
