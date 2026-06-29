import type { ButtonHTMLAttributes } from "react";
import { cn } from "@/lib/utils";

export type ButtonVariant = "primary" | "secondary" | "ghost" | "outline" | "danger";
export type ButtonSize = "sm" | "md" | "icon";

type ButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: ButtonVariant;
  size?: ButtonSize;
};

const sizeClassName: Record<ButtonSize, string> = {
  sm: "h-7 rounded-[7px] px-2 text-xs",
  md: "h-8 rounded-[var(--surface-radius)] px-3 text-[13px]",
  icon: "h-8 w-8 rounded-[var(--surface-radius)] px-0",
};

export function Button({
  className,
  variant = "primary",
  size = "md",
  type = "button",
  ...props
}: ButtonProps) {
  return (
    <button
      type={type}
      className={cn(
        "inline-flex cursor-pointer items-center justify-center gap-2 font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[hsl(var(--accent)/0.35)] disabled:pointer-events-none disabled:cursor-default disabled:opacity-50",
        sizeClassName[size],
        variant === "primary" &&
          "bg-[hsl(var(--accent))] text-white shadow-[0_1px_2px_rgba(10,132,255,0.22)] hover:bg-[#0077ed]",
        variant === "secondary" &&
          "border border-border bg-white text-slate-700 hover:bg-slate-50",
        variant === "ghost" &&
          "text-slate-600 hover:bg-slate-100 hover:text-slate-900",
        variant === "outline" &&
          "border border-border bg-white text-slate-700 hover:bg-slate-50",
        variant === "danger" &&
          "border border-rose-200 bg-white text-rose-700 hover:bg-rose-50",
        className,
      )}
      {...props}
    />
  );
}
