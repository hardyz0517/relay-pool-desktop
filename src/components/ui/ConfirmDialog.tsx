import { AlertTriangle } from "lucide-react";
import { Button } from "./button";

type ConfirmDialogProps = {
  open: boolean;
  title: string;
  description: string;
  confirmLabel?: string;
  cancelLabel?: string;
  confirming?: boolean;
  onCancel: () => void;
  onConfirm: () => void;
};

export function ConfirmDialog({
  open,
  title,
  description,
  confirmLabel = "确定",
  cancelLabel = "取消",
  confirming = false,
  onCancel,
  onConfirm,
}: ConfirmDialogProps) {
  if (!open) {
    return null;
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-scrim/45 p-4 backdrop-blur-[1px]">
      <div className="w-full max-w-sm rounded-[var(--surface-radius)] border border-border bg-surface px-5 py-5 shadow-dialog">
        <div className="flex items-start gap-3">
          <div className="mt-0.5 flex h-5 w-5 shrink-0 items-center justify-center text-danger-foreground">
            <AlertTriangle className="h-5 w-5" />
          </div>
          <div className="min-w-0">
            <div className="text-base font-semibold text-foreground">{title}</div>
            <div className="mt-3 text-sm leading-6 text-muted-foreground">{description}</div>
          </div>
        </div>
        <div className="mt-5 flex justify-end gap-2">
          <Button variant="outline" disabled={confirming} onClick={onCancel}>
            {cancelLabel}
          </Button>
          <Button
            disabled={confirming}
            variant="danger"
            onClick={onConfirm}
          >
            {confirming ? "处理中" : confirmLabel}
          </Button>
        </div>
      </div>
    </div>
  );
}
