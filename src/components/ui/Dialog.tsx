import { useEffect, type ReactNode } from "react";
import { createPortal } from "react-dom";
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

let openDialogCount = 0;
let previousBodyOverflow = "";

export function Dialog({
  open,
  title,
  description,
  children,
  footer,
  onClose,
  className,
}: DialogProps) {
  useEffect(() => {
    if (!open) {
      return;
    }

    if (openDialogCount === 0) {
      previousBodyOverflow = document.body.style.overflow;
      document.body.style.overflow = "hidden";
    }
    openDialogCount += 1;

    return () => {
      openDialogCount = Math.max(0, openDialogCount - 1);
      if (openDialogCount === 0) {
        document.body.style.overflow = previousBodyOverflow;
      }
    };
  }, [open]);

  if (!open) {
    return null;
  }

  return createPortal(
    <div
      role="dialog"
      aria-modal="true"
      className="fixed inset-0 z-50 flex items-center justify-center bg-white/35 p-4 backdrop-blur-[1px]"
    >
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
    </div>,
    document.body,
  );
}
