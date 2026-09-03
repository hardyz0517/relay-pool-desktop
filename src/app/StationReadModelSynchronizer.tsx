import { useEffect } from "react";
import { isTauri } from "@tauri-apps/api/core";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { useQueryClient } from "@tanstack/react-query";
import {
  DOMAIN_REVISION_UPDATED_EVENT,
  reconcileStationReadModels,
  subscribeToDomainRevisionUpdates,
} from "@/lib/query/stationCollectionQuerySynchronization";

const RECONCILIATION_INTERVAL_MS = 30_000;

/**
 * Installs the process-wide station read-model event bridge. The event is a
 * best-effort latency hint; all state remains owned by React Query and the
 * durable backend projections.
 */
export function StationReadModelSynchronizer() {
  const queryClient = useQueryClient();

  useEffect(() => {
    if (!isTauri()) return;

    let disposed = false;
    let unlisten: UnlistenFn | undefined;
    let intervalId: number | undefined;
    let probeInFlight: Promise<void> | undefined;
    const revisionTracker = new Map<string, number>();

    const reconcile = () => {
      if (disposed || probeInFlight) return;
      probeInFlight = reconcileStationReadModels(queryClient, revisionTracker)
        .then(() => undefined)
        .catch(() => undefined)
        .finally(() => {
          probeInFlight = undefined;
        });
    };
    const reconcileWhenVisible = () => {
      if (document.visibilityState === "visible") reconcile();
    };

    void subscribeToDomainRevisionUpdates(queryClient, undefined, revisionTracker)
      .then((nextUnlisten) => {
        if (disposed) {
          nextUnlisten();
          return;
        }
        unlisten = nextUnlisten;
      })
      // The event is an optimization. A missing native bridge must not stop
      // normal query fetching or prevent the app from becoming usable.
      .catch(() => undefined)
      .finally(() => {
        if (disposed) return;
        document.addEventListener("visibilitychange", reconcileWhenVisible);
        window.addEventListener("pageshow", reconcile);
        intervalId = window.setInterval(reconcile, RECONCILIATION_INTERVAL_MS);
        reconcile();
      });

    return () => {
      disposed = true;
      document.removeEventListener("visibilitychange", reconcileWhenVisible);
      window.removeEventListener("pageshow", reconcile);
      if (intervalId !== undefined) window.clearInterval(intervalId);
      unlisten?.();
    };
  }, [queryClient]);

  return null;
}

export { DOMAIN_REVISION_UPDATED_EVENT };
