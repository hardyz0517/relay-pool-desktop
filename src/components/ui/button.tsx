import type { ButtonHTMLAttributes } from "react";
import { cn } from "@/lib/utils";

type ButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: "default" | "ghost" | "outline";
};

export function Button({
  className,
  variant = "default",
  type = "button",
  ...props
}: ButtonProps) {
  return (
    <button
      type={type}
      className={cn(
        "inline-flex h-8 items-center justify-center gap-2 rounded-md px-3 text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent disabled:pointer-events-none disabled:opacity-50",
        variant === "default" &&
          "bg-accent text-accent-foreground hover:bg-accent/90",
        variant === "ghost" &&
          "text-muted-foreground hover:bg-muted hover:text-slate-700",
        variant === "outline" &&
          "border border-border bg-white text-slate-700 hover:bg-muted",
        className,
      )}
      {...props}
    />
  );
}
