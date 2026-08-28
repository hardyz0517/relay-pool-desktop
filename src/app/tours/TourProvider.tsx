import { useEffect, useRef, type ReactNode } from "react";
import { useToast } from "@/components/ui";
import { TourOverlay } from "./TourOverlay";
import { hasBlockingBusinessModal } from "./tourModalGuard";
import type { TourManagerApi } from "./tourTypes";

export function TourProvider({ manager, children }: { manager: TourManagerApi; children: ReactNode }) {
  const toast = useToast();
  const disposeTimerRef = useRef<{ manager: TourManagerApi; timerId: number } | null>(null);

  useEffect(() => {
    let previousMessage: string | null = null;
    return manager.subscribe((snapshot) => {
      if (snapshot.message && snapshot.message !== previousMessage) {
        previousMessage = snapshot.message;
        if (snapshot.phase === "error" || snapshot.phase === "idle") {
          toast.error("教程暂时无法继续", snapshot.message);
        } else if (snapshot.phase === "completed") {
          toast.info("教程已完成", snapshot.message);
        } else if (snapshot.phase === "skipped" && snapshot.message.includes("未能持久化")) {
          toast.info("教程已退出", snapshot.message);
        }
      }
      if (!snapshot.message) previousMessage = null;
    });
  }, [manager, toast]);

  useEffect(() => {
    let disposed = false;
    const clearScheduledDisposeFor = (candidate: TourManagerApi) => {
      if (disposeTimerRef.current?.manager === candidate) {
        window.clearTimeout(disposeTimerRef.current.timerId);
        disposeTimerRef.current = null;
      }
    };
    // pagehide is the eager path; the delayed cleanup path exists only to
    // tolerate React StrictMode's effect probe without disposing a live
    // manager between the probe cleanup and re-mount.
    const dispose = () => {
      if (disposed) return;
      disposed = true;
      clearScheduledDisposeFor(manager);
      manager.dispose();
    };

    // StrictMode immediately re-runs the effect with the same manager, so only
    // that exact pending disposal is cancelled. A real A -> B prop replacement
    // must leave A's captured cleanup intact.
    clearScheduledDisposeFor(manager);
    window.addEventListener("pagehide", dispose);
    return () => {
      window.removeEventListener("pagehide", dispose);
      if (disposed) return;
      clearScheduledDisposeFor(manager);
      const pending = {
        manager,
        timerId: 0,
      };
      pending.timerId = window.setTimeout(() => {
          if (disposeTimerRef.current === pending) disposeTimerRef.current = null;
          dispose();
        }, 0);
      disposeTimerRef.current = pending;
    };
  }, [manager]);

  useEffect(() => {
    if (typeof document === "undefined" || typeof MutationObserver === "undefined") return;

    const observer = new MutationObserver(() => {
      if (isActiveTour(manager) && hasBlockingBusinessModal()) {
        // A business modal or transient editor may be opened by an external
        // event while the tour is active. End the tour before its overlay can
        // compete for focus or z-index with that surface.
        manager.close();
      }
    });
    observer.observe(document.body, {
      subtree: true,
      childList: true,
      attributes: true,
      attributeFilter: ["data-tour-blocking", "data-page-transition-kind", "data-page-transition-state", "role"],
    });
    return () => observer.disconnect();
  }, [manager]);

  useEffect(() => {
    const closeActiveTour = () => {
      if (isActiveTour(manager)) manager.close();
    };
    const closeWhenHidden = () => {
      if (document.visibilityState === "hidden") closeActiveTour();
    };

    window.addEventListener("blur", closeActiveTour);
    document.addEventListener("visibilitychange", closeWhenHidden);
    return () => {
      window.removeEventListener("blur", closeActiveTour);
      document.removeEventListener("visibilitychange", closeWhenHidden);
    };
  }, [manager]);

  return (
    <>
      {children}
      <TourOverlay manager={manager} />
    </>
  );
}

function isActiveTour(manager: TourManagerApi): boolean {
  const phase = manager.getSnapshot().phase;
  return phase === "preparing" ||
    phase === "waiting-target" ||
    phase === "running";
}
