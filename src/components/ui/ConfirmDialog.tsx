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
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-white/30 p-4 backdrop-blur-[1px]">
      <div className="w-full max-w-sm rounded-[var(--surface-radius)] border border-border bg-white px-5 py-5 shadow-[0_24px_70px_rgba(15,23,42,0.18)]">
        <div className="flex items-start gap-3">
          <div className="mt-0.5 flex h-5 w-5 shrink-0 items-center justify-center text-rose-500">
            <AlertTriangle className="h-5 w-5" />
          </div>
          <div className="min-w-0">
            <div className="text-base font-semibold text-slate-900">{title}</div>
            <div className="mt-3 text-sm leading-6 text-slate-500">{description}</div>
          </div>
        </div>
        <div className="mt-5 flex justify-end gap-2">
          <Button variant="outline" disabled={confirming} onClick={onCancel}>
            {cancelLabel}
          </Button>
          <Button
            className="border-rose-500 bg-rose-500 text-white shadow-[0_1px_2px_rgba(244,63,94,0.22)] hover:bg-rose-600"
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
