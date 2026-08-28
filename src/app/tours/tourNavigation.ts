import type { AppPageId } from "@/lib/types/navigation";
import type {
  NavigationReadySnapshot,
  TourNavigationCurrent,
  TourNavigationPort,
  TourNavigationRequest,
} from "./tourTypes";

export class TourNavigationCancelledError extends Error {
  readonly name = "TourNavigationCancelledError";

  constructor() {
    super("Tour navigation wait was cancelled");
  }
}

type Waiter = {
  request: TourNavigationRequest;
  resolve: (snapshot: NavigationReadySnapshot) => void;
  reject: (error: unknown) => void;
  cleanup: () => void;
};

export type TourNavigationController = TourNavigationPort & {
  notifyReady(snapshot: NavigationReadySnapshot): void;
  dispose(): void;
};

export function createTourNavigationPort({
  navigate,
  getCurrent,
}: {
  navigate: (routeId: AppPageId) => void;
  getCurrent: () => TourNavigationCurrent;
}): TourNavigationController {
  const waiters = new Map<number, Waiter>();
  let disposed = false;
  let activeRequestToken: number | null = null;

  const rejectWaiter = (waiter: Waiter) => {
    if (waiters.get(waiter.request.requestToken) === waiter) {
      waiters.delete(waiter.request.requestToken);
    }
    waiter.cleanup();
    waiter.reject(new TourNavigationCancelledError());
  };

  const tryResolve = (snapshot: NavigationReadySnapshot) => {
    for (const waiter of [...waiters.values()]) {
      if (waiter.request.signal.aborted) {
        rejectWaiter(waiter);
        continue;
      }
      if (
        waiter.request.requestToken !== activeRequestToken ||
        waiter.request.routeId !== snapshot.routeId ||
        snapshot.sequence <= waiter.request.afterSequence
      ) {
        continue;
      }
      waiters.delete(waiter.request.requestToken);
      waiter.cleanup();
      waiter.resolve(snapshot);
    }
  };

  return {
    navigate(routeId, requestToken) {
      if (disposed) return;
      activeRequestToken = requestToken;
      for (const waiter of [...waiters.values()]) {
        if (waiter.request.requestToken !== requestToken) rejectWaiter(waiter);
      }
      navigate(routeId);
    },

    getCurrent,

    waitForReady(request) {
      if (disposed || request.signal.aborted) {
        return Promise.reject(new TourNavigationCancelledError());
      }

      if (activeRequestToken !== request.requestToken) {
        return Promise.reject(new TourNavigationCancelledError());
      }

      const current = getCurrent();
      if (
        current.routeId === request.routeId &&
        !current.pending
      ) {
        return Promise.resolve({
          routeId: current.routeId,
          shellRouteId: current.shellRouteId,
          sequence: current.sequence,
        });
      }

      return new Promise<NavigationReadySnapshot>((resolve, reject) => {
        const waiter: Waiter = {
          request,
          resolve,
          reject,
          cleanup: () => request.signal.removeEventListener("abort", onAbort),
        };
        const onAbort = () => rejectWaiter(waiter);
        request.signal.addEventListener("abort", onAbort, { once: true });
        const previous = waiters.get(request.requestToken);
        if (previous) rejectWaiter(previous);
        waiters.set(request.requestToken, waiter);

        if (request.signal.aborted) {
          rejectWaiter(waiter);
        }
      });
    },

    notifyReady(snapshot) {
      if (!disposed) tryResolve(snapshot);
    },

    dispose() {
      if (disposed) return;
      disposed = true;
      activeRequestToken = null;
      for (const waiter of [...waiters.values()]) rejectWaiter(waiter);
    },
  };
}
