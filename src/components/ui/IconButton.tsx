import type { ButtonHTMLAttributes, ReactNode } from "react";
import { Button } from "./button";

type IconButtonProps = Omit<ButtonHTMLAttributes<HTMLButtonElement>, "children"> & {
  label: string;
  children: ReactNode;
  variant?: "primary" | "secondary" | "ghost" | "outline" | "danger";
};

export function IconButton({
  label,
  title,
  variant = "ghost",
  children,
  ...props
}: IconButtonProps) {
  return (
    <Button
      {...props}
      variant={variant}
      size="icon"
      title={title ?? label}
      aria-label={label}
    >
      {children}
    </Button>
  );
}
