import { useEffect } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useQueryClient, type QueryClient } from "@tanstack/react-query";
import { invalidateAlertingReadModels } from "@/lib/query/alertingQuerySynchronization";

export const ALERTING_READ_MODEL_UPDATED_EVENT = "alerting-read-model-updated";

type AlertingReadModelEventSubscriber = (
  event: typeof ALERTING_READ_MODEL_UPDATED_EVENT,
  handler: () => void,
) => Promise<UnlistenFn>;

/**
 * Subscribes before issuing a refresh, closing the startup race between the
 * initial badge query and a background collector commit.
 */
export async function subscribeToAlertingReadModelUpdates(
  queryClient: QueryClient,
  subscribe: AlertingReadModelEventSubscriber = (event, handler) => listen(event, handler),
): Promise<UnlistenFn> {
  const unlisten = await subscribe(ALERTING_READ_MODEL_UPDATED_EVENT, () => {
    void invalidateAlertingReadModels(queryClient);
  });
  await invalidateAlertingReadModels(queryClient);
  return unlisten;
}

export function AlertingReadModelSynchronizer() {
  const queryClient = useQueryClient();

  useEffect(() => {
    if (!isTauri()) {
      return;
    }

    let disposed = false;
    let unlisten: UnlistenFn | undefined;
    void subscribeToAlertingReadModelUpdates(queryClient)
      .then((nextUnlisten) => {
        if (disposed) {
          nextUnlisten();
          return;
        }
        unlisten = nextUnlisten;
      })
      // Desktop event registration is an enhancement. Queries remain usable
      // through their normal fetch lifecycle if the native bridge is absent.
      .catch(() => undefined);

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [queryClient]);

  return null;
}
