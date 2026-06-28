import type { ButtonHTMLAttributes } from "react";
import { cn } from "@/lib/utils";

type ButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: "primary" | "secondary" | "ghost" | "outline" | "danger";
};

export function Button({
  className,
  variant = "primary",
  type = "button",
  ...props
}: ButtonProps) {
  return (
    <button
      type={type}
      className={cn(
        "inline-flex h-8 cursor-pointer items-center justify-center gap-2 rounded-[var(--surface-radius)] px-3 text-[13px] font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent disabled:pointer-events-none disabled:cursor-default disabled:opacity-50",
        variant === "primary" &&
          "bg-teal-600 text-white shadow-[0_1px_2px_rgba(15,118,110,0.18)] hover:bg-teal-700",
        variant === "secondary" &&
          "border border-teal-100 bg-teal-50 text-teal-700 hover:bg-teal-100",
        variant === "ghost" &&
          "text-muted-foreground hover:bg-teal-50 hover:text-teal-700",
        variant === "outline" &&
          "border border-border bg-white/90 text-slate-700 hover:bg-teal-50",
        variant === "danger" &&
          "border border-rose-200 bg-white text-rose-700 hover:bg-rose-50",
        className,
      )}
      {...props}
    />
  );
}
