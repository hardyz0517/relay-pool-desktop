import type { ReactNode } from "react";
import { X } from "lucide-react";
import { Button } from "./button";
import { cn } from "@/lib/utils";

type DialogProps = {
  open: boolean;
  title: string;
  description?: string;
  children: ReactNode;
  footer?: ReactNode;
  onClose: () => void;
  className?: string;
};

export function Dialog({
  open,
  title,
  description,
  children,
  footer,
  onClose,
  className,
}: DialogProps) {
  if (!open) {
    return null;
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-slate-900/20 p-4 backdrop-blur-[2px]">
      <div
        className={cn(
          "max-h-[calc(100vh-32px)] w-full max-w-[780px] overflow-hidden rounded-[var(--surface-radius)] border border-border bg-white shadow-[0_24px_70px_rgba(15,23,42,0.16)]",
          className,
        )}
      >
        <div className="flex items-center justify-between gap-4 border-b border-border px-5 py-4">
          <div className="min-w-0">
            <div className="truncate text-[15px] font-semibold text-slate-800">{title}</div>
            {description && (
              <div className="mt-0.5 truncate text-xs text-muted-foreground">{description}</div>
            )}
          </div>
          <Button variant="ghost" className="h-8 w-8 px-0" onClick={onClose} aria-label="关闭">
            <X className="h-4 w-4" />
          </Button>
        </div>
        <div className="max-h-[calc(100vh-180px)] overflow-auto">{children}</div>
        {footer && <div className="border-t border-border px-5 py-4">{footer}</div>}
      </div>
    </div>
  );
}
