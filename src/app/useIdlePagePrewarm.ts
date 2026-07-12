import { useEffect, useRef } from "react";
import type { AppRouteId } from "@/lib/types/navigation";

type IdleDeadlineLike = {
  didTimeout: boolean;
  timeRemaining: () => number;
};

type IdleWindow = Window & {
  requestIdleCallback?: (
    callback: (deadline: IdleDeadlineLike) => void,
    options?: { timeout: number },
  ) => number;
  cancelIdleCallback?: (handle: number) => void;
};

type SchedulingNavigator = Navigator & {
  scheduling?: { isInputPending?: () => boolean };
};

export function useIdlePagePrewarm({
  candidates,
  mountedRouteIds,
  disabled,
  onPrewarm,
}: {
  candidates: readonly AppRouteId[];
  mountedRouteIds: ReadonlySet<AppRouteId>;
  disabled: boolean;
  onPrewarm: (routeId: AppRouteId) => void;
}) {
  const onPrewarmRef = useRef(onPrewarm);
  onPrewarmRef.current = onPrewarm;

  useEffect(() => {
    if (disabled) {
      return;
    }
    const next = candidates.find((routeId) => !mountedRouteIds.has(routeId));
    if (!next) {
      return;
    }
    const routeIdToPrewarm = next;

    const idleWindow = window as IdleWindow;
    const schedulingNavigator = navigator as SchedulingNavigator;
    let idleHandle: number | null = null;
    let timeoutHandle: number | null = null;
    let disposed = false;

    const cancelScheduled = () => {
      if (idleHandle !== null) idleWindow.cancelIdleCallback?.(idleHandle);
      if (timeoutHandle !== null) window.clearTimeout(timeoutHandle);
      idleHandle = null;
      timeoutHandle = null;
    };

    function schedule(delay = 0) {
      cancelScheduled();
      if (disposed) {
        return;
      }
      if (delay > 0) {
        timeoutHandle = window.setTimeout(() => schedule(), delay);
        return;
      }
      if (idleWindow.requestIdleCallback) {
        idleHandle = idleWindow.requestIdleCallback(run, { timeout: 1_000 });
        return;
      }
      timeoutHandle = window.setTimeout(() => run(), 250);
    }

    function run(deadline?: IdleDeadlineLike) {
      idleHandle = null;
      timeoutHandle = null;
      if (disposed) {
        return;
      }
      const inputPending = schedulingNavigator.scheduling?.isInputPending?.() ?? false;
      if (inputPending || (deadline && !deadline.didTimeout && deadline.timeRemaining() < 4)) {
        schedule(250);
        return;
      }
      onPrewarmRef.current(routeIdToPrewarm);
    }

    const postponeForInput = () => schedule(500);
    window.addEventListener("pointerdown", postponeForInput, { passive: true });
    window.addEventListener("keydown", postponeForInput);
    schedule();

    return () => {
      disposed = true;
      cancelScheduled();
      window.removeEventListener("pointerdown", postponeForInput);
      window.removeEventListener("keydown", postponeForInput);
    };
  }, [candidates, disabled, mountedRouteIds]);
}
