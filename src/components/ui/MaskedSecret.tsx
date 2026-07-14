import { Copy, Eye, EyeOff } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { useToast } from "@/components/ui/ToastProvider";

type MaskedSecretProps = {
  value: string;
  present?: boolean;
  revealLabel?: string;
  onReveal?: () => Promise<string>;
  onCopy?: (value: string) => Promise<void>;
};

export function MaskedSecret({
  value,
  present = true,
  revealLabel = "查看",
  onReveal,
  onCopy,
}: MaskedSecretProps) {
  const toast = useToast();
  const [revealed, setRevealed] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const displayValue = revealed ?? (present ? value : "未设置");

  async function handleReveal() {
    if (!onReveal) {
      return;
    }
    setBusy(true);
    try {
      if (revealed) {
        setRevealed(null);
      } else {
        setRevealed(await onReveal());
      }
    } finally {
      setBusy(false);
    }
  }

  async function handleCopy() {
    const copyValue = revealed ?? value;
    try {
      if (onCopy) {
        await onCopy(copyValue);
      } else {
        await navigator.clipboard.writeText(copyValue);
      }
      toast.success("已复制");
    } catch (copyError) {
      toast.error("复制失败", copyError instanceof Error ? copyError.message : "请手动复制。");
    }
  }

  return (
    <span className="inline-flex min-w-0 items-center gap-1">
      <code className="min-w-0 truncate rounded border border-border bg-surface-inset px-1.5 py-0.5 text-xs text-foreground">
        {displayValue}
      </code>
      {onReveal ? (
        <Button
          className="h-6 px-1.5 text-xs"
          disabled={busy || !present}
          type="button"
          variant="ghost"
          onClick={handleReveal}
        >
          {revealed ? <EyeOff className="h-3.5 w-3.5" /> : <Eye className="h-3.5 w-3.5" />}
          <span className="sr-only">{revealLabel}</span>
        </Button>
      ) : null}
      {onCopy ? (
        <Button
          className="h-6 px-1.5 text-xs"
          disabled={!present}
          type="button"
          variant="ghost"
          onClick={() => void handleCopy()}
        >
          <Copy className="h-3.5 w-3.5" />
          <span className="sr-only">复制</span>
        </Button>
      ) : null}
    </span>
  );
}
