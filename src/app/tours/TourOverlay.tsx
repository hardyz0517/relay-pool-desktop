import { useSyncExternalStore } from "react";
import { RefreshCw, X } from "lucide-react";
import { Button } from "@/components/ui";
import type { TourManagerApi } from "./tourTypes";

/**
 * React only observes the manager. Driver.js owns the actual overlay and
 * popover DOM, so feature pages never need to know that a tour is running.
 */
export function TourOverlay({ manager }: { manager: TourManagerApi }) {
  const snapshot = useSyncExternalStore(
    (listener) => manager.subscribe(listener),
    () => manager.getSnapshot(),
    () => manager.getSnapshot(),
  );

  const announcement = snapshot.message
    ? snapshot.message
    : snapshot.phase === "completed"
      ? "教程已完成"
      : snapshot.phase === "skipped"
        ? "教程已退出"
        : null;

  return (
    <>
      <div aria-live="polite" className="sr-only" data-tour-overlay-state={snapshot.phase}>
        {announcement}
      </div>
      {snapshot.phase === "error" && snapshot.tourId ? (
        <div
          role="alert"
          className="fixed bottom-4 left-1/2 z-[55] flex w-[min(440px,calc(100vw-32px))] -translate-x-1/2 items-start gap-3 rounded-[var(--surface-radius)] border border-danger-border bg-surface px-4 py-3 text-sm shadow-dialog"
          data-tour-error
        >
          <div className="min-w-0 flex-1">
            <div className="font-medium text-foreground">教程暂时无法继续</div>
            <div className="mt-1 break-words text-xs leading-5 text-muted-foreground">
              {snapshot.message ?? "当前步骤暂不可用。"}
            </div>
          </div>
          <div className="flex shrink-0 gap-1">
            <Button size="icon" variant="ghost" aria-label="重试教程步骤" title="重试" onClick={() => manager.retry()}>
              <RefreshCw className="h-4 w-4" />
            </Button>
            <Button size="icon" variant="ghost" aria-label="退出教程" title="退出" onClick={() => manager.close()}>
              <X className="h-4 w-4" />
            </Button>
          </div>
        </div>
      ) : null}
    </>
  );
}
