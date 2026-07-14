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

const buttonVariants: Record<ButtonVariant, string> = {
  primary: "bg-primary-solid text-primary-foreground shadow-surface hover:bg-primary-solid/90",
  secondary: "border border-border bg-surface text-foreground hover:bg-hover",
  ghost: "text-muted-foreground hover:bg-hover hover:text-foreground",
  outline: "border border-border bg-surface text-foreground hover:bg-hover",
  danger: "border border-danger-border bg-danger-surface text-danger-foreground hover:bg-danger-surface/80",
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
        "inline-flex cursor-pointer items-center justify-center gap-2 font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/30 disabled:pointer-events-none disabled:cursor-default disabled:opacity-50",
        sizeClassName[size],
        buttonVariants[variant],
        className,
      )}
      {...props}
    />
  );
}
